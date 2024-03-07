mod arguments;
mod grpc;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use clap::Parser;
use grpc::{
    main_grpc::{
        heartbeat_client::HeartbeatClient, heartbeat_server::HeartbeatServer, HeartbeatRequest,
    },
    Heartbeater,
};
use log::{debug, info, warn};
use rand::{thread_rng, Rng};
use tokio::{
    select,
    sync::{oneshot, watch, Mutex},
    time::{sleep, Instant},
};
use tonic::transport::Server;

use crate::grpc::main_grpc::RequestVoteRequest;

#[derive(Debug, Clone, PartialEq)]
enum NodeType {
    Follower,
    Candidate,
    Leader,
}

pub trait Sleeper: Send + 'static {
    fn sleep(&self, duration: Duration) -> impl std::future::Future<Output = ()> + Send;
}

struct TokioSleeper {}

impl Sleeper for TokioSleeper {
    fn sleep(&self, duration: Duration) -> impl std::future::Future<Output = ()> + Send {
        tokio::time::sleep(duration)
    }
}

type HeartBeatEvent = u64;

#[derive(Debug)]
struct Node<S>
where
    S: Sleeper,
{
    address: SocketAddr,
    peers: Vec<String>,
    node_type: NodeType,
    last_heartbeat: Instant, // TODO: maybe we can remove this field an rely only in the channels
    term: u64,
    last_voted_for_term: Option<u64>,
    heart_beat_event_sender: watch::Sender<HeartBeatEvent>,
    heart_beat_event_receiver: watch::Receiver<HeartBeatEvent>,
    sleep: S,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Ok;
    use tokio::spawn;

    use super::*;

    struct MockSleeper<F>
    where
        F: Fn(Duration) -> Duration,
    {
        modify_duration: F,
    }

    impl<F> Sleeper for MockSleeper<F>
    where
        F: Fn(Duration) -> Duration + Send + 'static,
    {
        fn sleep(&self, duration: Duration) -> impl std::future::Future<Output = ()> + Send {
            tokio::time::sleep((self.modify_duration)(duration))
        }
    }

    #[tokio::test]
    async fn becomes_candidate_when_no_heartbeats() -> anyhow::Result<()> {
        let socket = "[::1]:50000".parse()?;
        let (heart_beat_event_sender, heart_beat_event_receiver) = watch::channel(0);

        let node = Node {
            address: socket,
            peers: vec![],
            term: 0,
            last_heartbeat: Instant::now(),
            node_type: NodeType::Follower,
            last_voted_for_term: None,
            heart_beat_event_sender,
            heart_beat_event_receiver,
            sleep: MockSleeper {
                modify_duration: |x| 0 * x,
            },
        };
        let sleeper = MockSleeper {
            modify_duration: |x| 0 * x,
        };

        let node = Arc::new(Mutex::new(node));

        spawn(Node::run(node.clone(), sleeper));

        sleep(Duration::from_millis(100)).await;

        assert_ne!(node.lock().await.node_type, NodeType::Follower);

        Ok(())
    }
}

impl Node<TokioSleeper> {
    fn new(address: SocketAddr, peers: Vec<String>) -> Self {
        let (heart_beat_event_sender, heart_beat_event_receiver) = watch::channel(0);

        Self {
            address,
            peers,
            node_type: NodeType::Follower,
            last_heartbeat: Instant::now(),
            term: 0,
            last_voted_for_term: None,
            heart_beat_event_sender,
            heart_beat_event_receiver,
            sleep: TokioSleeper {},
        }
    }
}

impl<S> Node<S>
where
    S: Sleeper,
{
    async fn run(node: Arc<Mutex<Self>>, sleeper: impl Sleeper) -> anyhow::Result<()> {
        let peers = node.lock().await.peers.clone();

        let _node = node.clone();
        let client_thread = tokio::spawn(async move {
            loop {
                let node_type = node.lock().await.node_type.clone();
                let _node = node.clone();
                let (tx, rx) = oneshot::channel::<()>(); // Node type change signal
                tokio::spawn(async move {
                    let node = _node;
                    let last_node_type = node.lock().await.node_type.clone();
                    loop {
                        sleep(Duration::from_millis(10)).await;
                        if last_node_type != node.lock().await.node_type {
                            tx.send(()).unwrap_or_default();
                            break;
                        }
                    }
                });

                match node_type {
                    NodeType::Follower => {
                        info!("I'm a follower");

                        info!("Waiting for heartbeats");

                        let mut receiver = node.lock().await.heart_beat_event_receiver.clone();

                        info!("Last heartbeat seen {}! ", *receiver.borrow_and_update());
                        select! {
                            _ = sleeper.sleep(Duration::from_secs(2)) => {
                                warn!("Didn't receive a heartbeat in 2 seconds");
                                node.lock().await.node_type = NodeType::Candidate;
                            }
                            _ = receiver.changed() => {
                                info!("Heartbeat event received");
                            }
                        }
                    }
                    NodeType::Candidate => {
                        info!("I'm a candidate");

                        {
                            let mut node = node.lock().await;
                            node.term += 1;
                            node.last_voted_for_term = Some(node.term);
                        }

                        let mut total_votes = 1; // I vote for myself
                        for peer in &peers {
                            let mut client = match HeartbeatClient::connect(peer.clone()).await {
                                Ok(client) => client,
                                Err(e) => {
                                    warn!("Failed to connect to {}: {:?}", &peer, e);
                                    continue;
                                }
                            };

                            let request = tonic::Request::new(RequestVoteRequest {
                                term: node.lock().await.term,
                            });

                            match client.request_vote(request).await {
                                Ok(response) => {
                                    debug!("RESPONSE={:?}", response);
                                    if response.get_ref().vote_granted {
                                        info!("I got a vote!");
                                        total_votes += 1;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to send request: {:?}", e);
                                }
                            };
                        }

                        if total_votes > (peers.len() + 1) / 2 {
                            info!("I'm a leader now");
                            node.lock().await.node_type = NodeType::Leader;
                        } else {
                            info!("Waiting to request votes again");
                            select! {
                                _ = rx => {
                                    info!("Node type changed");
                                }
                                _ = tokio::time::sleep(Duration::from_millis( thread_rng().gen_range(90..110))) => {
                                }
                            }
                        }
                    }
                    NodeType::Leader => {
                        info!("I'm a leader");
                        info!("Sending heartbeats");
                        for peer in &peers {
                            let mut client = match HeartbeatClient::connect(peer.clone()).await {
                                Ok(client) => client,
                                Err(e) => {
                                    warn!("Failed to connect to {}: {:?}", &peer, e);
                                    continue;
                                }
                            };

                            let request = tonic::Request::new(HeartbeatRequest {
                                term: node.lock().await.term,
                            });

                            match client.heartbeat(request).await {
                                Ok(response) => {
                                    debug!("RESPONSE={:?}", response);
                                }
                                Err(e) => {
                                    warn!("Failed to send request: {:?}", e);
                                }
                            };
                        }

                        info!("Waiting to send heartbeats again");
                        select! {
                            _ = rx => {
                                info!("Node type changed");
                            }
                            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                            }
                        }
                    }
                }
            }
        });

        let node = _node;

        let server_thread = Server::builder()
            .add_service(HeartbeatServer::new(Heartbeater { node: node.clone() }))
            .serve(node.lock().await.address);

        let (client_status, server_status) = tokio::join!(client_thread, server_thread);

        client_status.unwrap();
        server_status.unwrap();

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let peers = args.peers;

    Node::run(
        Arc::new(Mutex::new(Node::new(addr, peers))),
        TokioSleeper {},
    )
    .await?;

    Ok(())
}
