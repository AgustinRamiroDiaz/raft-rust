use clap::Parser;
use grpc::{
    main_grpc::{greeter_client::GreeterClient, greeter_server::GreeterServer, HelloRequest},
    MyGreeter,
};
use log::{info, warn};
use tonic::transport::Server;

mod arguments;
mod grpc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let greeter = MyGreeter::default();
    let peers = args.peers;

    let client_thread = tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            for peer in &peers {
                let mut client = match GreeterClient::connect(peer.clone()).await {
                    Ok(client) => client,
                    Err(e) => {
                        warn!("Failed to connect to {}: {:?}", &peer, e);
                        continue;
                    }
                };

                let request = tonic::Request::new(HelloRequest {
                    name: "Tonic".into(),
                });

                match client.say_hello(request).await {
                    Ok(response) => {
                        info!("RESPONSE={:?}", response);
                    }
                    Err(e) => {
                        warn!("Failed to send request: {:?}", e);
                    }
                }
            }
        }
    });

    let server_thread = Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr);

    let (client_status, server_status) = tokio::join!(client_thread, server_thread);

    client_status.unwrap();
    server_status.unwrap();

    Ok(())
}
