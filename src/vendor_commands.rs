//! The CMSIS-DAP custom vendor commands for Riotee devices.
//!
//! See
//! https://github.com/NessieCircuits/Riotee_ProbeSoftware
//! `firmware/src/probe_vendor.c`

use anyhow::{Context, Result, bail};
use probe_rs::probe::cmsisdap::{CmsisDap, VendorCommandId};

const DAP_OK: u8 = 0x00;
const DAP_PACKET_SIZE: usize = 64;

/// Vendor command IDs/bytes.
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum CommandByte {
    Version = 0x80,
    Power = 0x81,
    SbwConnect = 0x82,
    SbwDisconnect = 0x83,
    SbwReset = 0x84,
    SbwHalt = 0x85,
    SbwResume = 0x86,
    SbwRead = 0x87,
    SbwWrite = 0x88,
    GpioSet = 0x89,
    GpioGet = 0x8A,
    Bypass = 0x8B,
    EnterBootloader = 0x8C,
}

pub trait Command {
    const BYTE: CommandByte;
    type Response;

    fn encode_payload(&self) -> Vec<u8>;

    fn parse_response(&self, data: &[u8]) -> Result<Self::Response>;
}

pub trait CmsisDapExt {
    fn send_riotee_command<C: Command>(&mut self, command: C) -> Result<C::Response>;
}

impl CmsisDapExt for CmsisDap {
    fn send_riotee_command<C: Command>(&mut self, command: C) -> Result<C::Response> {
        let id = VendorCommandId::new(C::BYTE as u8)
            .expect("all riotee vendor command ids fall in the 0x80..=0xFE vendor range");

        let response = self
            .send_vendor_command(id, &command.encode_payload())
            .with_context(|| {
                format!(
                    "sending vendor command {:?} (0x{:02X})",
                    C::BYTE,
                    C::BYTE as u8
                )
            })?;

        let (&status, data) = response
            .split_first()
            .context("probe sent an empty vendor command response")?;
        if status != DAP_OK {
            bail!(
                "probe returned error code 0x{status:02X} for command {:?} (0x{:02X})",
                C::BYTE,
                C::BYTE as u8
            );
        }
        command.parse_response(data)
    }
}

/// Helper macro to implement commands without paylaod or response data.
macro_rules! unit_command {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name;

        impl Command for $name {
            const BYTE: CommandByte = CommandByte::$name;
            type Response = ();

            fn encode_payload(&self) -> Vec<u8> {
                Vec::new()
            }

            fn parse_response(&self, _data: &[u8]) -> Result<()> {
                Ok(())
            }
        }
    };
}

unit_command!(SbwConnect);
unit_command!(SbwDisconnect);

// These commands rquire an active SwbSession, hence connect must be send before.
// TODO the interface does not defend against this
unit_command!(SbwReset);
unit_command!(SbwHalt);
unit_command!(SbwResume);

unit_command!(EnterBootloader);

/// Helper macro to implement commands to toggle hardware switches.
macro_rules! toggle_command {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            pub on: bool,
        }

        impl Command for $name {
            const BYTE: CommandByte = CommandByte::$name;
            type Response = ();

            fn encode_payload(&self) -> Vec<u8> {
                vec![self.on as u8]
            }

            fn parse_response(&self, _data: &[u8]) -> Result<()> {
                Ok(())
            }
        }
    };
}

toggle_command!(Power);
toggle_command!(Bypass);

/// Queries the firmware version string.
#[derive(Clone, Copy, Debug)]
pub struct Version;

impl Command for Version {
    const BYTE: CommandByte = CommandByte::Version;
    type Response = String;

    fn encode_payload(&self) -> Vec<u8> {
        Vec::new()
    }

    fn parse_response(&self, data: &[u8]) -> Result<String> {
        // Firmware versions before 1.1.0 send a trailing NUL over the wire.
        let trimmed = data
            .iter()
            .position(|&b| b == 0)
            .map_or(data, |nul| &data[..nul]);
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpioConfig {
    Input,
    Output(Level),
}

pub const GPIO_COUNT: u8 = 4;
fn check_gpio_pin(pin: u8) -> Result<()> {
    if pin >= GPIO_COUNT {
        bail!(
            "GPIO pin {pin} does not exist, the Riotee Probe has {GPIO_COUNT} GPIO pins (0 through {})",
            GPIO_COUNT - 1
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct GpioSet {
    pin: u8,
    config: GpioConfig,
}

impl GpioSet {
    pub fn new(pin: u8, config: GpioConfig) -> Result<Self> {
        check_gpio_pin(pin)?;
        Ok(Self { pin, config })
    }
}

impl Command for GpioSet {
    const BYTE: CommandByte = CommandByte::GpioSet;
    type Response = ();

    fn encode_payload(&self) -> Vec<u8> {
        let state = match self.config {
            GpioConfig::Output(Level::Low) => 0,
            GpioConfig::Output(Level::High) => 1,
            GpioConfig::Input => 2,
        };
        vec![self.pin, state]
    }

    fn parse_response(&self, _data: &[u8]) -> Result<()> {
        // TODO check the response data is empty, here and everywhere else.
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GpioGet {
    pin: u8,
}

impl GpioGet {
    pub fn new(pin: u8) -> Result<Self> {
        check_gpio_pin(pin)?;
        Ok(Self { pin })
    }
}

impl Command for GpioGet {
    const BYTE: CommandByte = CommandByte::GpioGet;
    type Response = Level;

    fn encode_payload(&self) -> Vec<u8> {
        vec![self.pin]
    }

    fn parse_response(&self, data: &[u8]) -> Result<Level> {
        let &byte = data.first().context("probe sent an empty GPIO response")?;
        // TODO check the remaining response data is empty (here and everywehre else)
        Ok(if byte != 0 { Level::High } else { Level::Low })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SbwRead {
    address: u32,
    count: u8,
}

impl SbwRead {
    // The read response packet contains the command byte (1B) and the status byte (1B).
    // Each word is two byte.
    pub const MAX_WORDS: usize = (DAP_PACKET_SIZE - 2) / 2;

    pub fn new(address: u32, count: u8) -> Result<Self> {
        if usize::from(count) > Self::MAX_WORDS {
            bail!(
                "cannot read {count} words at once, a single Spy-Bi-Wire read returns at most {}",
                Self::MAX_WORDS
            );
        }
        Ok(Self { address, count })
    }
}

impl Command for SbwRead {
    const BYTE: CommandByte = CommandByte::SbwRead;
    type Response = Vec<u16>;

    fn encode_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(5);
        payload.extend_from_slice(&self.address.to_le_bytes());
        payload.push(self.count);
        payload
    }

    fn parse_response(&self, data: &[u8]) -> Result<Vec<u16>> {
        let count = self.count as usize;
        if data.len() < count * 2 {
            bail!(
                "probe returned {} bytes for a {count}-word read",
                data.len()
            );
        }
        Ok(data[..count * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&w| u16::from_le_bytes(w))
            .collect())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SbwWrite<'a> {
    address: u32,
    words: &'a [u16],
}

impl<'a> SbwWrite<'a> {
    /// Request layout: `[command id | address | word count | words]`.
    // The write request contains the command byte (1B), the address (4B) and
    // the number of words (1B).
    // Each word is two byte.
    pub const MAX_WORDS: usize = (DAP_PACKET_SIZE - 1 - 4 - 1) / 2;

    pub fn new(address: u32, words: &'a [u16]) -> Result<Self> {
        if words.len() > Self::MAX_WORDS {
            bail!(
                "cannot write {} words at once, a single Spy-Bi-Wire write carries at most {}",
                words.len(),
                Self::MAX_WORDS
            );
        }
        Ok(Self { address, words })
    }
}

impl Command for SbwWrite<'_> {
    const BYTE: CommandByte = CommandByte::SbwWrite;
    type Response = ();

    fn encode_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(5 + self.words.len() * 2);
        payload.extend_from_slice(&self.address.to_le_bytes());
        payload.push(self.words.len() as u8);
        for word in self.words {
            payload.extend_from_slice(&word.to_le_bytes());
        }
        payload
    }

    fn parse_response(&self, _data: &[u8]) -> Result<()> {
        // TODO this (and maybe all) commands should send an OK return code,
        // we hould verify that and bubble it up
        Ok(())
    }
}
