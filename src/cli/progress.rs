//! Progress bar implementation based on the indicatif crate.
//!
//! This just binds the probe-rs flash progress to a terminal progress bar.

use std::cell::RefCell;
use std::rc::Rc;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use probe_rs::flashing::{FlashProgress, ProgressEvent, ProgressOperation};

fn operation_label(operation: ProgressOperation) -> &'static str {
    match operation {
        ProgressOperation::Compare => "Compare",
        ProgressOperation::Fill => "Fill",
        ProgressOperation::Erase => "Erase",
        ProgressOperation::Program => "Program",
        ProgressOperation::Verify => "Verify",
        ProgressOperation::Ram => "RAM",
    }
}

pub fn indicatif_progress<'a>() -> FlashProgress<'a> {
    // Claudes implementation, not reviewed, seems to work.
    let multi = MultiProgress::new();
    let bars: Rc<RefCell<Vec<(ProgressOperation, ProgressBar)>>> =
        Rc::new(RefCell::new(Vec::new()));

    FlashProgress::new(move |event| match event {
        ProgressEvent::AddProgressBar { operation, total } => {
            let bar = multi.add(ProgressBar::new(total.unwrap_or_default()));
            if let Ok(style) =
                ProgressStyle::with_template("{msg:>8} [{bar:40.cyan/blue}] {bytes}/{total_bytes}")
            {
                bar.set_style(style.progress_chars("=> "));
            }
            bar.set_message(operation_label(operation));
            bars.borrow_mut().push((operation, bar));
        }
        ProgressEvent::Progress {
            operation, size, ..
        } => {
            if let Some((_, bar)) = bars.borrow().iter().find(|(op, _)| *op == operation) {
                bar.inc(size);
            }
        }
        ProgressEvent::Finished(operation) => {
            if let Some((_, bar)) = bars.borrow().iter().find(|(op, _)| *op == operation) {
                bar.finish();
            }
        }
        ProgressEvent::Failed(operation) => {
            if let Some((_, bar)) = bars.borrow().iter().find(|(op, _)| *op == operation) {
                bar.abandon();
            }
        }
        _ => {}
    })
}
