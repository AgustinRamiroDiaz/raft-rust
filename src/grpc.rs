use tonic::{Request, Response, Status};

use main_grpc::greeter_server::Greeter;

use main_grpc::{HelloReply, HelloRequest};
pub mod main_grpc {
    tonic::include_proto!("main");
}

use tonic;

use log::{info, warn};

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        info!("Got a request: {:?}", request);

        let reply = main_grpc::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }
}
