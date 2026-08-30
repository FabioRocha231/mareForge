use anyhow::Result;
use clap::Parser;

use mareforge_db_migrate::{run, Cli};

#[tokio::main]
async fn main() -> Result<()> {
    run(Cli::parse()).await
}
