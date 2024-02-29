use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use main_grpc::heartbeat_server::Heartbeat;

use main_grpc::{HeartbeatReply, HeartbeatRequest};
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

        self.node.lock().await.called += 1;

        info!("Called: {}", self.node.lock().await.called);

        self.node.lock().await.last_heartbeat = chrono::Utc::now();

        let reply = main_grpc::HeartbeatReply {};

        Ok(Response::new(reply))
    }
}
