mod arguments;
mod grpc;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use clap::Parser;
use grpc::{
    main_grpc::{
        heartbeat_client::HeartbeatClient, heartbeat_server::HeartbeatServer, HeartbeatRequest,
    },
    Heartbeater,
};
use log::{info, warn};
use tokio::sync::Mutex;
use tonic::transport::Server;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
enum NodeType {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone)]
struct Node {
    address: SocketAddr,
    peers: Arc<Vec<String>>,
    called: u64,
    node_type: NodeType,
    last_heartbeat: DateTime<Utc>,
}
// TODO: the node should be an Arc<Mutex<Node>> so that it can be shared between the client and server threads
// the node will behave like a database, and the client and server threads will be the clients connecting to the database

async fn run(node: Node) -> Result<(), Box<dyn std::error::Error>> {
    let peers = node.peers.clone();

    let node = Arc::new(Mutex::new(node));

    let _node = node.clone();
    let client_thread = tokio::spawn(async move {
        loop {
            let node_type;
            {
                node_type = node.lock().await.node_type.clone();
            }

            match node_type {
                NodeType::Follower => {
                    info!("I'm a follower");

                    let last_heartbeat = node.lock().await.last_heartbeat;

                    let deadline = last_heartbeat + chrono::Duration::seconds(1);

                    info!("Deadline: {:?}", deadline);

                    while Utc::now() < deadline
                        && last_heartbeat == node.lock().await.last_heartbeat
                    {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        info!("Waiting for a heartbeat")
                    }

                    info!("Last heartbeat: {:?}", last_heartbeat);
                    if last_heartbeat == node.lock().await.last_heartbeat {
                        info!("Didn't receive a heartbeat in 2 seconds");
                        info!("I'm a candidate now");
                        node.lock().await.node_type = NodeType::Candidate;
                    }
                }
                NodeType::Candidate => {
                    info!("I'm a candidate");
                }
                NodeType::Leader => {
                    info!("I'm a leader");
                    tokio::time::sleep(Duration::from_secs(1)).await;
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
                        };
                    }
                }
            }
        }
    });

    let node = _node;

    let server_thread = Server::builder()
        .add_service(HeartbeatServer::new(Heartbeater { node: node.clone() }))
        .serve(node.lock().await.address);

    let (client_status, server_status) = tokio::join!(client_thread, server_thread);

    client_status.unwrap();
    server_status.unwrap();

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let peers = Arc::new(args.peers);

    run(Node {
        address: addr,
        peers,
        called: 0,
        node_type: NodeType::Follower,
        last_heartbeat: Utc::now(),
    })
    .await?;

    Ok(())
}
