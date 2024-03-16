mod arguments;
mod node;
mod server;

use std::sync::Arc;

use clap::Parser;
use node::Node;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = arguments::Args::parse();

    // let addr = args.address.parse()?;

    let peers = args.peers;

    // node::Node::run(Arc::new(Mutex::new(Node::new(addr, peers)))).await?;

    Ok(())
}
