use std::path::PathBuf;

use crate::target::msp430::Msp430Session;
use crate::target::nrf52::Nrf52Session;
use crate::target::{ProgramOptions, Target as TargetSession};
use crate::{GpioConfig, KindMarker, Level, Riotee};
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use probe_rs::probe::DebugProbeSelector;

pub mod progress;

#[derive(Parser)]
#[command(
    name = "riotee-probe-rs",
    version,
    about = "Control Riotee probes and boards"
)]
pub struct Cli {
    /// Riotee device to use, in probe-rs's `VID:PID[-Interface][:Serial]` syntax
    #[arg(long, global = true)]
    pub probe: Option<DebugProbeSelector>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Switch the power-bypass measurement switch (Riotee Board only)
    Bypass {
        #[arg(value_enum)]
        state: Switch,
    },
    /// Switch the target's fixed power supply
    TargetPower {
        #[arg(value_enum)]
        state: Switch,
    },
    /// Flash firmware onto the target
    Program {
        #[arg(short, long, value_enum, default_value_t = Target::Nrf52)]
        target: Target,
        /// Firmware image: an ELF file for the nRF52, an Intel HEX file for the MSP430
        firmware: PathBuf,
        /// Do not read the firmware back to check it was written correctly
        #[arg(long)]
        skip_verify: bool,
    },
    /// Reset the target
    Reset {
        #[arg(short, long, value_enum, default_value_t = Target::Nrf52)]
        target: Target,
    },
    /// Halt the target
    Halt {
        #[arg(short, long, value_enum, default_value_t = Target::Nrf52)]
        target: Target,
    },
    /// Resume the target
    Resume {
        #[arg(short, long, value_enum, default_value_t = Target::Nrf52)]
        target: Target,
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
    /// Drive a pin high or low, or make it an input
    Set {
        /// Pin to configure
        pin: u8,
        #[arg(value_enum)]
        config: PinConfig,
    },
    /// Read a pin's state
    Get {
        /// Pin to read
        pin: u8,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Target {
    Nrf52,
    Msp430,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

impl From<Switch> for bool {
    fn from(state: Switch) -> Self {
        match state {
            Switch::On => true,
            Switch::Off => false,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PinConfig {
    High,
    Low,
    Input,
}

impl From<PinConfig> for GpioConfig {
    fn from(config: PinConfig) -> Self {
        match config {
            PinConfig::High => GpioConfig::Output(Level::High),
            PinConfig::Low => GpioConfig::Output(Level::Low),
            PinConfig::Input => GpioConfig::Input,
        }
    }
}

impl Cli {
    pub fn run(self) -> Result<()> {
        if matches!(self.command, Command::List) {
            return print_devices();
        }

        // Create riotee probe for all probe related commands
        let mut riotee = match &self.probe {
            Some(selector) => Riotee::open(selector)?,
            None => Riotee::open_unique()?,
        };

        let result = match self.command {
            Command::Bypass { state } => riotee.try_as_board()?.set_bypass(state.into()),
            Command::TargetPower { state } => riotee.set_target_power(state.into()),

            Command::Program {
                target,
                firmware,
                skip_verify,
            } => {
                let options = ProgramOptions {
                    verify: !skip_verify,
                };
                open_target(riotee, target)?.program(
                    &firmware,
                    options,
                    progress::indicatif_progress(),
                )
            }
            Command::Reset { target } => open_target(riotee, target)?.reset(),
            Command::Halt { target } => open_target(riotee, target)?.halt(),
            Command::Resume { target } => open_target(riotee, target)?.resume(),

            Command::Gpio { command } => {
                let mut riotee = riotee.try_as_probe()?;
                match command {
                    GpioCommand::Set { pin, config } => riotee.write_gpio(pin, config.into()),
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
        };
        result?;
        Ok(())
    }
}

fn print_devices() -> Result<()> {
    let devices = crate::list_info();
    if devices.is_empty() {
        eprintln!("no Riotee device is connected");
        return Ok(());
    }

    println!("{:<14}{:<10}Selector", "Kind", "Firmware");
    for device in devices {
        let firmware = device.firmware_version.as_deref().unwrap_or("unknown");
        println!(
            "{:<14}{:<10}{}",
            device.kind().name(),
            firmware,
            device.selector
        );
    }
    Ok(())
}

fn open_target<K: KindMarker>(riotee: Riotee<K>, target: Target) -> Result<Box<dyn TargetSession>> {
    Ok(match target {
        Target::Nrf52 => Box::new(Nrf52Session::attach(riotee.into_probe_rs())?),
        Target::Msp430 => Box::new(Msp430Session::connect(riotee.into_cmsisdap())?),
    })
}
