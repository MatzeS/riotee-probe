use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "riotee-probe-rs",
    version,
    about = "Control Riotee probes and boards"
)]
pub struct Cli {
    /// Passed to probe-rs as is. See `--probe` argument on `probe-rs download`
    #[arg(long, global = true)]
    pub probe: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Switch the power-bypass measurement switch (Riotee Board only)
    Bypass {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Switch the target's fixed power supply
    TargetPower {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Flash firmware onto the target
    Program {
        #[arg(short, long, value_enum, default_value_t = Device::Nrf52)]
        device: Device,
        #[arg(short, long)]
        firmware: PathBuf,
    },
    /// Reset the target
    Reset {
        #[arg(short, long, value_enum, default_value_t = Device::Nrf52)]
        device: Device,
    },
    /// Halt the target
    Halt {
        #[arg(short, long, value_enum, default_value_t = Device::Nrf52)]
        device: Device,
    },
    /// Resume the target
    Resume {
        #[arg(short, long, value_enum, default_value_t = Device::Nrf52)]
        device: Device,
    },
    /// Control GPIOs (Riotee Probe only)
    Gpio {
        #[command(subcommand)]
        command: GpioCommand,
    },
    /// List connected Riotee probes and boards
    List,
    /// Reboot the probe into its USB bootloader, for firmware updates
    Bootloader,
}

#[derive(Subcommand)]
pub enum GpioCommand {
    /// Configure a pin's direction
    Dir {
        #[arg(short, long)]
        pin: u8,
        #[arg(short, long, value_enum)]
        direction: Direction,
    },
    /// Drive a pin high or low
    Set {
        #[arg(short, long)]
        pin: u8,
        #[arg(short, long, value_enum)]
        state: PinState,
    },
    /// Read a pin's state
    Get {
        #[arg(short, long)]
        pin: u8,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Device {
    Nrf52,
    Msp430,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn is_on(self) -> bool {
        matches!(self, OnOff::On)
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Direction {
    In,
    Out,
}

impl Direction {
    pub fn is_out(self) -> bool {
        matches!(self, Direction::Out)
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PinState {
    High,
    Low,
}

impl PinState {
    pub fn is_high(self) -> bool {
        matches!(self, PinState::High)
    }
}
