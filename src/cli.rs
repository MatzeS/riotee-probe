use std::path::PathBuf;

use crate::probe::Riotee;
use crate::target::Target;
use crate::target::msp430::Msp430;
use crate::{probe, target};
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

            Command::Program { device, firmware } => with_target(riotee, device, |target| {
                target.program(&firmware, progress::indicatif_progress())
            }),
            Command::Reset { device } => with_target(riotee, device, |target| target.reset()),
            Command::Halt { device } => with_target(riotee, device, |target| target.halt()),
            Command::Resume { device } => with_target(riotee, device, |target| target.resume()),

            Command::Gpio { command } => {
                let mut riotee = riotee.try_as_probe()?;
                match command {
                    GpioCommand::Dir { pin, direction } => {
                        riotee.gpio_set_direction(pin, direction.is_out())
                    }
                    GpioCommand::Set { pin, state } => riotee.gpio_set_state(pin, state.is_high()),
                    GpioCommand::Get { pin } => {
                        println!("{}", if riotee.gpio_get(pin)? { "high" } else { "low" });
                        Ok(())
                    }
                }
            }

            Command::Bootloader => riotee.enter_bootloader(),

            Command::List => unreachable!("handled above"),
        }
    }

    fn list() -> Result<()> {
        let devices = probe::list_info();
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

// TODO this function is a strange shuffle around the target parameter.
// This could be more elegant.
fn with_target<K: probe::KindMarker>(
    mut riotee: probe::Riotee<K>,
    device: Device,
    action: impl FnOnce(&mut dyn Target) -> Result<()>,
) -> Result<()> {
    match device {
        Device::Nrf52 => {
            let mut session = target::nrf52::attach(riotee.into_probe_rs())?;
            action(&mut session)
        }
        Device::Msp430 => {
            let mut msp430 = Msp430::connect(riotee.cmsisdap_mut())?;
            action(&mut msp430)
        }
    }
}
