//! Native browser operation, retained for projection gaps, not clone avoidance.
//!
//! `chromium/projection-launch-options`: `Browser::new` takes `LaunchOptions<'_>`;
//! projection currently declines this lifetime-bearing input, so Terrane cannot
//! construct a browser with the selected executable through a projected call.
//! `chromium/projection-arc-tab`: `Browser::new_tab` returns an opaque `Arc<Tab>`;
//! the projected handle has no forwarded `Tab` operations (a source call to
//! `print_to_pdf` fails with T0055). Native construction fails with T0117.
//!
//! Keep creation, CDP requests and printing in one scope so the browser outlives
//! its tab and Rust drops both on every success/error path. Separate inline
//! `rust` blocks would still need native handle plumbing for these same gaps;
//! this module keeps that cohesive operation readable. Public options, typed
//! errors and output persistence do not belong here.
//!
//! Remove this boundary when source can construct the configured browser and
//! invoke CDP/evaluate/print through its returned handle, including the generic
//! CDP request/associated-response contract, with correct native lifetimes.
//! The small PathBuf-to-text discovery conversion is inline in src/pdf.trn.

use headless_chrome::{Browser, LaunchOptions, protocol::cdp::Page, types::PrintToPdfOptions};
use std::error::Error;
use std::path::PathBuf;

fn print_html(
    html: &str,
    executable: Option<&str>,
    display_header_footer: bool,
    print_background: bool,
    prefer_css_page_size: bool,
    expand_disclosures: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    // Launch configuration is native because of projection-launch-options.
    // Executable discovery also returns a native PathBuf. Preserve the upstream
    // search and sandbox defaults; the message adds installation context only.
    let executable = match executable {
        Some(path) => PathBuf::from(path),
        None => headless_chrome::browser::default_executable().map_err(|error| {
            format!("could not find Chromium: {error}; install Chromium/Chrome or set CHROME to its executable path")
        })?,
    };
    let options = LaunchOptions::default_builder()
        .path(Some(executable))
        .build()?;
    let browser = Browser::new(options).map_err(|error| {
        format!("could not start Chromium: {error}; install Chromium/Chrome or set CHROME to its executable path")
    })?;
    let tab = browser.new_tab()?;
    // projection-arc-tab prevents these calls through the Terrane Arc handle.
    // Ask CDP for the current main frame, then install HTML directly: this is
    // deliberately not a Markdown conversion or a navigation to a temporary file.
    let frame = tab.call_method(Page::GetFrameTree(None))?;
    tab.call_method(Page::SetDocumentContent {
        frame_id: frame.frame_tree.frame.id,
        // SetDocumentContent requires an owned String. This copy is the request's
        // ownership requirement, NOT a justification for keeping code in Rust.
        html: html.to_owned(),
    })?;
    // Expanding disclosures is package behavior selected by a Terrane option,
    // not a separate language gap. Only the Tab::evaluate call requires this
    // native scope; keep it beside DOM installation rather than expose handles.
    if expand_disclosures {
        tab.evaluate(
            "document.querySelectorAll('details').forEach(node => node.open = true)",
            false,
        )?;
    }
    // Wait for image decoding and font readiness before taking the print snapshot.
    // This is another evaluate on the same unprojectable handle, not a general
    // JavaScript application-readiness policy.
    tab.evaluate(
        "Promise.all(Array.from(document.images, image => image.decode())).then(() => document.fonts.ready)",
        true,
    )?;
    // Map the package's booleans onto native print options and return PDF bytes.
    // The need for Rust is the Arc<Tab> receiver, not this options construction.
    Ok(tab.print_to_pdf(Some(PrintToPdfOptions {
        display_header_footer: Some(display_header_footer),
        print_background: Some(print_background),
        prefer_css_page_size: Some(prefer_css_page_size),
        ..PrintToPdfOptions::default()
    }))?)
}

// Normalize heterogeneous native errors once for the inline Terrane boundary,
// which constructs chromium-error. Box<dyn Error> above is just local Rust error
// propagation convenience, not an additional claimed projection limitation.
// No file writes or document-specific policy are performed here.
pub fn render_pdf(
    html: &str,
    executable: Option<&str>,
    display_header_footer: bool,
    print_background: bool,
    prefer_css_page_size: bool,
    expand_disclosures: bool,
) -> Result<Vec<u8>, String> {
    print_html(
        html,
        executable,
        display_header_footer,
        print_background,
        prefer_css_page_size,
        expand_disclosures,
    )
    .map_err(|error| error.to_string())
}
