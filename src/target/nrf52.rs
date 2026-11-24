use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use probe_rs::config::TargetSelector;
use probe_rs::flashing::{self, DownloadOptions, FlashProgress, HexLoader};
use probe_rs::probe::Probe;
use probe_rs::{Permissions, Session};

use super::Target;

pub struct Nrf52 {
    session: Session,
}

impl Nrf52 {
    pub fn attach(probe: Probe) -> Result<Self> {
        let session = probe
            .attach(TargetSelector::Auto, Permissions::new())
            .context("attaching to nRF52 target")?;
        Ok(Self { session })
    }
}

impl Target for Nrf52 {
    fn reset(&mut self) -> Result<()> {
        self.session.core(0)?.reset()?;
        Ok(())
    }

    fn halt(&mut self) -> Result<()> {
        // TODO is core(0) a magic number, why 0? what if there are multiple cores?
        self.session.core(0)?.halt(Duration::from_millis(500))?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.session.core(0)?.run()?;
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
