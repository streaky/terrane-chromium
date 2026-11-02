//! Native compatibility seams for the source-owned Chromium page.
//!
//! Browser construction still needs `LaunchOptions<'_>` and transfers native
//! RAII owners into page state. Event polling remains until crossbeam exposes a
//! Terrane error distinction for Empty versus Disconnected. No page policy lives
//! here.

use crossbeam_channel::Receiver;
use headless_chrome::{Browser, LaunchOptions};
use std::{path::PathBuf, sync::Arc, time::Duration};

// LaunchOptions<'_> construction is declined by projection. Native RAII covers
// setup failure; successful ownership transfers into the source page state.
pub fn open_page(
    executable: Option<&str>,
    timeout_ms: u64,
    diagnostic_capacity: usize,
) -> Result<
    (
        Browser,
        Arc<headless_chrome::browser::tab::Tab>,
        Receiver<String>,
    ),
    String,
> {
    let path = match executable {
        Some(path) => PathBuf::from(path),
        None => headless_chrome::browser::default_executable()
            .map_err(|e| format!("could not find Chromium: {e}"))?,
    };
    let options = LaunchOptions::default_builder()
        .path(Some(path))
        .build()
        .map_err(|e| e.to_string())?;
    let browser = Browser::new(options).map_err(|e| format!("could not start Chromium: {e}"))?;
    // Start on a fresh tab instead of Chrome's privileged new-tab page: its
    // content security policy would silently block scripts in installed HTML.
    let tab = browser.new_tab().map_err(|e| e.to_string())?;
    tab.set_default_timeout(Duration::from_millis(timeout_ms));
    let receiver = crate::chromium_events::capture(
        &browser.get_ws_url(),
        tab.get_target_id(),
        diagnostic_capacity,
        timeout_ms,
    )?;
    Ok((browser, tab, receiver))
}

// Bundle 2 leaves only crossbeam's Empty/Disconnected enum adaptation here:
// dependency-error cannot distinguish those variants in Terrane. Remove this
// primitive when try_recv exposes typed empty/disconnected results (bundle 5).
pub fn poll_event(receiver: &Receiver<String>) -> (Option<String>, bool) {
    match receiver.try_recv() {
        Ok(record) => (Some(record), false),
        Err(crossbeam_channel::TryRecvError::Empty) => (None, false),
        Err(crossbeam_channel::TryRecvError::Disconnected) => (None, true),
    }
}
