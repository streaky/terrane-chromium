//! Native CDP adapters for the source-owned page lifecycle.
//!
//! The source language currently cannot project `Browser::new(LaunchOptions<'_>)`
//! because the launch options carry a lifetime, cannot operate on the opaque
//! `Arc<Tab>` returned by `Browser::get_tabs`, and cannot express generic CDP
//! methods with their associated response types or native event callbacks.
//! These adapters keep those concrete interop gaps local; validation, units,
//! defaults, persistence and policy remain in the package source.
//!
//! Remove this module when source can construct configured browsers, retain and
//! identify native tabs, send typed CDP requests/read responses, and register
//! bounded event callbacks with correct lifetimes. Convenience such as asset
//! readiness and disclosure expansion is package behavior, not itself a gap.

use crossbeam_channel::Receiver;
use headless_chrome::{
    Browser, LaunchOptions,
    protocol::cdp::{Emulation, Page, Runtime},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

// The source owns Browser. Borrow it and briefly clone the matching Arc so tab
// lifetime is stable across each CDP operation; this small clone is expected.
fn tab(browser: &Browser, target: &str) -> Result<Arc<headless_chrome::browser::tab::Tab>, String> {
    browser
        .get_tabs()
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|tab| tab.get_target_id().as_str() == target)
        .cloned()
        .ok_or_else(|| format!("page target {target} is no longer open"))
}

fn remaining_ms(deadline: Instant) -> Result<u64, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("timed out waiting for page readiness".into());
    }
    Ok(remaining.as_millis().max(1).min(u64::MAX as u128) as u64)
}

// LaunchOptions<'_> construction is declined by projection. Native RAII covers
// setup failure; successful ownership transfers into the source page state.
pub fn open_page(
    executable: Option<&str>,
    timeout_ms: u64,
    width: u32,
    height: u32,
    device_scale: f64,
    diagnostic_capacity: usize,
) -> Result<(Browser, Receiver<String>, String), String> {
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
    // Start on a fresh about:blank target, not Chrome's privileged new-tab page,
    // whose CSP would silently prevent scripts in subsequently installed HTML.
    let tab = browser.new_tab().map_err(|e| e.to_string())?;
    tab.set_default_timeout(Duration::from_millis(timeout_ms));
    set_viewport(&browser, tab.get_target_id(), width, height, device_scale)?;
    let receiver = crate::chromium_events::capture(
        &browser.get_ws_url(),
        tab.get_target_id(),
        diagnostic_capacity,
        timeout_ms,
    )?;
    Ok((browser, receiver, tab.get_target_id().to_string()))
}

// Navigation and DOM installation require the opaque tab's native CDP methods.
// The readiness stages share a deadline. Browser startup and CDP transport RPCs
// also have their own upstream limits; this is not a hard resource sandbox.
pub fn navigate(
    browser: &Browser,
    target: &str,
    url: &str,
    wait_assets: bool,
    selector: Option<&str>,
    predicate: Option<&str>,
    timeout_ms: u64,
) -> Result<(), String> {
    let t = tab(browser, target)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    // Observe loader identity and document readiness instead of network idle.
    // The upstream event parser is incompatible with Terrane's exact-number JSON
    // features (see events.rs), so navigation must not depend on its listeners.
    let navigation = t
        .call_method(Page::Navigate {
            url: url.to_owned(),
            referrer: None,
            transition_Type: None,
            frame_id: None,
            referrer_policy: None,
        })
        .map_err(|e| e.to_string())?;
    if let Some(error) = navigation.error_text {
        return Err(format!("navigation failed: {error}"));
    }
    if navigation.is_download == Some(true) {
        return Err("navigation started a download rather than a page".into());
    }
    if let Some(loader) = navigation.loader_id {
        loop {
            remaining_ms(deadline)?;
            let frame = t
                .call_method(Page::GetFrameTree(None))
                .map_err(|e| e.to_string())?;
            if frame.frame_tree.frame.loader_id == loader {
                break;
            }
            std::thread::sleep(
                Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
    wait_loop(
        &t,
        "document.readyState === 'complete'",
        remaining_ms(deadline)?,
    )?;
    readiness(&t, wait_assets, selector, predicate, deadline)
}

pub fn set_html(
    browser: &Browser,
    target: &str,
    html: &str,
    base_url: Option<&str>,
    wait_assets: bool,
    selector: Option<&str>,
    predicate: Option<&str>,
    timeout_ms: u64,
) -> Result<(), String> {
    let t = tab(browser, target)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    // A base URL establishes relative-asset resolution, not a preliminary URL
    // navigation. Use the browser DOM parser to prepend <base> before installing
    // active HTML, preserving its doctype and raw script/style text.
    let prepared;
    let content = if let Some(url) = base_url {
        let expression = format!(
            "(()=>{{const doc=new DOMParser().parseFromString({},'text/html');const base=doc.createElement('base');base.href={};doc.head.prepend(base);return (doc.doctype?new XMLSerializer().serializeToString(doc.doctype):'')+doc.documentElement.outerHTML;}})()",
            serde_json::to_string(html).map_err(|e| e.to_string())?,
            serde_json::to_string(url).map_err(|e| e.to_string())?
        );
        prepared = evaluate_raw(&t, &expression, false, remaining_ms(deadline)?)?
            .as_str()
            .ok_or("could not prepare HTML base URL")?
            .to_owned();
        prepared.as_str()
    } else {
        html
    };
    let frame = t
        .call_method(Page::GetFrameTree(None))
        .map_err(|e| e.to_string())?;
    // SetDocumentContent owns its HTML string; this required copy is not a
    // reason to retain Rust. The unavailable tab operation is the actual gap.
    t.call_method(Page::SetDocumentContent {
        frame_id: frame.frame_tree.frame.id,
        html: content.to_owned(),
    })
    .map_err(|e| e.to_string())?;
    readiness(&t, wait_assets, selector, predicate, deadline)
}

fn readiness(
    t: &headless_chrome::browser::tab::Tab,
    assets: bool,
    selector: Option<&str>,
    predicate: Option<&str>,
    deadline: Instant,
) -> Result<(), String> {
    if assets {
        evaluate_raw(
            t,
            "Promise.all(Array.from(document.images, i => i.decode())).then(() => document.fonts.ready).then(() => true)",
            true,
            remaining_ms(deadline)?,
        )?;
    }
    if let Some(selector) = selector {
        wait_loop(
            t,
            &format!(
                "!!document.querySelector({})",
                serde_json::to_string(selector).map_err(|e| e.to_string())?
            ),
            remaining_ms(deadline)?,
        )?;
    }
    if let Some(expression) = predicate {
        wait_loop(t, &format!("!!({expression})"), remaining_ms(deadline)?)?;
    }
    Ok(())
}

fn wait_loop(
    t: &headless_chrome::browser::tab::Tab,
    expression: &str,
    timeout_ms: u64,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remaining = remaining_ms(deadline)?;
        let value = evaluate_raw(t, expression, false, remaining)?;
        if value.as_bool() == Some(true) {
            return Ok(());
        }
        std::thread::sleep(
            Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

// Runtime::Evaluate's exception-details channel must be checked; the high-level
// helper discards it. Awaited expressions race a timer, and CDP's execution
// timeout also bounds synchronous JavaScript. Only by-value JSON escapes.
fn evaluate_raw(
    t: &headless_chrome::browser::tab::Tab,
    expression: &str,
    await_promise: bool,
    timeout_ms: u64,
) -> Result<Value, String> {
    let expression = if await_promise {
        format!(
            "(()=>{{const start=Date.now();const value=({expression});let timer;return Promise.race([Promise.resolve(value),new Promise((_,reject)=>{{timer=setTimeout(()=>reject(new Error('JavaScript promise timed out')),Math.max(0,{timeout_ms}-(Date.now()-start)));}})]).finally(()=>clearTimeout(timer));}})()"
        )
    } else {
        expression.to_owned()
    };
    let response = t
        .call_method(Runtime::Evaluate {
            expression,
            return_by_value: Some(true),
            generate_preview: Some(false),
            silent: Some(false),
            await_promise: Some(await_promise),
            include_command_line_api: Some(false),
            user_gesture: Some(false),
            object_group: None,
            context_id: None,
            throw_on_side_effect: None,
            timeout: Some(timeout_ms as f64),
            disable_breaks: None,
            repl_mode: None,
            allow_unsafe_eval_blocked_by_csp: None,
            unique_context_id: None,
            serialization_options: None,
        })
        .map_err(|e| e.to_string())?;
    if let Some(exception) = response.exception_details {
        let detail = exception
            .exception
            .and_then(|value| value.description)
            .unwrap_or(exception.text);
        return Err(format!("JavaScript evaluation failed: {detail}"));
    }
    let result = response.result;
    match result.subtype {
        Some(Runtime::RemoteObjectSubtype::Null) => return Ok(Value::Null),
        None | Some(Runtime::RemoteObjectSubtype::Array) => {}
        Some(_) => {
            return Err("JavaScript returned a non-JSON object or an unawaited promise".into());
        }
    }
    result
        .value
        .ok_or_else(|| "JavaScript returned a non-JSON value".into())
}

// Screen emulation is a native CDP operation on the opaque Arc<Tab>. Source
// validates dimensions and scale; this adapter only maps that public contract.
pub fn set_viewport(
    browser: &Browser,
    target: &str,
    width: u32,
    height: u32,
    device_scale: f64,
) -> Result<(), String> {
    tab(browser, target)?
        .call_method(Emulation::SetDeviceMetricsOverride {
            width,
            height,
            device_scale_factor: device_scale,
            mobile: false,
            scale: None,
            screen_width: None,
            screen_height: None,
            position_x: None,
            position_y: None,
            dont_set_visible_size: None,
            screen_orientation: None,
            viewport: None,
            display_feature: None,
            device_posture: None,
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn wait_for_selector(
    browser: &Browser,
    target: &str,
    selector: &str,
    timeout_ms: u64,
) -> Result<(), String> {
    let expression = format!(
        "!!document.querySelector({})",
        serde_json::to_string(selector).map_err(|e| e.to_string())?
    );
    wait_loop(tab(browser, target)?.as_ref(), &expression, timeout_ms)
}

pub fn wait_for(
    browser: &Browser,
    target: &str,
    expression: &str,
    timeout_ms: u64,
) -> Result<(), String> {
    wait_loop(
        tab(browser, target)?.as_ref(),
        &format!("!!({expression})"),
        timeout_ms,
    )
}

pub fn evaluate(
    browser: &Browser,
    target: &str,
    expression: &str,
    await_promise: bool,
    timeout_ms: u64,
) -> Result<String, String> {
    serde_json::to_string(&evaluate_raw(
        tab(browser, target)?.as_ref(),
        expression,
        await_promise,
        timeout_ms,
    )?)
    .map_err(|e| e.to_string())
}

// Capture an explicit viewport or the current CSS content extent. CDP's
// captureBeyondViewport is not exposed by the high-level screenshot helper.
// Return its base64 payload for the ordinary Terrane codec to decode.
pub fn screenshot(
    browser: &Browser,
    target: &str,
    format: &str,
    quality: Option<u32>,
    full_page: bool,
) -> Result<String, String> {
    let t = tab(browser, target)?;
    let format = match format {
        "png" => Page::CaptureScreenshotFormatOption::Png,
        "jpeg" => Page::CaptureScreenshotFormatOption::Jpeg,
        _ => return Err("unsupported screenshot format".into()),
    };
    let clip = if full_page {
        let dimensions = t
            .call_method(Page::GetLayoutMetrics(None))
            .map_err(|e| e.to_string())?
            .css_content_size;
        Some(Page::Viewport {
            x: 0.0,
            y: 0.0,
            width: dimensions.width,
            height: dimensions.height,
            scale: 1.0,
        })
    } else {
        None
    };
    let result = t
        .call_method(Page::CaptureScreenshot {
            format: Some(format),
            quality,
            clip,
            from_surface: Some(true),
            capture_beyond_viewport: Some(full_page),
            optimize_for_speed: None,
        })
        .map_err(|e| e.to_string())?;
    Ok(result.data)
}

// Native print options are needed only because print_to_pdf is behind the
// opaque tab. Named sizes, millimetre conversion and validation remain source.
pub fn print_pdf(
    browser: &Browser,
    target: &str,
    display_header_footer: bool,
    print_background: bool,
    prefer_css_page_size: bool,
    expand_disclosures: bool,
    paper_width_inches: Option<f64>,
    paper_height_inches: Option<f64>,
    landscape: bool,
    margin_top_inches: Option<f64>,
    margin_bottom_inches: Option<f64>,
    margin_left_inches: Option<f64>,
    margin_right_inches: Option<f64>,
    scale: Option<f64>,
    page_ranges: Option<&str>,
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    let t = tab(browser, target)?;
    if expand_disclosures {
        evaluate_raw(
            &t,
            "document.querySelectorAll('details').forEach(e => e.open = true); true",
            false,
            timeout_ms,
        )?;
    }
    evaluate_raw(
        &t,
        "Promise.all(Array.from(document.images, i => i.decode())).then(() => document.fonts.ready).then(() => true)",
        true,
        timeout_ms,
    )?;
    let options = headless_chrome::types::PrintToPdfOptions {
        landscape: Some(landscape),
        display_header_footer: Some(display_header_footer),
        print_background: Some(print_background),
        paper_width: paper_width_inches,
        paper_height: paper_height_inches,
        margin_top: margin_top_inches,
        margin_bottom: margin_bottom_inches,
        margin_left: margin_left_inches,
        margin_right: margin_right_inches,
        scale,
        page_ranges: page_ranges.map(str::to_owned),
        prefer_css_page_size: Some(prefer_css_page_size),
        ..Default::default()
    };
    t.print_to_pdf(Some(options)).map_err(|e| e.to_string())
}

// Capture metadata through the same native evaluator and drain only normalized
// events. This is bounded diagnostic evidence, not an audit or full waterfall.
pub fn diagnostics(
    browser: &Browser,
    target: &str,
    events: &Receiver<String>,
    injected_html: bool,
    timeout_ms: u64,
) -> Result<String, String> {
    let t = tab(browser, target)?;
    let document = evaluate_raw(
        &t,
        "({url:location.href,title:document.title,timing:performance.getEntriesByType('navigation')[0]?.toJSON() ?? null,resources:performance.getEntriesByType('resource').map(e=>e.toJSON())})",
        false,
        timeout_ms,
    )?;
    // Snapshot the batch size: a page logging continuously must not turn this
    // drain into an unbounded loop or grow a report beyond the capture capacity.
    let available = events.len();
    let capacity_reached = events.capacity() == Some(available);
    let mut records = Vec::with_capacity(available);
    let first = if available == 0 {
        match events.try_recv() {
            Ok(record) => Some(record),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                return Err("browser diagnostic capture stopped".into());
            }
        }
    } else {
        None
    };
    for record in first.into_iter().chain(events.try_iter().take(available)) {
        let record: Value =
            serde_json::from_str(&record).map_err(|e| format!("invalid diagnostic event: {e}"))?;
        if record["kind"] == "capture-error" {
            return Err(format!(
                "browser diagnostic capture failed: {}",
                record["message"]
            ));
        }
        records.push(record);
    }
    let status = if injected_html {
        None
    } else {
        document["timing"]["responseStatus"]
            .as_u64()
            .filter(|status| *status > 0)
            .or_else(|| {
                records
                    .iter()
                    .rev()
                    .find(|event| {
                        event.get("kind").and_then(Value::as_str) == Some("response")
                            && event.get("resource-type").and_then(Value::as_str)
                                == Some("Document")
                            && event.get("url") == document["timing"].get("name")
                    })
                    .and_then(|event| event.get("status").and_then(Value::as_u64))
            })
    };
    let output = json!({"url":document["url"],"title":document["title"],"status":status,"navigation-timing":document["timing"],"resource-timings":document["resources"],"events":records,"capacity-reached":capacity_reached});
    serde_json::to_string(&output).map_err(|e| e.to_string())
}

// Explicit close can report CDP failure. Source always releases Browser after
// this operation, including failure; its native Drop is the cleanup backstop.
pub fn close(browser: &Browser, target: &str) -> Result<(), String> {
    tab(browser, target)?
        .close(false)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
