mod arguments;
mod client_trait;
mod log_entry;
mod node;
mod server;
mod state_machine;

use std::{collections::HashMap, sync::Arc};

use clap::Parser;
use node::Node;
use server::main_grpc::raft_client::RaftClient;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = arguments::Args::parse();

    let addr = args.address.parse()?;

    let peers = args.peers;

    let peers_clients = RaftClient::connect;

    let my_state_machine: HashMap<u64, u64> = HashMap::new();

    node::Node::run(Arc::new(Mutex::new(Node::new(
        addr,
        peers,
        peers_clients,
        my_state_machine,
    ))))
    .await?;

    Ok(())
}
