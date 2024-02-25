use tonic::{Request, Response, Status};

use main_grpc::heartbeat_server::Heartbeat;

use main_grpc::{HeartbeatReply, HeartbeatRequest};
pub mod main_grpc {
    tonic::include_proto!("main");
}

use tonic;

use log::info;

#[derive(Debug, Default)]
pub struct Heartbeater {}

#[tonic::async_trait]
impl Heartbeat for Heartbeater {
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status> {
        info!("Got a request: {:?}", request);

        let reply = main_grpc::HeartbeatReply {};

        Ok(Response::new(reply))
    }
}
