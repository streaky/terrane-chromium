# streaky/chromium

A Terrane library for Chromium-backed page operations and HTML-to-PDF rendering. The managed page API supports navigation, HTML installation, readiness waits, evaluation, screenshots, PDF printing, and bounded diagnostics; `render-pdf` remains a one-shot convenience function. Callers own content policy, storage, and output delivery.

## Manual

The [Terrane manual IR publication](docs/src/manual.yaml) follows the `docs/src/` layout. It includes an [introduction and runtime setup](docs/src/records/guides/introduction.yaml), a [complete render-and-save tutorial](docs/src/records/tutorials/render-pdf.yaml), and references for [PDF rendering](docs/src/records/api/pdf.yaml) and the [managed page API](docs/src/records/api/page.yaml), including security, diagnostics, and lazy/infinite content boundaries.

From this package directory in a Terrane checkout, render it with docs-converter:

```text
terrane run ../../projects/docs-converter -- html docs/src /tmp/chromium-manual.html
```

Use `markdown` or `json` instead of `html` for the other reference-publication outputs. The YAML records are the authored documentation sources.

## Dependency

```toml
[terrane-dependencies.streaky.chromium]
path = "../chromium"
```

Choose the path relative to the consuming package manifest. This is an ordinary source-composed Terrane library; it has no helper executable or manual bootstrap step.

## Existing PDF convenience API

`render-pdf` continues to render supplied HTML and return PDF bytes. Its existing contract is independent of the managed page API; docs-converter retains its `render-pdf(html,browser,printing)` call shape.

## Managed page API

The page API is documented in the [manual reference](docs/src/records/api/page.yaml). `open-page` accepts `browser-options` and `viewport-options`, returning a managed page. Configure browser `executable` (default `none` for system discovery), `timeout-ms` (30000), and bounded `diagnostic-capacity` (1000). Viewport defaults are 1280×720 at device scale factor 1.0. Launch configuration is built in source using the upstream `LaunchOptions` defaults; startup errors become `chromium-error`, and partial startup is cleaned up by ordinary ownership destruction.

The page supports URL navigation and HTML installation with load readiness options, viewport changes, selector/expression waits, JSON-compatible evaluation, PNG/JPEG screenshots, PDF printing, diagnostics, and idempotent close. `is-open` is the only operation that does not throw `chromium-error`; close is safe to repeat. Page cleanup also follows final shared ownership destruction. Evaluation exceptions, rejected promises, undefined and other non-JSON results are errors. PDF paper sizes, custom paired dimensions, margins, scale, orientation, page ranges, CSS precedence, and print-media behavior are detailed in the manual reference.

Full-page screenshot captures the current document extent, not all lazy or infinite-scroll content. Use page evaluation and application-specific waits to prepare content; no universal exhaustion guarantee exists. Diagnostics drain bounded best-effort events and are not a complete waterfall, HAR, or audit log.

## Security

The library does not sanitize HTML or JavaScript, restrict URL schemes or network destinations, or promise SSRF, filesystem, or network isolation. Active content, redirects, and browser vulnerabilities can access resources available under the host's privileges. Browser sandboxing remains enabled but is not network policy. Constrain untrusted content using caller-managed OS/container and network controls. Captures and diagnostics may expose private page data and secret-bearing URLs; redact and access-control them. Cookies are not returned deliberately. Operation timeouts are not overall CPU, memory, or network budgets.

No page clicking/typing, multi-page automation, downloads, raw CDP, or complete waterfall/HAR API is provided.

## Native boundary

Public types belong to `streaky/chromium`; native browser and tab handles are private. Terrane retains the `Arc<Tab>` returned at page creation. Navigation, typed CDP requests/responses, readiness, evaluation, viewport/screenshot capture, printing, and diagnostic aggregation are package source policy, not Rust adapters.

The private associated-type contract binds each operation to its projected response type. Source methods call the retained tab's generic CDP operation directly; no request wrapper allocation or packed numeric argument vector mediates that relationship. Evaluation serializes a borrowed native JSON value through projected `serde_json::to_string` before the source document parser consumes it.

Source constructs `LaunchOptions` with the pinned upstream defaults and calls `Browser::new`; it owns the returned browser, fresh tab, and event receiver in managed page state. The fresh tab avoids Chrome's privileged new-tab CSP, which blocks scripts in installed HTML. Executable discovery is projected and retains its native error; the single inline launch conversion maps source executable text to an owned `PathBuf`. Remove it when path construction projects. A private stack record carries the native event receiver/error alternative into source error handling. The diagnostic poll bridge maps crossbeam's typed `Empty` versus `Disconnected` result; remove these event crossings when retained event ownership and that result distinction project directly to Terrane.

Browser wall-clock and sleep primitives remain because the native `Instant` is opaque and `/core/time` is virtual under the native test runner. Remove those primitives when a projected host clock/sleep path exists. No retained Rust operation exists to avoid short-lived clones.

The backend uses `headless_chrome` 1.0.22. Native types are not part of the public API.

The private `rust/events.rs` observer handles an upstream event-parser incompatibility with Terrane's exact-number JSON features: fractional timestamps otherwise cause console/network events to be silently discarded. It parses a local, target-scoped CDP event stream as JSON values without changing document-number semantics. This compatibility bridge and its removal criteria are documented beside the implementation; it is not a public raw-CDP API.

Retained callback/carrier projection is available, but it does not by itself make
upstream event decoding compatible. A probe in this package's resolved dependency
graph accepts the raw JSON event with timestamp `123.456`, while decoding it as
`headless_chrome::protocol::cdp::types::Event` rejects it with
`invalid type: map, expected f64`. The observer therefore remains until the JSON
boundary and native event decoding are repaired and actual fractional-timestamp
console/network delivery passes. Upstream also invokes listeners while holding its
registration mutex; its add/remove operations must not be called reentrantly from
a listener. Generic retained-callback support does not change that native contract.


## Development

With `terrane` available on `PATH`, run from this package directory:

```sh
terrane check .
terrane test . --tier unit
```

For real-browser tests, set `CHROME` to the installed Chromium/Chrome executable and pass that path to the test runner:

```sh
terrane test . --argument "$CHROME" --timeout 60s
```

The real-browser cases exercise page operations against controlled fixtures. Without a browser argument they skip; the missing-executable unit regression needs no installed browser. The docs-converter suite separately checks its PDF consumer behavior.
