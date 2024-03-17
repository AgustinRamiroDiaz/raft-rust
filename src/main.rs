mod arguments;
mod node;
mod server;

use std::sync::Arc;

use clap::Parser;
use node::Node;
use server::main_grpc::heartbeat_client::HeartbeatClient;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let peers = args.peers;

    let peers_clients = |peer| HeartbeatClient::connect(peer);

    node::Node::run(Arc::new(Mutex::new(Node::new(addr, peers, peers_clients)))).await?;

    Ok(())
}
