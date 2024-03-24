use crate::{client_trait::RaftClientTrait, log_entry::LogEntry, state_machine::StateMachine};
use log::info;
use rand::{thread_rng, Rng};
use std::{future::Future, net::SocketAddr, time::Duration};

use tokio::time::sleep;

use tokio::{
    sync::watch,
    time::{Instant, Sleep},
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeType {
    Follower,
    Candidate,
    Leader,
}

pub(crate) type HeartBeatEvent = u64;

#[derive(Debug)]
pub(crate) struct Node<SO, PCO, SM, LET> {
    pub(crate) address: SocketAddr,
    pub(crate) peers: Vec<String>,
    pub(crate) node_type: NodeType, // TODO: hide this field so it can only be changed through the change_node_type method
    pub(crate) last_heartbeat: Instant, // TODO: maybe we can remove this field an rely only in the channels
    pub(crate) term: u64,
    pub(crate) last_voted_for_term: Option<u64>,
    pub(crate) heart_beat_event_sender: watch::Sender<HeartBeatEvent>,
    pub(crate) heart_beat_event_receiver: watch::Receiver<HeartBeatEvent>,
    pub(crate) node_type_changed_event_sender: watch::Sender<()>,
    pub(crate) node_type_changed_event_receiver: watch::Receiver<()>,
    pub(crate) _sleep: fn(Duration) -> SO,
    pub(crate) get_candidate_sleep_time: fn() -> Duration,
    pub(crate) get_client: fn(String) -> PCO,
    pub(crate) state_machine: SM,
    pub(crate) log_entries: Vec<LogEntry<LET>>,
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
            _sleep: sleep,
            get_candidate_sleep_time: || Duration::from_millis(thread_rng().gen_range(0..100)),
            node_type_changed_event_receiver,
            node_type_changed_event_sender,
            get_client,
            state_machine,
            log_entries: Vec::new(),
        }
    }
}
