mod cli;
mod config;
mod error;
mod memory;
mod policy;
mod runner;
mod protocol;
mod util;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), error::CliError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = cli::Args::parse();

    let exit = runner::run(args).await?;
    std::process::exit(exit);
}
