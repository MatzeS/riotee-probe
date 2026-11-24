use std::path::PathBuf;

use crate::target::Target;
use crate::target::msp430::Msp430;
use crate::target::nrf52::Nrf52;
use crate::{GpioConfig, KindMarker, Level, Riotee};
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

pub mod progress;

#[derive(Parser)]
#[command(
    name = "riotee-probe-rs",
    version,
    about = "Control Riotee probes and boards"
)]
pub struct Cli {
    /// Equivalent to `probe-rs --probe` argument.
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

impl From<Direction> for GpioConfig {
    fn from(direction: Direction) -> Self {
        match direction {
            // The firmware has no "output, level untouched" state, so an output has to pick
            // a level; low, as the Python tool did.
            Direction::Out => GpioConfig::Output(Level::Low),
            Direction::In => GpioConfig::Input,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PinState {
    High,
    Low,
}

impl From<PinState> for Level {
    fn from(state: PinState) -> Self {
        match state {
            PinState::High => Level::High,
            PinState::Low => Level::Low,
        }
    }
}

impl Cli {
    pub fn run(self) -> Result<()> {
        if matches!(self.command, Command::List) {
            return Self::list();
        }

        // Create riotee probe for all probe related commands
        let mut riotee = match self.probe.as_deref() {
            Some(serial) => Riotee::open_by_serial(serial)?,
            None => Riotee::open_unique()?,
        };

        match self.command {
            Command::Bypass { state } => riotee.try_as_board()?.set_bypass(state.is_on()),
            Command::TargetPower { state } => riotee.set_target_power(state.is_on()),

            Command::Program { device, firmware } => {
                open_target(riotee, device)?.program(&firmware, progress::indicatif_progress())
            }
            Command::Reset { device } => open_target(riotee, device)?.reset(),
            Command::Halt { device } => open_target(riotee, device)?.halt(),
            Command::Resume { device } => open_target(riotee, device)?.resume(),

            Command::Gpio { command } => {
                let mut riotee = riotee.try_as_probe()?;
                match command {
                    GpioCommand::Dir { pin, direction } => riotee.write_gpio(pin, direction.into()),
                    GpioCommand::Set { pin, state } => {
                        riotee.write_gpio(pin, GpioConfig::Output(state.into()))
                    }
                    GpioCommand::Get { pin } => {
                        let level = match riotee.read_gpio(pin)? {
                            Level::High => "high",
                            Level::Low => "low",
                        };
                        println!("{level}");
                        Ok(())
                    }
                }
            }

            Command::Bootloader => riotee.enter_bootloader(),

            Command::List => unreachable!("handled above"),
        }
    }

    fn list() -> Result<()> {
        let devices = crate::list_info();
        if devices.is_empty() {
            eprintln!("No Riotee probes or boards connected");
            return Ok(());
        }

        println!("{:<14}{:<24}Firmware", "Kind", "Serial");
        for device in devices {
            let firmware = device.firmware_version.as_deref().unwrap_or("unknown");
            println!(
                "{:<14}{:<24}{}",
                device.kind.name(),
                device.serial_number,
                firmware
            );
        }
        Ok(())
    }
}

fn open_target<K: KindMarker>(riotee: Riotee<K>, device: Device) -> Result<Box<dyn Target>> {
    Ok(match device {
        Device::Nrf52 => Box::new(Nrf52::attach(riotee.into_probe_rs())?),
        Device::Msp430 => Box::new(Msp430::connect(riotee.into_cmsisdap())?),
    })
}
