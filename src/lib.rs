//! Library and CLI tool to interact with Riotee devices.
//!
//! The `riotee-probe-rs` binary (see `src/bin/main.rs`) provides a CLI tool
//! similar to the original `riotee-probe` tool (https://pypi.org/project/riotee-probe/).
//! These features are also programmatically available by the library implementation.

pub mod cli;
pub mod intelhex;
pub mod probe;
pub mod target;
pub mod vendor_commands;
