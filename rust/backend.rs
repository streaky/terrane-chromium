//! Native compatibility boundary for Chromium event polling.
//!
//! Browser startup and ownership transfer are implemented in package source.
//! The event bridge maps crossbeam's typed Empty versus Disconnected result.

use crossbeam_channel::Receiver;

// Preserve Empty versus Disconnected for the source diagnostics API.
pub fn poll_event(receiver: &Receiver<String>) -> (Option<String>, bool) {
    match receiver.try_recv() {
        Ok(record) => (Some(record), false),
        Err(crossbeam_channel::TryRecvError::Empty) => (None, false),
        Err(crossbeam_channel::TryRecvError::Disconnected) => (None, true),
    }
}
