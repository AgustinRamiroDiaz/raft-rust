use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};

use crate::server::{
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
    time::Instant,
};
use tonic::transport::Server;

use crate::server::main_grpc::RequestVoteRequest;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeType {
    Follower,
    Candidate,
    Leader,
}

pub trait Sleeper: Send + 'static {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}

#[derive(Clone, Copy)]
pub(crate) struct TokioSleeper {}

impl Sleeper for TokioSleeper {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(duration)
    }
}

pub(crate) type HeartBeatEvent = u64;

#[derive(Debug)]
pub(crate) struct Node<S>
where
    S: Sleeper,
{
    address: SocketAddr,
    peers: Vec<String>,
    pub(crate) node_type: NodeType,
    pub(crate) last_heartbeat: Instant, // TODO: maybe we can remove this field an rely only in the channels
    pub(crate) term: u64,
    pub(crate) last_voted_for_term: Option<u64>,
    pub(crate) heart_beat_event_sender: watch::Sender<HeartBeatEvent>,
    pub(crate) heart_beat_event_receiver: watch::Receiver<HeartBeatEvent>,
    pub(crate) sleeper: S,
    get_candidate_sleep_time: fn() -> Duration,
}

impl Node<TokioSleeper> {
    pub(crate) fn new(address: SocketAddr, peers: Vec<String>) -> Self {
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
            sleeper: TokioSleeper {},
            get_candidate_sleep_time: || Duration::from_millis(thread_rng().gen_range(80..120)),
        }
    }
}

impl<S> Node<S>
where
    S: Sleeper + Clone + Copy + Sync,
{
    pub(crate) async fn run(node: Arc<Mutex<Self>>) -> anyhow::Result<()> {
        let peers = node.lock().await.peers.clone();

        let sleeper = node.lock().await.sleeper.clone();

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
                        sleeper.sleep(Duration::from_millis(10)).await;
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

                        // TODO: review
                        // We are cloning them in order to release the lock, since having references doesn't drop the guard
                        // There might be an alternative: using multiple Arc<Mutex>> for each field, since they all have different purposes and aren't strictly bundled
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
                                _ = sleeper.sleep((node.lock().await.get_candidate_sleep_time)()) => {
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
                            _ = sleeper.sleep(Duration::from_millis(500)) => {
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

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use anyhow::Ok;
    use tokio::{spawn, time::sleep};

    use super::*;

    #[derive(Clone, Copy)]
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
            sleeper: MockSleeper {
                modify_duration: |x| 0 * x,
            },
            get_candidate_sleep_time: || Duration::from_millis(0),
        };

        let node = Arc::new(Mutex::new(node));

        spawn(Node::run(node.clone()));
        spawn(async {
            sleep(Duration::from_millis(100)).await;
            panic!("Test is taking too long, probably a deadlock")
        });

        sleep(Duration::from_millis(10)).await;

        assert_ne!(node.lock().await.node_type, NodeType::Follower);

        Ok(())
    }
}
