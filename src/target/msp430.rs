use std::path::Path;
use std::time::Instant;

use probe_rs::flashing::{FlashProgress, ProgressEvent, ProgressOperation};
use probe_rs::probe::cmsisdap::CmsisDap;

use crate::error::{Error, Result};
use crate::intelhex::{self, packetize};
use crate::vendor_commands::{self, CmsisDapExt};

use super::{ProgramOptions, Target};

pub struct Msp430Session {
    probe: CmsisDap,
    disconnected: bool,
}

impl Msp430Session {
    pub fn connect(mut probe: CmsisDap) -> Result<Self> {
        probe.send_riotee_command(vendor_commands::SbwConnect)?;
        Ok(Self {
            probe,
            disconnected: false,
        })
    }

    /// Disconnect from the target.
    ///
    /// If not called manually, drop will call this for you, but will be unable to handle the error,
    /// if one occurs.
    pub fn disconnect(mut self) -> Result<()> {
        self.send_disconnect()
    }

    fn send_disconnect(&mut self) -> Result<()> {
        if self.disconnected {
            return Ok(());
        }
        self.disconnected = true;
        self.probe
            .send_riotee_command(vendor_commands::SbwDisconnect)
    }

    fn write_words(&mut self, address: u32, words: &[u16]) -> Result<()> {
        self.probe
            .send_riotee_command(vendor_commands::SbwWrite::new(address, words)?)
    }

    fn read_words(&mut self, address: u32, count: u8) -> Result<Vec<u16>> {
        self.probe
            .send_riotee_command(vendor_commands::SbwRead::new(address, count)?)
    }

    fn write_segments(
        &mut self,
        segments: &[intelhex::Segment],
        progress: &mut FlashProgress<'_>,
    ) -> Result<()> {
        for segment in segments {
            for (address, chunk) in packetize(segment, vendor_commands::SbwWrite::MAX_WORDS) {
                let start = Instant::now();
                self.write_words(address, chunk)?;
                progress.emit(ProgressEvent::Progress {
                    operation: ProgressOperation::Program,
                    size: size_of_val(chunk) as u64,
                    time: start.elapsed(),
                });
            }
        }
        Ok(())
    }

    fn verify_segments(
        &mut self,
        segments: &[intelhex::Segment],
        progress: &mut FlashProgress<'_>,
    ) -> Result<()> {
        for segment in segments {
            for (address, chunk) in packetize(segment, vendor_commands::SbwRead::MAX_WORDS) {
                let start = Instant::now();
                let count = u8::try_from(chunk.len())
                    .expect("packetize bounds chunks by SbwRead::MAX_WORDS");
                let readback = self.read_words(address, count)?;
                if readback != chunk {
                    return Err(Error::VerificationFailed { address });
                }
                progress.emit(ProgressEvent::Progress {
                    operation: ProgressOperation::Verify,
                    size: size_of_val(chunk) as u64,
                    time: start.elapsed(),
                });
            }
        }
        Ok(())
    }
}

impl Target for Msp430Session {
    fn reset(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwReset)
    }

    fn halt(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwHalt)
    }

    fn resume(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwResume)
    }

    /// Flashes `firmware`, which must be an Intel HEX file.
    fn program(
        &mut self,
        firmware: &Path,
        options: ProgramOptions,
        mut progress: FlashProgress<'_>,
    ) -> Result<()> {
        let contents = std::fs::read_to_string(firmware).map_err(|source| Error::ReadFirmware {
            path: firmware.to_path_buf(),
            source,
        })?;
        let segments = intelhex::parse_words(&contents).map_err(|source| Error::ParseFirmware {
            path: firmware.to_path_buf(),
            source,
        })?;

        let total_bytes = segments
            .iter()
            .map(|segment| size_of_val(segment.words.as_slice()) as u64)
            .sum();

        add_progress_bar(&mut progress, ProgressOperation::Program, total_bytes);
        if options.verify {
            add_progress_bar(&mut progress, ProgressOperation::Verify, total_bytes);
        }

        self.halt()?;

        start_operation(&mut progress, ProgressOperation::Program);
        let written = self.write_segments(&segments, &mut progress);
        finish_operation(&mut progress, ProgressOperation::Program, written)?;

        if options.verify {
            start_operation(&mut progress, ProgressOperation::Verify);
            let verified = self.verify_segments(&segments, &mut progress);
            finish_operation(&mut progress, ProgressOperation::Verify, verified)?;
        }

        self.resume()
    }
}

// Drop cannot return a Result, so this is the one place the library writes to stderr itself.
// Callers that want the error instead call `disconnect`.
impl Drop for Msp430Session {
    fn drop(&mut self) {
        if let Err(err) = self.send_disconnect() {
            eprintln!("warning: failed to disconnect from MSP430 target: {err:#}");
        }
    }
}

fn add_progress_bar(progress: &mut FlashProgress<'_>, operation: ProgressOperation, total: u64) {
    progress.emit(ProgressEvent::AddProgressBar {
        operation,
        total: Some(total),
    });
}

fn start_operation(progress: &mut FlashProgress<'_>, operation: ProgressOperation) {
    progress.emit(ProgressEvent::Started(operation));
}

fn finish_operation(
    progress: &mut FlashProgress<'_>,
    operation: ProgressOperation,
    result: Result<()>,
) -> Result<()> {
    progress.emit(if result.is_ok() {
        ProgressEvent::Finished(operation)
    } else {
        ProgressEvent::Failed(operation)
    });
    result
}
