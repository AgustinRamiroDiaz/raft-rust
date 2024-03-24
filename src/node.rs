use crate::server::main_grpc::RequestVoteRequest;
use crate::{
    client_trait::RaftClientTrait,
    log_entry::LogEntry,
    server::{
        main_grpc::{raft_client::RaftClient, raft_server::RaftServer, HeartbeatRequest},
        RaftServerNode,
    },
    state_machine::{HashMapStateMachineEvent, StateMachine},
};
use log::{debug, info, warn};
use rand::{thread_rng, Rng};
use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    select,
    sync::{watch, Mutex},
    time::{Instant, Sleep},
};
use tonic::transport::{Channel, Server};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeType {
    Follower,
    Candidate,
    Leader,
}

pub(crate) type HeartBeatEvent = u64;

#[derive(Debug)]
pub(crate) struct Node<SO, PCO, SM, LET> {
    address: SocketAddr,
    peers: Vec<String>,
    node_type: NodeType, // TODO: hide this field so it can only be changed through the change_node_type method
    pub(crate) last_heartbeat: Instant, // TODO: maybe we can remove this field an rely only in the channels
    pub(crate) term: u64,
    pub(crate) last_voted_for_term: Option<u64>,
    pub(crate) heart_beat_event_sender: watch::Sender<HeartBeatEvent>,
    pub(crate) heart_beat_event_receiver: watch::Receiver<HeartBeatEvent>,
    node_type_changed_event_sender: watch::Sender<()>,
    node_type_changed_event_receiver: watch::Receiver<()>,
    pub(crate) _sleep: fn(Duration) -> SO,
    get_candidate_sleep_time: fn() -> Duration,
    get_client: fn(String) -> PCO,
    state_machine: SM,
    log_entries: Vec<LogEntry<LET>>,
}

impl<SO, PCO, SM, LET> Node<SO, PCO, SM, LET> {
    pub(crate) fn change_node_type(
        &mut self,
        node_type: NodeType,
    ) -> Result<(), watch::error::SendError<()>> {
        if self.node_type != node_type {
            info!(
                "Node type changed from {:?} to {:?}",
                self.node_type, node_type
            );
            self.node_type = node_type;
            return self.node_type_changed_event_sender.send(());
        }

        Ok(())
    }
}

impl<PCO, SM, LET, F> Node<Sleep, F, SM, LET>
where
    SM: StateMachine<Event = LET> + Sync,
    F: Future<Output = Result<PCO, tonic::transport::Error>>,
    PCO: RaftClientTrait,
{
    pub(crate) fn new(
        address: SocketAddr,
        peers: Vec<String>,
        get_client: fn(String) -> F,
        state_machine: SM,
    ) -> Self {
        let (heart_beat_event_sender, heart_beat_event_receiver) = watch::channel(0);
        let (node_type_changed_event_sender, node_type_changed_event_receiver) = watch::channel(());

        Self {
            address,
            peers,
            node_type: NodeType::Follower,
            last_heartbeat: Instant::now(),
            term: 0,
            last_voted_for_term: None,
            heart_beat_event_sender,
            heart_beat_event_receiver,
            _sleep: tokio::time::sleep,
            get_candidate_sleep_time: || Duration::from_millis(thread_rng().gen_range(0..100)),
            node_type_changed_event_receiver,
            node_type_changed_event_sender,
            get_client,
            state_machine,
            log_entries: Vec::new(),
        }
    }
}

impl<SO, PCO, SM> Node<SO, PCO, SM, HashMapStateMachineEvent<u64, u64>>
where
    SO: Future<Output = ()> + Send + 'static,
    PCO: Future<Output = Result<RaftClient<Channel>, tonic::transport::Error>> + 'static + Send,
    SM: StateMachine<Event = HashMapStateMachineEvent<u64, u64>> + Send + 'static,
{
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

        let mut should_send_put_event = false;
        loop {
            // Temporarilly sending put events manually the first time each node becomes a leader
            // Eventually we'll have a load balancer that will send events to the leader
            let node_type = node.lock().await.node_type.clone();

            match node_type {
                NodeType::Follower => {
                    info!("I'm a follower");

                    info!("Waiting for heartbeats");

                    info!(
                        "Last heartbeat seen {}! ",
                        *heart_beat_event_receiver.borrow_and_update()
                    );
                    select! {
                        _ = _sleep(Duration::from_secs(2)) => {
                            warn!("Didn't receive a heartbeat in 2 seconds");
                            node.lock().await.change_node_type(NodeType::Candidate).unwrap();
                        }
                        _ = heart_beat_event_receiver.changed() => {
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
                        should_send_put_event = true;
                        node.lock()
                            .await
                            .change_node_type(NodeType::Leader)
                            .unwrap();
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
                NodeType::Leader => {
                    info!("I'm a leader");
                    info!("Sending heartbeats");

                    // Temporarilly sending put events manually the first time each node becomes a leader
                    // Eventually we'll have a load balancer that will send events to the leader
                    let entries_to_send = if should_send_put_event {
                        should_send_put_event = false;
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
                    for peer in &peers {
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
            }
        }
    }

    pub(crate) async fn run(node: Arc<Mutex<Self>>) -> anyhow::Result<()> {
        let _node = node.clone();
        let client_thread = tokio::spawn(Self::run_client(_node.clone()));

        let server_thread = Server::builder()
            .add_service(RaftServer::new(RaftServerNode { node: node.clone() }))
            .serve(node.lock().await.address);

        let (client_status, server_status) = tokio::join!(client_thread, server_thread);

        client_status?;
        server_status?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{collections::HashMap, sync::Arc, vec};

    use anyhow::Ok;
    use tokio::{spawn, time::sleep};

    use crate::state_machine::HashMapStateMachineEvent;

    use super::*;

    #[tokio::test]
    async fn becomes_candidate_when_no_heartbeats() -> anyhow::Result<()> {
        spawn(async {
            sleep(Duration::from_millis(100)).await;
            panic!("Test is taking too long, probably a deadlock")
        });

        let socket = "[::1]:50000".parse()?;
        let (heart_beat_event_sender, heart_beat_event_receiver) = watch::channel(0);
        let (node_type_changed_event_sender, node_type_changed_event_receiver) = watch::channel(());

        let get_client = RaftClient::connect;
        let my_state_machine: HashMap<u64, u64> = HashMap::new();

        let log_entries: Vec<LogEntry<HashMapStateMachineEvent<u64, u64>>> = vec![];
        let _sleep = |_| async {};
        let node = Node {
            address: socket,
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

        spawn(Node::run(node.clone()));

        node_type_changed_event_receiver.changed().await?;

        assert_ne!(node.lock().await.node_type, NodeType::Follower);

        Ok(())
    }
}
