//! Progress bar implementation based on the indicatif crate.
//!
//! This just binds the probe-rs flash progress to a terminal progress bar.
//!
//! Claudes implementation, not reviewed, seems to work.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use probe_rs::flashing::{FlashProgress, ProgressEvent, ProgressOperation};

const TEMPLATE: &str = "{msg:>8} [{bar:40.cyan/blue}] {bytes}/{total_bytes}";

struct Bar {
    operation: ProgressOperation,
    bar: ProgressBar,
    started: bool,
    finished: bool,
}

fn select(bars: &mut [Bar], operation: ProgressOperation) -> Option<&mut Bar> {
    bars.iter_mut()
        .find(|bar| bar.operation == operation && !bar.finished)
}

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
    let multi = MultiProgress::new();
    let mut bars: Vec<Bar> = Vec::new();

    FlashProgress::new(move |event| match event {
        ProgressEvent::AddProgressBar { operation, total } => {
            let style = ProgressStyle::with_template(TEMPLATE)
                .expect("the progress bar template is valid")
                .progress_chars("=> ");
            let bar = multi.add(ProgressBar::new(total.unwrap_or_default()));
            bar.set_style(style);
            bar.set_message(operation_label(operation));
            bars.push(Bar {
                operation,
                bar,
                started: false,
                finished: false,
            });
        }
        ProgressEvent::Started(operation) => {
            if let Some(entry) = select(&mut bars, operation) {
                entry.started = true;
                entry.bar.reset_elapsed();
                entry.bar.reset_eta();
            }
        }
        ProgressEvent::Progress {
            operation, size, ..
        } => {
            if let Some(entry) = select(&mut bars, operation) {
                if entry.started {
                    entry.bar.inc(size);
                } else {
                    // Progress before `Started` is work the operation skips, so take it off
                    // the total instead of counting it as done.
                    entry.bar.dec_length(size);
                }
            }
        }
        ProgressEvent::Finished(operation) => {
            if let Some(entry) = select(&mut bars, operation) {
                entry.finished = true;
                entry.bar.finish();
            }
        }
        ProgressEvent::Failed(operation) => {
            if let Some(entry) = select(&mut bars, operation) {
                entry.finished = true;
                entry.bar.abandon();
            }
        }
        _ => {}
    })
}
