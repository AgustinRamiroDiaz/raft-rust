use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    #[arg(short, long, default_value = "[::1]:50051")]
    pub(crate) address: String,
    #[arg(short, long)]
    pub(crate) peers: Vec<String>,
}
