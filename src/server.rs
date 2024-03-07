use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use main_grpc::heartbeat_server::Heartbeat;

use main_grpc::{HeartbeatReply, HeartbeatRequest, RequestVoteReply, RequestVoteRequest};
pub mod main_grpc {
    tonic::include_proto!("main");
}

use tonic;

use log::debug;

use crate::{Node, Sleeper};

#[derive(Debug)]
pub struct Heartbeater<S>
where
    S: Sleeper,
{
    pub node: Arc<Mutex<Node<S>>>,
}

#[tonic::async_trait]
impl<S> Heartbeat for Heartbeater<S>
where
    S: Sleeper,
{
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status> {
        debug!("Got a request: {:?}", request);

        let mut node = self.node.lock().await;

        if request.get_ref().term > node.term {
            node.term = request.get_ref().term;
            node.node_type = crate::NodeType::Follower;
        }

        if request.get_ref().term >= node.term {
            node.heart_beat_event_sender.send_modify(|x| *x = *x + 1);
        }

        node.last_heartbeat = tokio::time::Instant::now();

        let reply = main_grpc::HeartbeatReply {};

        Ok(Response::new(reply))
    }

    async fn request_vote(
        &self,
        request: tonic::Request<RequestVoteRequest>,
    ) -> std::result::Result<tonic::Response<RequestVoteReply>, tonic::Status> {
        debug!("Got a request: {:?}", request);

        let mut node = self.node.lock().await;

        if request.get_ref().term > node.term {
            node.term = request.get_ref().term;
            node.node_type = crate::NodeType::Follower;
        }

        let grant_vote = node.last_voted_for_term < Some(request.get_ref().term);

        let reply = RequestVoteReply {
            vote_granted: grant_vote,
            term: node.term,
        };

        Ok(Response::new(reply))
    }
}
