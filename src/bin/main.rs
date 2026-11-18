use anyhow::Result;
use clap::Parser;

use riotee_probe::cli::Cli;

fn main() -> Result<()> {
    Cli::parse().run()
}
