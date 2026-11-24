//! The CMSIS-DAP custom vendor commands for Riotee devices.
//!
//! See
//! https://github.com/NessieCircuits/Riotee_ProbeSoftware
//! `firmware/src/probe_vendor.c`

use anyhow::{Context, Result, bail};
use probe_rs::probe::cmsisdap::{CmsisDap, VendorCommandId};

const DAP_OK: u8 = 0x00;
const DAP_PACKET_SIZE: usize = 64;

/// Riotee defined vendor command IDs.
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

impl CommandByte {
    const fn id(self) -> VendorCommandId {
        VendorCommandId::new(self as u8).expect("riotee command ids are in the vendor range")
    }
}

pub trait Command {
    /// Command byte of a CMSIS-DAP vendor command.
    const BYTE: VendorCommandId;

    type Response;

    fn encode_payload(&self) -> Vec<u8>;

    fn parse_response(&self, data: &mut &[u8]) -> Result<Self::Response>;
}

fn read_exact<'a>(data: &mut &'a [u8], len: usize) -> Result<&'a [u8]> {
    if data.len() < len {
        bail!(
            "expected {len} more response bytes, but {} remain",
            data.len()
        );
    }
    let (head, tail) = data.split_at(len);
    *data = tail;
    Ok(head)
}

fn read_all<'a>(data: &mut &'a [u8]) -> &'a [u8] {
    read_exact(data, data.len()).expect("len bytes are safe to consume")
}

fn read_u8(data: &mut &[u8]) -> Result<u8> {
    let data = read_exact(data, 1)?;
    Ok(data[0])
}

fn read_u16(data: &mut &[u8]) -> Result<u16> {
    let bytes = read_exact(data, size_of::<u16>())?;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("read_exact returned two bytes"),
    ))
}

fn check_empty(data: &[u8]) -> Result<()> {
    if !data.is_empty() {
        bail!(
            "response not fully parsed, {} bytes remain: {data:02X?}",
            data.len()
        );
    }
    Ok(())
}

pub trait CmsisDapExt {
    fn send_riotee_command<C: Command>(&mut self, command: C) -> Result<C::Response>;
}

impl CmsisDapExt for CmsisDap {
    fn send_riotee_command<C: Command>(&mut self, command: C) -> Result<C::Response> {
        let response = self
            .send_vendor_command(C::BYTE, &command.encode_payload())
            .with_context(|| format!("sending vendor command 0x{:02X}", C::BYTE.get()))?;

        let mut data = response.as_slice();
        let status = read_u8(&mut data).context("probe sent an empty vendor command response")?;
        if status != DAP_OK {
            bail!(
                "probe returned error code 0x{status:02X} for vendor command 0x{:02X}",
                C::BYTE.get()
            );
        }

        let parsed = command.parse_response(&mut data).with_context(|| {
            format!(
                "parsing the response of vendor command 0x{:02X}",
                C::BYTE.get()
            )
        })?;
        check_empty(data).with_context(|| {
            format!(
                "parsing the response of vendor command 0x{:02X}",
                C::BYTE.get()
            )
        })?;
        Ok(parsed)
    }
}

/// Helper macro to implement commands without paylaod or response data.
macro_rules! unit_command {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name;

        impl Command for $name {
            const BYTE: VendorCommandId = CommandByte::$name.id();
            type Response = ();

            fn encode_payload(&self) -> Vec<u8> {
                Vec::new()
            }

            fn parse_response(&self, _data: &mut &[u8]) -> Result<()> {
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
            const BYTE: VendorCommandId = CommandByte::$name.id();
            type Response = ();

            fn encode_payload(&self) -> Vec<u8> {
                vec![self.on as u8]
            }

            fn parse_response(&self, _data: &mut &[u8]) -> Result<()> {
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
    const BYTE: VendorCommandId = CommandByte::Version.id();
    type Response = String;

    fn encode_payload(&self) -> Vec<u8> {
        Vec::new()
    }

    fn parse_response(&self, data: &mut &[u8]) -> Result<String> {
        let version = read_all(data);
        // Firmware versions before 1.1.0 send a trailing NUL over the wire.
        let trimmed = version
            .iter()
            .position(|&b| b == 0)
            .map_or(version, |nul| &version[..nul]);
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
    const BYTE: VendorCommandId = CommandByte::GpioSet.id();
    type Response = ();

    fn encode_payload(&self) -> Vec<u8> {
        let state = match self.config {
            GpioConfig::Output(Level::Low) => 0,
            GpioConfig::Output(Level::High) => 1,
            GpioConfig::Input => 2,
        };
        vec![self.pin, state]
    }

    fn parse_response(&self, _data: &mut &[u8]) -> Result<()> {
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
    const BYTE: VendorCommandId = CommandByte::GpioGet.id();
    type Response = Level;

    fn encode_payload(&self) -> Vec<u8> {
        vec![self.pin]
    }

    fn parse_response(&self, data: &mut &[u8]) -> Result<Level> {
        let state = read_u8(data)?;
        let level = match state {
            0 => Level::Low,
            1 => Level::High,
            _ => bail!("probe reported an unknown GPIO level 0x{state:02X}"),
        };
        Ok(level)
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
    const BYTE: VendorCommandId = CommandByte::SbwRead.id();
    type Response = Vec<u16>;

    fn encode_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(5);
        payload.extend_from_slice(&self.address.to_le_bytes());
        payload.push(self.count);
        payload
    }

    fn parse_response(&self, data: &mut &[u8]) -> Result<Vec<u16>> {
        (0..self.count).map(|_| read_u16(data)).collect()
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
    const BYTE: VendorCommandId = CommandByte::SbwWrite.id();
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

    fn parse_response(&self, _data: &mut &[u8]) -> Result<()> {
        Ok(())
    }
}
