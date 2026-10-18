//! Progress indicators for long pack phases. Everything draws to
//! stderr and stays hidden when piped, so scripts keep clean stdout.
//! Bar style matches `fetch-data`.

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

const COUNT_TEMPLATE: &str = "{msg:12} {bar:40.cyan/blue} {pos}/{len} {per_sec} eta {eta}";
const BYTES_TEMPLATE: &str =
    "{msg:12} {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} eta {eta}";
const SPINNER_TEMPLATE: &str = "{msg} {spinner}";

/// Hide the bar when stderr is not a terminal (piped output, tests).
fn maybe_hide(bar: ProgressBar) -> ProgressBar {
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        bar.set_draw_target(ProgressDrawTarget::hidden());
    }
    bar
}

fn style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .unwrap()
        .progress_chars("=>-")
        .tick_chars(r"/|\- ")
}

/// Entry-count bar for loops with a known total.
pub fn count_bar(label: &str, total: u64) -> ProgressBar {
    let bar = maybe_hide(ProgressBar::new(total));
    bar.set_style(style(COUNT_TEMPLATE));
    bar.set_message(label.to_owned());
    bar
}

/// Byte bar for streaming file reads (advance via `wrap_read`).
pub fn bytes_bar(label: &str, total: u64) -> ProgressBar {
    let bar = maybe_hide(ProgressBar::new(total));
    bar.set_style(style(BYTES_TEMPLATE));
    bar.set_message(label.to_owned());
    bar
}

/// Indeterminate spinner for phases without a total (sort, encode).
pub fn spinner(label: &str) -> ProgressBar {
    let bar = maybe_hide(ProgressBar::new_spinner());
    bar.set_style(style(SPINNER_TEMPLATE));
    bar.set_message(label.to_owned());
    bar.enable_steady_tick(Duration::from_millis(120));
    bar
}
