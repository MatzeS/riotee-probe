use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use probe_rs::config::TargetSelector;
use probe_rs::flashing::{self, DownloadOptions, FlashProgress, HexLoader};
use probe_rs::probe::Probe;
use probe_rs::{Core, Permissions, Session};

use super::Target;

pub struct Nrf52 {
    session: Session,
}

impl Nrf52 {
    pub fn attach(probe: Probe) -> Result<Self> {
        let session = probe
            // TODO attaching with TargetSelector::Auto detects the chip; the Python tool pins
            // `target_override="nrf52"` instead. Consider pinning the target.
            .attach(TargetSelector::Auto, Permissions::new())
            .context("attaching to nRF52 target")?;
        Ok(Self { session })
    }

    fn core(&mut self) -> Result<Core<'_>> {
        // The nRF52833 on the Riotee Module is single-core, so core 0 is the only one.
        Ok(self.session.core(0)?)
    }
}

impl Target for Nrf52 {
    fn reset(&mut self) -> Result<()> {
        self.core()?.reset()?;
        Ok(())
    }

    fn halt(&mut self) -> Result<()> {
        self.core()?.halt(Duration::from_millis(500))?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.core()?.run()?;
        Ok(())
    }

    fn program(&mut self, firmware: &Path, progress: FlashProgress<'_>) -> Result<()> {
        let mut options = DownloadOptions::new();
        options.verify = true;
        options.progress = progress;

        flashing::download_file_with_options(&mut self.session, firmware, HexLoader, options)
            .with_context(|| format!("programming {}", firmware.display()))
    }
}
