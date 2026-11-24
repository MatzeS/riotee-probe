use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use probe_rs::flashing::{FlashProgress, ProgressEvent, ProgressOperation};
use probe_rs::probe::cmsisdap::CmsisDap;

use crate::intelhex::{self, packetize};
use crate::vendor_commands::{self, CmsisDapExt};

use super::Target;

pub struct Msp430Session {
    probe: CmsisDap,
}

impl Msp430Session {
    pub fn connect(mut probe: CmsisDap) -> Result<Self> {
        probe.send_riotee_command(vendor_commands::SbwConnect)?;
        Ok(Self { probe })
    }

    fn write_words(&mut self, address: u32, words: &[u16]) -> Result<()> {
        self.probe
            .send_riotee_command(vendor_commands::SbwWrite::new(address, words)?)
    }

    fn read_words(&mut self, address: u32, count: u8) -> Result<Vec<u16>> {
        self.probe
            .send_riotee_command(vendor_commands::SbwRead::new(address, count)?)
    }

    // TODO inline this function, there is no reason to extract it
    fn program_inner(
        &mut self,
        segments: &[intelhex::Segment],
        progress: &mut FlashProgress<'_>,
    ) -> Result<()> {
        self.halt()?;
        self.write_segments(segments, progress)?;
        self.verify_segments(segments, progress)?;
        self.resume()
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
                    size: chunk.len() as u64,
                    time: start.elapsed(),
                });
            }
        }
        Ok(())
    }

    // TODO add ability to skip verificiation
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
                    bail!("verification failed at 0x{address:08X}");
                }
                progress.emit(ProgressEvent::Progress {
                    operation: ProgressOperation::Verify,
                    size: chunk.len() as u64,
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

    fn program(&mut self, firmware: &Path, mut progress: FlashProgress<'_>) -> Result<()> {
        let contents = std::fs::read_to_string(firmware)
            .with_context(|| format!("reading {}", firmware.display()))?;
        let segments = intelhex::parse_words(&contents)
            .with_context(|| format!("parsing {}", firmware.display()))?;

        let total_words = segments.iter().map(|s| s.words.len() as u64).sum::<u64>();
        progress.emit(ProgressEvent::AddProgressBar {
            operation: ProgressOperation::Program,
            total: Some(total_words),
        });
        progress.emit(ProgressEvent::AddProgressBar {
            operation: ProgressOperation::Verify,
            total: Some(total_words),
        });
        progress.emit(ProgressEvent::Started(ProgressOperation::Program));
        progress.emit(ProgressEvent::Started(ProgressOperation::Verify));

        if let Err(err) = self.program_inner(&segments, &mut progress) {
            progress.emit(ProgressEvent::Failed(ProgressOperation::Program));
            progress.emit(ProgressEvent::Failed(ProgressOperation::Verify));
            return Err(err);
        }

        progress.emit(ProgressEvent::Finished(ProgressOperation::Program));
        progress.emit(ProgressEvent::Finished(ProgressOperation::Verify));
        Ok(())
    }
}

impl Drop for Msp430Session {
    fn drop(&mut self) {
        let result = self
            .probe
            .send_riotee_command(vendor_commands::SbwDisconnect);
        if let Err(err) = result {
            eprintln!("warning: failed to disconnect from MSP430 target: {err:#}");
        }
    }
}
