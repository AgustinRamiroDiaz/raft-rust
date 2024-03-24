use crate::client_trait::RaftClientTrait;
use crate::node::{HeartBeatEvent, Node, NodeType};
use crate::server::main_grpc::RequestVoteRequest;
use crate::{
    log_entry::LogEntry,
    server::{
        main_grpc::{raft_server::RaftServer, HeartbeatRequest},
        RaftServerNode,
    },
    state_machine::{HashMapStateMachineEvent, StateMachine},
};
use log::{debug, info, warn};
use rand::{thread_rng, Rng};
use std::net::SocketAddr;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::sync::watch::Receiver;
use tokio::{join, spawn};
use tokio::{select, sync::Mutex};
use tonic::transport::Server;

#[derive(PartialEq, Debug)]
enum NodeClientFollowerOutput {
    DidNotReceiveHeartbeat,
    HeartbeatReceived,
}

async fn run_client_follower<SO>(
    heart_beat_event_receiver: &mut Receiver<HeartBeatEvent>,
    _sleep: fn(Duration) -> SO,
) -> NodeClientFollowerOutput
where
    SO: Future<Output = ()>,
{
    info!("I'm a follower");

    info!("Waiting for heartbeats");

    info!(
        "Last heartbeat seen {}! ",
        *heart_beat_event_receiver.borrow_and_update()
    );
    select! {
        _ = _sleep(Duration::from_secs(2)) => {
            warn!("Didn't receive a heartbeat in 2 seconds");
            NodeClientFollowerOutput::DidNotReceiveHeartbeat
        }
        _ = heart_beat_event_receiver.changed() => {
            info!("Heartbeat event received");
            NodeClientFollowerOutput::HeartbeatReceived
        }
    }
}

impl<SO, PCO, SM, RCT> Node<SO, PCO, SM, HashMapStateMachineEvent<u64, u64>>
where
    SO: Future<Output = ()> + Send + 'static,
    PCO: Future<Output = Result<RCT, tonic::transport::Error>> + Send + 'static,
    RCT: RaftClientTrait + Send + 'static,
    SM: StateMachine<Event = HashMapStateMachineEvent<u64, u64>> + Send + 'static,
{
    async fn run_client_candidate(
        node: &Arc<Mutex<Self>>,
        peers: &[String],
        get_client: fn(String) -> PCO,
        node_type_changed_event_receiver: &mut Receiver<()>,
        _sleep: fn(Duration) -> SO,
    ) {
        info!("I'm a candidate");

        {
            let mut node = node.lock().await;
            node.term += 1;
            node.last_voted_for_term = Some(node.term);
        }

        let mut total_votes = 1; // I vote for myself
        for peer in peers {
            let mut client = match get_client(peer.clone()).await {
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
            node.lock().await.change_node_type(NodeType::Leader)
        } else {
            info!("Waiting to request votes again");
            select! {
                _ = node_type_changed_event_receiver.changed() => {
                }
                _ = _sleep((node.lock().await.get_candidate_sleep_time)()) => {
                }
            }
        }
    }

    async fn run_client_leader(
        node: &Arc<Mutex<Self>>,
        peers: &[String],
        get_client: fn(String) -> PCO,
        node_type_changed_event_receiver: &mut Receiver<()>,
        _sleep: fn(Duration) -> SO,
    ) {
        info!("I'm a leader");
        info!("Sending heartbeats");

        // Temporarilly sending put events manually the first time each node becomes a leader
        // Eventually we'll have a load balancer that will send events to the leader
        let entries_to_send = if thread_rng().gen_bool(0.1) {
            let mut node = node.lock().await;
            let term = node.term;
            let command = HashMapStateMachineEvent::Put(term, term);
            let index = node.log_entries.len() as u64;

            node.state_machine.apply(command.clone());

            let entry = LogEntry {
                term,
                index,
                command,
            };
            node.log_entries.push(entry.clone());
            vec![entry.into()]
        } else {
            vec![]
        };
        for peer in peers {
            let mut client = match get_client(peer.clone()).await {
                Ok(client) => client,
                Err(e) => {
                    warn!("Failed to connect to {}: {:?}", &peer, e);
                    continue;
                }
            };

            let request = tonic::Request::new(HeartbeatRequest {
                term: node.lock().await.term,
                entries: entries_to_send.clone(),
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
            _ = node_type_changed_event_receiver.changed() => {
            }
            _ = _sleep(Duration::from_millis(500)) => {
            }
        }
    }

    async fn run_client(node: Arc<Mutex<Self>>) -> anyhow::Error {
        let peers = node.lock().await.peers.clone();
        let get_client = node.lock().await.get_client;

        let _sleep = node.lock().await._sleep;

        let _node = node.clone();
        // TODO: review
        // We are cloning them in order to release the lock, since having references doesn't drop the guard
        // There might be an alternative: using multiple Arc<Mutex>> for each field, since they all have different purposes and aren't strictly bundled
        let mut heart_beat_event_receiver = node.lock().await.heart_beat_event_receiver.clone();
        let mut node_type_changed_event_receiver =
            node.lock().await.node_type_changed_event_receiver.clone();

        loop {
            let node_type = node.lock().await.node_type.clone();

            match node_type {
                NodeType::Follower => {
                    match run_client_follower(&mut heart_beat_event_receiver, _sleep).await {
                        NodeClientFollowerOutput::DidNotReceiveHeartbeat => {
                            node.lock().await.change_node_type(NodeType::Candidate)
                        }
                        NodeClientFollowerOutput::HeartbeatReceived => {}
                    }
                }
                NodeType::Candidate => {
                    Self::run_client_candidate(
                        &node,
                        &peers,
                        get_client,
                        &mut node_type_changed_event_receiver,
                        _sleep,
                    )
                    .await;
                }
                NodeType::Leader => {
                    Self::run_client_leader(
                        &node,
                        &peers,
                        get_client,
                        &mut node_type_changed_event_receiver,
                        _sleep,
                    )
                    .await
                }
            }
        }
    }

    pub(crate) async fn run(node: Arc<Mutex<Self>>, address: SocketAddr) -> anyhow::Result<()> {
        let _node = node.clone();
        let client_thread = spawn(Self::run_client(_node.clone()));

        let server_thread = Server::builder()
            .add_service(RaftServer::new(RaftServerNode { node: node.clone() }))
            .serve(address);

        let (client_status, server_status) = join!(client_thread, server_thread);

        client_status?;
        server_status?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{collections::HashMap, sync::Arc, vec};

    use tokio::{
        spawn,
        sync::watch,
        time::{sleep, Instant},
    };
    use tonic::{Request, Response, Status};

    use crate::{
        client_trait::RaftClientTrait,
        server::main_grpc::{HeartbeatReply, RequestVoteReply},
        state_machine::HashMapStateMachineEvent,
    };

    use super::*;

    #[derive(Clone)]
    struct RaftClientMock<H, RV> {
        _heartbeat: H,
        _request_vote: RV,
    }

    impl<H, RV, FH, FRV> RaftClientTrait for RaftClientMock<H, RV>
    where
        H: Fn(Request<HeartbeatRequest>) -> FH,
        FH: Future<Output = Result<Response<HeartbeatReply>, Status>> + Send,
        RV: Fn(Request<RequestVoteRequest>) -> FRV,
        FRV: Future<Output = Result<Response<RequestVoteReply>, Status>> + Send,
    {
        fn heartbeat(
            &mut self,
            request: impl tonic::IntoRequest<HeartbeatRequest> + Send,
        ) -> impl Future<Output = Result<Response<HeartbeatReply>, Status>> + Send {
            (self._heartbeat)(request.into_request())
        }

        fn request_vote(
            &mut self,
            request: impl tonic::IntoRequest<RequestVoteRequest>,
        ) -> impl Future<Output = Result<Response<RequestVoteReply>, Status>> + Send {
            (self._request_vote)(request.into_request())
        }
    }

    #[tokio::test]
    async fn follower_heartbeats_received() -> anyhow::Result<()> {
        let (heart_beat_event_sender, mut heart_beat_event_receiver) = watch::channel(0);

        heart_beat_event_sender.send_modify(|x| *x += 1);

        let output = run_client_follower(&mut heart_beat_event_receiver, |_| async {
            sleep(Duration::from_millis(100)).await;
            panic!("Test is taking too long, aborting")
        })
        .await;

        assert_eq!(output, NodeClientFollowerOutput::HeartbeatReceived);

        assert_eq!(
            *heart_beat_event_receiver.borrow_and_update(),
            1,
            "Heartbeat event should have been received"
        );

        Ok(())
    }

    #[tokio::test]
    async fn follower_becomes_candidate_when_no_heartbeats_received() -> anyhow::Result<()> {
        let (_do_not_remove_me_because_i_shall_not_be_dropped, mut heart_beat_event_receiver) =
            watch::channel(0);

        let output = run_client_follower(&mut heart_beat_event_receiver, |_| async {}).await;

        assert_eq!(output, NodeClientFollowerOutput::DidNotReceiveHeartbeat);

        Ok(())
    }

    #[tokio::test]
    async fn becomes_candidate_when_no_heartbeats() -> anyhow::Result<()> {
        spawn(async {
            sleep(Duration::from_millis(100)).await;
            panic!("Test is taking too long, probably a deadlock")
        });

        let (heart_beat_event_sender, heart_beat_event_receiver) = watch::channel(0);
        let (node_type_changed_event_sender, node_type_changed_event_receiver) = watch::channel(());

        let get_client = |_| async {
            Ok(RaftClientMock {
                _heartbeat: |_| async { Ok(Response::new(HeartbeatReply {})) },
                _request_vote: |_| async {
                    Ok(Response::new(RequestVoteReply {
                        ..Default::default()
                    }))
                },
            })
        };

        let my_state_machine: HashMap<u64, u64> = HashMap::new();

        let log_entries: Vec<LogEntry<HashMapStateMachineEvent<u64, u64>>> = vec![];
        let _sleep = |_| async {};
        let node = Node {
            peers: vec![],
            term: 0,
            last_heartbeat: Instant::now(),
            node_type: NodeType::Follower,
            last_voted_for_term: None,
            heart_beat_event_sender,
            heart_beat_event_receiver,
            _sleep,
            get_candidate_sleep_time: || Duration::from_millis(0),
            node_type_changed_event_sender,
            node_type_changed_event_receiver,
            get_client,
            state_machine: my_state_machine,
            log_entries,
        };

        let node = Arc::new(Mutex::new(node));
        let mut node_type_changed_event_receiver =
            node.lock().await.node_type_changed_event_receiver.clone();

        spawn(Node::run_client(node.clone()));

        node_type_changed_event_receiver.changed().await?;

        assert_ne!(node.lock().await.node_type, NodeType::Follower);

        Ok(())
    }
}
