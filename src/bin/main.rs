use anyhow::Result;
use clap::Parser;

use riotee_probe_rs::cli::Cli;

fn main() -> Result<()> {
    Cli::parse().run()
}
