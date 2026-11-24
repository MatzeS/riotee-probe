#![deny(rustdoc::broken_intra_doc_links, rustdoc::bare_urls)]

//! Library and CLI tool to interact with Riotee devices.
//!
//! The `riotee-probe-rs` binary (see `src/bin/main.rs`) provides a CLI tool
//! similar to the original `riotee-probe` tool (<https://pypi.org/project/riotee-probe/>).
//! These features are also programmatically available by the library implementation.

pub mod cli;
pub mod error;
pub mod intelhex;
pub mod target;
pub mod vendor_commands;

use std::any::Any;
use std::marker::PhantomData;

use probe_rs::probe::DebugProbeSelector;
use probe_rs::probe::ProbeFactory;
use probe_rs::probe::cmsisdap::{CmsisDap, CmsisDapFactory};
use probe_rs::probe::list::Lister;

use crate::error::{Error, Result};
use crate::vendor_commands::CmsisDapExt;
pub use crate::vendor_commands::{GPIO_COUNT, GpioConfig, Level};

const RIOTEE_VID: u16 = 0x1209;
const BOARD_PID: u16 = 0xC8A0;
const PROBE_PID: u16 = 0xC8A1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Board,
    Probe,
}

impl Kind {
    fn from_product_id(product_id: u16) -> Option<Self> {
        match product_id {
            BOARD_PID => Some(Kind::Board),
            PROBE_PID => Some(Kind::Probe),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Board => "Riotee Board",
            Kind::Probe => "Riotee Probe",
        }
    }
}

pub trait DebugProbeSelectorExt {
    /// Which Riotee device this selector identifies based on its VID and PID.
    fn riotee_kind(&self) -> Option<Kind>;

    /// True, if the selector points to a Riotee device.
    fn is_riotee(&self) -> bool {
        self.riotee_kind().is_some()
    }
}

impl DebugProbeSelectorExt for DebugProbeSelector {
    fn riotee_kind(&self) -> Option<Kind> {
        let DebugProbeSelector::Usb(usb) = self;
        if usb.vendor_id != RIOTEE_VID {
            return None;
        }
        Kind::from_product_id(usb.product_id)
    }
}

/// List all Riotee devices.
pub fn list() -> Vec<DebugProbeSelector> {
    Lister::new()
        .list_all()
        .into_iter()
        .map(DebugProbeSelector::from)
        .filter(DebugProbeSelectorExt::is_riotee)
        .collect()
}

pub struct RioteeInfo {
    pub selector: DebugProbeSelector,
    pub firmware_version: Option<String>,
}

impl RioteeInfo {
    pub fn kind(&self) -> Kind {
        self.selector
            .riotee_kind()
            .expect("a RioteeInfo is only built from a Riotee selector")
    }

    pub fn serial_number(&self) -> Option<&str> {
        let DebugProbeSelector::Usb(usb) = &self.selector;
        usb.serial_number.as_deref()
    }
}

/// Lists info of all Riotee devices.
///
/// This connects to all devices to query the firmware version.
pub fn list_info() -> Vec<RioteeInfo> {
    list()
        .into_iter()
        .map(|selector| {
            let firmware_version = Riotee::open_device(&selector)
                .ok()
                .and_then(|mut riotee| riotee.firmware_version().ok());
            RioteeInfo {
                selector,
                firmware_version,
            }
        })
        .collect()
}

/// Marker trait for type system guidance of the Riotee type.
pub trait KindMarker {}

/// Marker for type system guidance of `Riotee`.
///
/// This refers to the [Riotee Board](https://www.riotee.nessie-circuits.de/docs/latest/hardware/board.html)
/// device.
#[derive(Clone, Copy, Debug)]
pub struct Board;
impl KindMarker for Board {}

/// Marker for type system guidance of `Riotee`.
///
/// This refers to the [Riotee Probe](https://www.riotee.nessie-circuits.de/docs/latest/hardware/probe.html)
/// device. Not to anything probe-rs related or implementation items.
#[derive(Clone, Copy, Debug)]
pub struct Probe;
impl KindMarker for Probe {}

#[derive(Clone, Copy, Debug)]
pub struct Unknown;
impl KindMarker for Unknown {}

/// Riotee device interface.
///
/// `K` marks which Riotee device is actually connected through the underlying probe connection.
/// This influences the available features of the device (board or probe).
pub struct Riotee<K>
where
    K: KindMarker,
{
    dap: CmsisDap,

    // The actual kind connected through this probe.
    // The `K` type may specialize from Unknown to the corresponding marker
    // which agrees with this value.
    kind: Kind,

    _marker: PhantomData<K>,
}

impl Riotee<Unknown> {
    /// The caller needs to verify that `self` is compatible with whatever `K` this is being
    /// downcast to.
    fn downcast<K: KindMarker>(self) -> Riotee<K> {
        Riotee {
            dap: self.dap,
            kind: self.kind,
            _marker: PhantomData,
        }
    }

    fn require_kind(&self, kind: Kind) -> Result<()> {
        if self.kind != kind {
            return Err(Error::WrongKind {
                expected: kind,
                found: self.kind,
            });
        }
        Ok(())
    }

    pub fn try_as_board(self) -> Result<Riotee<Board>> {
        self.require_kind(Kind::Board)?;
        Ok(self.downcast())
    }

    pub fn try_as_probe(self) -> Result<Riotee<Probe>> {
        self.require_kind(Kind::Probe)?;
        Ok(self.downcast())
    }

    /// Opens the Riotee device identified by `selector`.
    ///
    /// Also, see `open_probe` and `try_as_probe` resp. `open_board` and `try_as_board`.
    pub fn open(selector: &DebugProbeSelector) -> Result<Self> {
        let mut candidates = list();
        candidates.retain(|candidate| selector.matches_selector(candidate));

        match candidates.len() {
            0 => Err(Error::NoMatch {
                selector: selector.clone(),
            }),
            1 => Self::open_device(&candidates[0]),
            _ => Err(Error::Ambiguous {
                selectors: candidates,
            }),
        }
    }

    /// Opens the Riotee device.
    ///
    /// Fails if multiple devices are connected.
    /// In this case, use `open_by_serial` or `open` to identify the device you want to connect to.
    pub fn open_unique() -> Result<Self> {
        let candidates = list();

        match candidates.len() {
            0 => Err(Error::NoDevice),
            1 => Self::open_device(&candidates[0]),
            _ => Err(Error::Ambiguous {
                selectors: candidates,
            }),
        }
    }

    fn open_device(selector: &DebugProbeSelector) -> Result<Self> {
        let kind = selector.riotee_kind().ok_or_else(|| Error::NotRiotee {
            selector: selector.clone(),
        })?;
        let probe = CmsisDapFactory
            .open(selector)
            .map_err(|source| Error::OpenProbe { kind, source })?;
        let probe = *(probe as Box<dyn Any>)
            .downcast::<CmsisDap>()
            .expect("CmsisDapFactory only ever yields a CmsisDap");
        Ok(Riotee {
            dap: probe,
            kind,
            _marker: PhantomData,
        })
    }
}

impl<K: KindMarker> Riotee<K> {
    pub fn cmsisdap_mut(&mut self) -> &mut CmsisDap {
        &mut self.dap
    }

    /// Convert into a CMSIS-DAP.
    pub fn into_cmsisdap(self) -> CmsisDap {
        self.dap
    }

    /// Convert into a generic probe-rs Probe (not referring to the Riotee Probe adapter).
    pub fn into_probe_rs(self) -> probe_rs::probe::Probe {
        probe_rs::probe::Probe::new(self.dap)
    }

    pub fn firmware_version(&mut self) -> Result<String> {
        self.dap.send_riotee_command(vendor_commands::Version)
    }

    pub fn set_target_power(&mut self, on: bool) -> Result<()> {
        self.dap.send_riotee_command(vendor_commands::Power { on })
    }

    /// Reboots the probe into its USB bootloader, for firmware updates.
    ///
    /// This is only available for 'recent' probes, where the firmware supports the 'bootloader'
    /// command.
    /// See: <https://github.com/NessieCircuits/Riotee_ProbeSoftware/pull/19>
    pub fn enter_bootloader(&mut self) -> Result<()> {
        self.dap
            .send_riotee_command(vendor_commands::EnterBootloader)
    }
}

impl Riotee<Board> {
    pub fn open_board(selector: &DebugProbeSelector) -> Result<Self> {
        Riotee::open(selector)?.try_as_board()
    }

    pub fn set_bypass(&mut self, on: bool) -> Result<()> {
        self.dap.send_riotee_command(vendor_commands::Bypass { on })
    }
}

impl Riotee<Probe> {
    pub fn open_probe(selector: &DebugProbeSelector) -> Result<Self> {
        Riotee::open(selector)?.try_as_probe()
    }

    pub fn write_gpio(&mut self, pin: u8, config: GpioConfig) -> Result<()> {
        self.dap
            .send_riotee_command(vendor_commands::GpioSet::new(pin, config)?)
    }

    pub fn read_gpio(&mut self, pin: u8) -> Result<Level> {
        self.dap
            .send_riotee_command(vendor_commands::GpioGet::new(pin)?)
    }
}
