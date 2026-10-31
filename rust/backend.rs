use headless_chrome::{Browser, LaunchOptions, protocol::cdp::Page, types::PrintToPdfOptions};
use std::error::Error;
use std::path::PathBuf;

pub fn find_executable() -> Option<String> {
    headless_chrome::browser::default_executable()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

// Browser::new_tab returns Arc<Tab>, whose operations are not currently exposed
// by Terrane's projection. Keep that native lifetime here; the public package
// API contains only Terrane options, bytes, and a typed error.
fn print_html(
    html: &str,
    executable: Option<&str>,
    display_header_footer: bool,
    print_background: bool,
    prefer_css_page_size: bool,
    expand_disclosures: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
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
    let frame = tab.call_method(Page::GetFrameTree(None))?;
    tab.call_method(Page::SetDocumentContent {
        frame_id: frame.frame_tree.frame.id,
        // The CDP request owns its HTML. This is the sole input copy, made at
        // that ownership boundary rather than by the source-call wrapper.
        html: html.to_owned(),
    })?;
    if expand_disclosures {
        tab.evaluate(
            "document.querySelectorAll('details').forEach(node => node.open = true)",
            false,
        )?;
    }
    tab.evaluate(
        "Promise.all(Array.from(document.images, image => image.decode())).then(() => document.fonts.ready)",
        true,
    )?;
    Ok(tab.print_to_pdf(Some(PrintToPdfOptions {
        display_header_footer: Some(display_header_footer),
        print_background: Some(print_background),
        prefer_css_page_size: Some(prefer_css_page_size),
        ..PrintToPdfOptions::default()
    }))?)
}

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
