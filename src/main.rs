mod cli;
mod intelhex;
mod probe;
mod progress;
mod target;
mod vendor;

use anyhow::Result;
use clap::Parser;

use cli::Cli;

fn main() -> Result<()> {
    Cli::parse().run()
}
