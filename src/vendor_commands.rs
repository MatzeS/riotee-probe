//! The CMSIS-DAP custom vendor commands for Riotee devices.
//!
//! See
//! https://github.com/NessieCircuits/Riotee_ProbeSoftware
//! `firmware/src/probe_vendor.c`

use anyhow::{Context, Result, bail};
use probe_rs::probe::cmsisdap::{CmsisDap, VendorCommandId};

const DAP_OK: u8 = 0x00;

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

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum GpioState {
    Low = 0,
    High = 1,
    In = 2,
}

#[derive(Clone, Copy, Debug)]
pub struct GpioSet {
    pub pin: u8,
    pub state: GpioState,
}

impl Command for GpioSet {
    const BYTE: CommandByte = CommandByte::GpioSet;
    type Response = ();

    fn encode_payload(&self) -> Vec<u8> {
        vec![self.pin, self.state as u8]
    }

    fn parse_response(&self, _data: &[u8]) -> Result<()> {
        // TODO check the response data is empty, here and everywhere else.
        Ok(())
    }
}

/// Reads a Probe GPIO pin's state.
#[derive(Clone, Copy, Debug)]
pub struct GpioGet {
    pub pin: u8,
}

impl Command for GpioGet {
    const BYTE: CommandByte = CommandByte::GpioGet;
    type Response = bool;

    fn encode_payload(&self) -> Vec<u8> {
        vec![self.pin]
    }

    fn parse_response(&self, data: &[u8]) -> Result<bool> {
        let &byte = data.first().context("probe sent an empty GPIO response")?;
        // TODO check the remaining response data is empty
        Ok(byte != 0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SbwRead {
    pub address: u32,
    pub count: u8,
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
    pub address: u32,
    pub words: &'a [u16],
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
        Ok(())
    }
}
