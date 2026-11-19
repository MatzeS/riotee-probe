use std::marker::PhantomData;

use anyhow::{Context, Result, bail};
use probe_rs::probe::Probe as RawProbe;
use probe_rs::probe::list::Lister;
use probe_rs::probe::{DebugProbeSelector, UsbSelector};

use crate::vendor_commands::{self, CmsisDapExt, ProbeExt};

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

    fn product_id(self) -> u16 {
        match self {
            Kind::Board => BOARD_PID,
            Kind::Probe => PROBE_PID,
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
    /// Which Riotee device this selector identifies based on the VID.
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
    pub kind: Kind,
    pub serial_number: String,
    pub firmware_version: Option<String>,
}

impl RioteeInfo {
    /// Convert the info to a selector used to open the device.
    pub fn to_selector(&self) -> DebugProbeSelector {
        DebugProbeSelector::Usb(UsbSelector {
            vendor_id: RIOTEE_VID,
            product_id: self.kind.product_id(),
            interface: None,
            serial_number: Some(self.serial_number.clone()),
            usb_location: None,
        })
    }
}

/// Lists info of all Riotee devices.
///
/// This connects to all devices to query the firmware version.
pub fn list_info() -> Vec<RioteeInfo> {
    list()
        .into_iter()
        .map(|selector| {
            let kind = selector
                .riotee_kind()
                .expect("list() only returns Riotee selectors");
            let DebugProbeSelector::Usb(usb) = &selector;
            let serial_number = usb.serial_number.clone().unwrap_or_default();
            let firmware_version = Riotee::open(selector)
                .ok()
                .and_then(|mut riotee| riotee.firmware_version().ok());
            RioteeInfo {
                kind,
                serial_number,
                firmware_version,
            }
        })
        .collect()
}

/// Marker trait for type system guidance of the Riotee type.
pub trait KindMarker {}

#[derive(Clone, Copy, Debug)]
pub struct Board;
impl KindMarker for Board {}

/// Marker for type system guidance of `Riotee`.
///
/// This referes to the riotee probe device (TODO add link).
/// Not to anything probe-rs related or implementation items.
#[derive(Clone, Copy, Debug)]
pub struct Probe;
impl KindMarker for Probe {}

#[derive(Clone, Copy, Debug)]
pub struct Unknown;
impl KindMarker for Unknown {}

/// Riotee device interface.
///
/// `K` marks which Riotee device is actually connected though the undelying probe connection.
/// This influences the available features of the device (board or probe).
pub struct Riotee<K>
where
    K: KindMarker,
{
    probe: RawProbe,

    // The actual kind connected through this probe.
    // The `K` type may specialize from Unknown to the corresponding marker
    // which agrees with this value.
    kind: Kind,

    _marker: PhantomData<K>,
}

impl Riotee<Unknown> {
    /// The caller needs to verify that `self` is compaible with what ever `K` this is being
    /// downcast to.
    unsafe fn downcast<K: KindMarker>(self) -> Riotee<K> {
        Riotee {
            probe: self.probe,
            kind: self.kind,
            _marker: PhantomData,
        }
    }

    pub fn try_as_board(self) -> Result<Riotee<Board>> {
        if self.kind != Kind::Board {
            bail!("not a board");
        }

        // Safety: self is a board
        Ok(unsafe { self.downcast() })
    }

    pub fn try_as_probe(self) -> Result<Riotee<Probe>> {
        if self.kind != Kind::Probe {
            bail!("not a probe");
        }

        // Safety: self is a probe
        Ok(unsafe { self.downcast() })
    }

    /// Opens the Riotee device identified by `selector`.
    ///
    /// Also, see `open_probe` and `try_as_probe` resp. `open_board` and `try_as_board`.
    pub fn open(selector: DebugProbeSelector) -> Result<Self> {
        let kind = selector
            .riotee_kind()
            .context("selector does not identify a Riotee probe or board")?;
        let probe = Lister::new()
            .open(selector)
            .with_context(|| format!("opening {}", kind.name()))?;
        Ok(Riotee {
            probe,
            kind,
            _marker: PhantomData,
        })
    }

    /// Opens the Riotee device with the given serial number.
    pub fn open_by_serial(serial: &str) -> Result<Riotee<Unknown>> {
        let selector = list()
            .into_iter()
            .find(|selector| {
                let DebugProbeSelector::Usb(usb) = selector;
                usb.serial_number.as_deref() == Some(serial)
            })
            .with_context(|| {
                format!("no connected Riotee device has serial number \"{serial}\"")
            })?;

        Riotee::<Unknown>::open(selector)
    }

    /// Opens the Riotee device.
    ///
    /// Fails if multiple devices are connected.
    /// In this case, use `open_by_serial` or `open` to identify the device you want to connect to.
    pub fn open_unique() -> Result<Riotee<Unknown>> {
        let mut candidates = list();

        let selector = match candidates.len() {
            0 => bail!("no Riotee probe or board is connected"),
            1 => candidates.pop().unwrap(),
            n => {
                let names = candidates
                    .iter()
                    .map(|selector| {
                        let DebugProbeSelector::Usb(usb) = selector;
                        format!(
                            "  {} (serial {})",
                            selector
                                .riotee_kind()
                                .expect("list() only returns Riotee selectors")
                                .name(),
                            usb.serial_number.as_deref().unwrap_or("unknown")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                bail!(
                    "{n} Riotee devices are connected, pass --probe <serial> to pick one:\n{names}"
                );
            }
        };

        Riotee::<Unknown>::open(selector)
    }
}

impl<K: KindMarker> Riotee<K> {
    pub fn probe_mut(&mut self) -> &mut RawProbe {
        &mut self.probe
    }

    pub fn into_probe(self) -> RawProbe {
        self.probe
    }

    /// Firmware version string reported by the probe.
    pub fn firmware_version(&mut self) -> Result<String> {
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::Version)
    }

    /// Switches the target power supply on or off.
    pub fn set_target_power(&mut self, on: bool) -> Result<()> {
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::Power { on })
    }

    /// Reboots the probe into its USB bootloader, for firmware updates.
    ///
    /// This is only available for 'recent' probes, where the firmware supports the 'bootloader'
    /// command.
    /// See: https://github.com/NessieCircuits/Riotee_ProbeSoftware/pull/19
    pub fn enter_bootloader(&mut self) -> Result<()> {
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::EnterBootloader)
    }
}

impl Riotee<Board> {
    pub fn open_board(selector: DebugProbeSelector) -> Result<Self> {
        Riotee::open(selector)?.try_as_board()
    }

    /// Switches the power-bypass measurement switch.
    pub fn set_bypass(&mut self, on: bool) -> Result<()> {
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::Bypass { on })
    }
}

impl Riotee<Probe> {
    pub fn open_probe(selector: DebugProbeSelector) -> Result<Self> {
        Riotee::open(selector)?.try_as_probe()
    }

    /// Configures a GPIO pin's direction.
    pub fn gpio_set_direction(&mut self, pin: u8, output: bool) -> Result<()> {
        // Defaults an output pin to driving low, matching the Python tool's behavior.
        let state = if output {
            vendor_commands::GpioState::Low
        } else {
            vendor_commands::GpioState::In
        };
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::GpioSet { pin, state })
    }

    /// Drives a GPIO pin high or low.
    pub fn gpio_set_state(&mut self, pin: u8, high: bool) -> Result<()> {
        let state = if high {
            vendor_commands::GpioState::High
        } else {
            vendor_commands::GpioState::Low
        };
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::GpioSet { pin, state })
    }

    /// Reads a GPIO pin's state.
    pub fn gpio_get(&mut self, pin: u8) -> Result<bool> {
        self.probe
            .try_as_cmsisdap()?
            .send_riotee_command(vendor_commands::GpioGet { pin })
    }
}
