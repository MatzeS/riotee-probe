use std::path::Path;
use std::time::Duration;

use probe_rs::flashing::{self, DownloadOptions, ElfLoader, ElfOptions, FlashProgress};
use probe_rs::probe::Probe;
use probe_rs::{Core, Permissions, Session};

use crate::error::{Error, Result};

use super::{ProgramOptions, Target};

pub struct Nrf52Session {
    session: Session,
}

impl Nrf52Session {
    pub fn attach(probe: Probe) -> Result<Self> {
        const CHIP: &str = "nRF52833_xxAA";
        let session = probe
            .attach(CHIP, Permissions::new())
            .map_err(|source| Error::Attach { chip: CHIP, source })?;
        Ok(Self { session })
    }

    fn core(&mut self) -> Result<Core<'_>> {
        // The nRF52833 on the Riotee Module is single-core, so core 0 is the only one.
        Ok(self.session.core(0)?)
    }
}

impl Target for Nrf52Session {
    fn reset(&mut self) -> Result<()> {
        self.core()?.reset()?;
        Ok(())
    }

    fn halt(&mut self) -> Result<()> {
        const TIMEOUT: Duration = Duration::from_millis(500);
        self.core()?.halt(TIMEOUT)?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.core()?.run()?;
        Ok(())
    }

    /// Flashes `firmware`, which must be an ELF file.
    fn program(
        &mut self,
        firmware: &Path,
        options: ProgramOptions,
        progress: FlashProgress<'_>,
    ) -> Result<()> {
        let mut download = DownloadOptions::new();
        download.verify = options.verify;
        download.progress = progress;

        flashing::download_file_with_options(
            &mut self.session,
            firmware,
            ElfLoader(ElfOptions::default()),
            download,
        )
        .map_err(|source| Error::Download {
            path: firmware.to_path_buf(),
            source: Box::new(source),
        })
    }
}
