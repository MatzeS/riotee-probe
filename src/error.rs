use std::io;
use std::path::PathBuf;

use probe_rs::flashing::FileDownloadError;
use probe_rs::probe::{DebugProbeError, DebugProbeSelector};

use crate::{DebugProbeSelectorExt, GPIO_COUNT, Kind};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no Riotee device is connected")]
    NoDevice,

    #[error("no connected Riotee device matches {selector}")]
    NoMatch { selector: DebugProbeSelector },

    #[error("{} Riotee devices match, must be unique:\n{}", .selectors.len(), describe_selectors(.selectors))]
    Ambiguous { selectors: Vec<DebugProbeSelector> },

    #[error("{selector} does not identify a Riotee probe or board")]
    NotRiotee { selector: DebugProbeSelector },

    #[error("expected a {}, found a {}", expected.name(), found.name())]
    WrongKind { expected: Kind, found: Kind },

    #[error("opening {} as a CMSIS-DAP device", kind.name())]
    OpenProbe {
        kind: Kind,
        #[source]
        source: DebugProbeError,
    },

    #[error("sending vendor command 0x{id:02X}")]
    SendCommand {
        id: u8,
        #[source]
        source: DebugProbeError,
    },

    #[error("the probe returned error code 0x{status:02X} for vendor command 0x{id:02X}")]
    CommandFailed { id: u8, status: u8 },

    #[error("parsing the response of vendor command 0x{id:02X}")]
    BadResponse {
        id: u8,
        #[source]
        source: ResponseError,
    },

    #[error("GPIO pin {pin} does not exist, the Riotee Probe has {GPIO_COUNT} GPIO pins (0 through {})", GPIO_COUNT - 1)]
    NoSuchGpioPin { pin: u8 },

    #[error("cannot {operation} {words} words at once, a single Spy-Bi-Wire packet holds {max}")]
    TooManyWords {
        operation: &'static str,
        words: usize,
        max: usize,
    },

    #[error("reading {}", path.display())]
    ReadFirmware {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("parsing {}", path.display())]
    ParseFirmware {
        path: PathBuf,
        #[source]
        source: HexError,
    },

    #[error("attaching to {chip}")]
    Attach {
        chip: &'static str,
        #[source]
        source: probe_rs::Error,
    },

    #[error("programming {}", path.display())]
    Download {
        path: PathBuf,
        // Boxed, it is by far the largest of these and would bloat every Result in the crate.
        #[source]
        source: Box<FileDownloadError>,
    },

    #[error("verification failed at 0x{address:08X}")]
    VerificationFailed { address: u32 },

    #[error(transparent)]
    ProbeRs(#[from] probe_rs::Error),
}

/// A vendor command response with unexpected data/payload.
#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("expected {expected} more bytes, but {remaining} remain")]
    Truncated { expected: usize, remaining: usize },

    #[error("not fully parsed, {} bytes remain: {trailing:02X?}", trailing.len())]
    Trailing { trailing: Vec<u8> },

    #[error("unknown GPIO level 0x{value:02X}")]
    UnknownGpioLevel { value: u8 },
}

/// A firmware image that is not usable Intel HEX.
#[derive(Debug, thiserror::Error)]
pub enum HexError {
    #[error("malformed Intel HEX record")]
    Record(#[from] ihex::ReaderError),

    #[error("segment at 0x{address:08X} does not start on a 16-bit boundary")]
    Unaligned { address: u32 },

    #[error("segment at 0x{address:08X} has an odd number of bytes ({len})")]
    OddLength { address: u32, len: usize },
}

fn describe_selectors(selectors: &[DebugProbeSelector]) -> String {
    selectors
        .iter()
        .map(|selector| {
            let kind = selector
                .riotee_kind()
                .expect("list() only returns Riotee selectors");
            format!("  {kind} as {selector}", kind = kind.name())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
