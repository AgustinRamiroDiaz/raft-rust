mod arguments;
mod grpc;

use std::{net::SocketAddr, sync::Arc};

use clap::Parser;
use grpc::{
    main_grpc::{
        heartbeat_client::HeartbeatClient, heartbeat_server::HeartbeatServer, HeartbeatRequest,
    },
    Heartbeater,
};
use log::{info, warn};
use tonic::transport::Server;

struct Node {
    address: SocketAddr,
    peers: Arc<Vec<String>>,
}

impl Node {
    async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let peers = self.peers.clone();

        let client_thread = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                for peer in &*peers {
                    let mut client = match HeartbeatClient::connect(peer.clone()).await {
                        Ok(client) => client,
                        Err(e) => {
                            warn!("Failed to connect to {}: {:?}", &peer, e);
                            continue;
                        }
                    };

                    let request = tonic::Request::new(HeartbeatRequest {});

                    match client.heartbeat(request).await {
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
            .add_service(HeartbeatServer::new(Heartbeater::default()))
            .serve(self.address);

        let (client_status, server_status) = tokio::join!(client_thread, server_thread);

        client_status.unwrap();
        server_status.unwrap();

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let peers = Arc::new(args.peers);

    Node {
        address: addr,
        peers,
    }
    .run()
    .await?;

    Ok(())
}
