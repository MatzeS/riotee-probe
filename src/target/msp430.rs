use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use probe_rs::flashing::{FlashProgress, ProgressEvent, ProgressOperation};
use probe_rs::probe::cmsisdap::CmsisDap;

use crate::intelhex::{self, packetize};
use crate::vendor_commands::{self, CmsisDapExt};

use super::Target;

/// A connected Spy-Bi-Wire session. Disconnects automatically when dropped.
pub struct Msp430<'p> {
    probe: &'p mut CmsisDap,
}

impl<'p> Msp430<'p> {
    pub fn connect(probe: &'p mut CmsisDap) -> Result<Self> {
        probe.send_riotee_command(vendor_commands::SbwConnect)?;
        Ok(Self { probe })
    }

    pub fn halt(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwHalt)
    }

    pub fn resume(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwResume)
    }

    pub fn reset(&mut self) -> Result<()> {
        self.probe.send_riotee_command(vendor_commands::SbwReset)
    }

    fn write_words(&mut self, address: u32, words: &[u16]) -> Result<()> {
        // TODO check input: word length
        self.probe
            .send_riotee_command(vendor_commands::SbwWrite { address, words })
    }

    fn read_words(&mut self, address: u32, count: usize) -> Result<Vec<u16>> {
        self.probe.send_riotee_command(vendor_commands::SbwRead {
            address,
            count: count as u8,
        })
    }

    pub fn program(&mut self, firmware: &Path, mut progress: FlashProgress<'_>) -> Result<()> {
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

    // TODO inline this function, there is no reason to extract it
    fn program_inner(
        &mut self,
        segments: &[intelhex::Segment],
        progress: &mut FlashProgress<'_>,
    ) -> Result<()> {
        // The vendor command packets have a maximum size of 64 byte.
        // Hence, the program write must be chunked into small write batches.
        const MAX_PACKET_SIZE: usize = 64;
        const PACKET_HEADER_LENGTH: usize = 6;
        const MAX_WORDS_PER_PACKET: usize = (MAX_PACKET_SIZE - PACKET_HEADER_LENGTH) / 2;

        self.halt()?;
        for segment in segments {
            for (address, chunk) in packetize(segment, MAX_WORDS_PER_PACKET) {
                let write_start = Instant::now();
                self.write_words(address, chunk)?;
                progress.emit(ProgressEvent::Progress {
                    operation: ProgressOperation::Program,
                    size: chunk.len() as u64,
                    time: write_start.elapsed(),
                });

                let verify_start = Instant::now();
                let readback = self.read_words(address, chunk.len())?;
                if readback != chunk {
                    bail!("verification failed at 0x{address:08X}");
                }
                progress.emit(ProgressEvent::Progress {
                    operation: ProgressOperation::Verify,
                    size: chunk.len() as u64,
                    time: verify_start.elapsed(),
                });
                // TODO running this interleaved has no benefit for the progress display.
                // TODO add ability to skip verificiation
                // TODO run verification after
            }
        }
        self.resume()
    }
}

impl Target for Msp430<'_> {
    // TODO inline these functions here, there is no point to redirect them
    fn reset(&mut self) -> Result<()> {
        Msp430::reset(self)
    }

    fn halt(&mut self) -> Result<()> {
        Msp430::halt(self)
    }

    fn resume(&mut self) -> Result<()> {
        Msp430::resume(self)
    }

    fn program(&mut self, firmware: &Path, progress: FlashProgress<'_>) -> Result<()> {
        Msp430::program(self, firmware, progress)
    }
}

// TODO rename Msp430, the name should make it clear that this is a connected 'session'?
impl Drop for Msp430<'_> {
    fn drop(&mut self) {
        let result = self
            .probe
            .send_riotee_command(vendor_commands::SbwDisconnect);
        if let Err(err) = result {
            eprintln!("warning: failed to disconnect from MSP430 target: {err:#}");
        }
    }
}
