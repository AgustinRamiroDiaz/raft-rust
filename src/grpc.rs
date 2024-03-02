use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use main_grpc::heartbeat_server::Heartbeat;

use main_grpc::{HeartbeatReply, HeartbeatRequest, RequestVoteReply, RequestVoteRequest};
pub mod main_grpc {
    tonic::include_proto!("main");
}

use tonic;

use log::info;

use crate::Node;

#[derive(Debug)]
pub struct Heartbeater {
    pub node: Arc<Mutex<Node>>,
}

#[tonic::async_trait]
impl Heartbeat for Heartbeater {
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status> {
        info!("Got a request: {:?}", request);

        let mut node = self.node.lock().await;

        if request.get_ref().term > node.term {
            node.term = request.get_ref().term;
            node.node_type = crate::NodeType::Follower;
        }

        node.last_heartbeat = chrono::Utc::now();

        let reply = main_grpc::HeartbeatReply {};

        Ok(Response::new(reply))
    }

    async fn request_vote(
        &self,
        request: tonic::Request<RequestVoteRequest>,
    ) -> std::result::Result<tonic::Response<RequestVoteReply>, tonic::Status> {
        info!("Got a request: {:?}", request);

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
