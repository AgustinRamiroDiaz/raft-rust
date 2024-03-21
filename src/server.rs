use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tonic::{Request, Response, Status};

use main_grpc::{
    raft_server::Raft, HeartbeatReply, HeartbeatRequest, RequestVoteReply, RequestVoteRequest,
};
pub mod main_grpc {
    tonic::include_proto!("main");
}

use tonic;

use log::debug;

use crate::{
    node::{LogEntry, Node, NodeType},
    state_machine::HashMapStateMachineEvent,
};

#[derive(Debug)]
pub struct Heartbeater<SO, PCO, SM, LET> {
    pub node: Arc<Mutex<Node<SO, PCO, SM, LET>>>,
}

#[tonic::async_trait]
impl<SO, PCO, SM, LET> Raft for Heartbeater<SO, PCO, SM, LET>
where
    SO: Future<Output = ()> + Send + 'static,
    PCO: Send + 'static,
    SM: Send + 'static,
    LET: Send + 'static,
{
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status> {
        let request = request.into_inner();
        debug!("Got a request: {:?}", request);
        for entry in request.entries {
            debug!(
                "Got an entry: {:?}",
                <main_grpc::Entry as Into<LogEntry<HashMapStateMachineEvent<u64, u64>>>>::into(
                    entry
                )
            );
        }

        let mut node = self.node.lock().await;

        if request.term > node.term {
            node.term = request.term;
        }

        if request.term >= node.term {
            node.change_node_type(NodeType::Follower).unwrap();
            node.heart_beat_event_sender.send_modify(|x| *x = *x + 1);
        }

        node.last_heartbeat = Instant::now();

        let reply = main_grpc::HeartbeatReply {};

        Ok(Response::new(reply))
    }

    async fn request_vote(
        &self,
        request: tonic::Request<RequestVoteRequest>,
    ) -> std::result::Result<tonic::Response<RequestVoteReply>, tonic::Status> {
        debug!("Got a request: {:?}", request);

        let mut node = self.node.lock().await;

        let grant_vote = node.last_voted_for_term < Some(request.get_ref().term);

        let reply = RequestVoteReply {
            vote_granted: grant_vote,
            term: node.term,
        };

        if grant_vote {
            node.last_voted_for_term = Some(request.get_ref().term);
        }

        if request.get_ref().term > node.term {
            node.term = request.get_ref().term;
            node.change_node_type(NodeType::Follower).unwrap();
        }

        Ok(Response::new(reply))
    }
}
