use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use probe_rs::config::TargetSelector;
use probe_rs::flashing::{self, DownloadOptions, FlashProgress, HexLoader};
use probe_rs::probe::Probe;
use probe_rs::{Permissions, Session};

use super::Target;

// TODO it is strange that a MSP type exists but no nrf type, instead this one just uses the session
// as is, but maybe it makes sense to wrap it for consistentcy.

pub fn attach(probe: Probe) -> Result<Session> {
    probe
        .attach(TargetSelector::Auto, Permissions::new())
        .context("attaching to nRF52 target")
}

pub fn halt(session: &mut Session) -> Result<()> {
    // TODO is core(0) a magic number, why 0? what if there are multiple cores?
    session.core(0)?.halt(Duration::from_millis(500))?;
    Ok(())
}

pub fn resume(session: &mut Session) -> Result<()> {
    session.core(0)?.run()?;
    Ok(())
}

pub fn reset(session: &mut Session) -> Result<()> {
    session.core(0)?.reset()?;
    Ok(())
}

pub fn program(session: &mut Session, firmware: &Path, progress: FlashProgress<'_>) -> Result<()> {
    let mut options = DownloadOptions::new();
    options.verify = true;
    options.progress = progress;

    flashing::download_file_with_options(session, firmware, HexLoader, options)
        .with_context(|| format!("programming {}", firmware.display()))
}

impl Target for Session {
    // TODO inline these functions

    fn reset(&mut self) -> Result<()> {
        reset(self)
    }

    fn halt(&mut self) -> Result<()> {
        halt(self)
    }

    fn resume(&mut self) -> Result<()> {
        resume(self)
    }

    fn program(&mut self, firmware: &Path, progress: FlashProgress<'_>) -> Result<()> {
        program(self, firmware, progress)
    }
}
