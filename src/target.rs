use std::path::Path;

use probe_rs::flashing::FlashProgress;

use crate::error::Result;

pub mod msp430;
pub mod nrf52;

#[derive(Clone, Copy, Debug)]
pub struct ProgramOptions {
    pub verify: bool,
}

impl Default for ProgramOptions {
    fn default() -> Self {
        Self { verify: true }
    }
}

/// Operations common to both targets on the Riotee module.
pub trait Target {
    fn reset(&mut self) -> Result<()>;
    fn halt(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn program(
        &mut self,
        firmware: &Path,
        options: ProgramOptions,
        progress: FlashProgress<'_>,
    ) -> Result<()>;
}
