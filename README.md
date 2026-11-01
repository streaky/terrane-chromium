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

The public page API is documented in the [manual reference](docs/src/records/api/page.yaml). `open-page` accepts `browser-options` and `viewport-options`, returning a managed page. Configure browser `executable` (default `none` for discovery), `timeout-ms` (30000), and bounded `diagnostic-capacity` (1000). Viewport defaults are 1280×720 at device scale factor 1.0.

The page supports URL navigation and HTML installation with load readiness options, viewport changes, selector/expression waits, JSON-compatible evaluation, PNG/JPEG screenshots, PDF printing, diagnostics, and idempotent close. `is-open` is the only operation that does not throw `chromium-error`; close is safe to repeat. Page cleanup also follows final shared ownership destruction. Evaluation exceptions, rejected promises, undefined and other non-JSON results are errors. PDF paper sizes, custom paired dimensions, margins, scale, orientation, page ranges, CSS precedence, and print-media behavior are detailed in the manual reference.

Full-page screenshot captures the current document extent, not all lazy or infinite-scroll content. Use page evaluation and application-specific waits to prepare content; no universal exhaustion guarantee exists. Diagnostics drain bounded best-effort events and are not a complete waterfall, HAR, or audit log.

## Security

The library does not sanitize HTML or JavaScript, restrict URL schemes or network destinations, or promise SSRF, filesystem, or network isolation. Active content, redirects, and browser vulnerabilities can access resources available under the host's privileges. Browser sandboxing remains enabled but is not network policy. Constrain untrusted content using caller-managed OS/container and network controls. Captures and diagnostics may expose private page data and secret-bearing URLs; redact and access-control them. Cookies are not returned deliberately. Operation timeouts are not overall CPU, memory, or network budgets.

No page clicking/typing, multi-page automation, downloads, raw CDP, or complete waterfall/HAR API is provided.

## Native boundary

Public types belong to `streaky/chromium`; native browser and tab handles are private. Terrane retains the `Arc<Tab>` returned at page creation and uses ordinary projected methods for directly supported operations such as close. The maintained backend still accounts for browser launch lifetime inputs and generic CDP/associated responses not expressible through current projection. Those remaining boundaries are projection limitations, not policy requirements; native code should be removed as the relevant calls and responses become expressible through source. It is not retained to avoid short-lived clones.

The backend uses `headless_chrome` 1.0.22. Native types are not part of the public API.

The private `rust/events.rs` observer handles an upstream event-parser incompatibility with Terrane's exact-number JSON features: fractional timestamps otherwise cause console/network events to be silently discarded. It parses a local, target-scoped CDP event stream as JSON values without changing document-number semantics. This compatibility bridge and its removal criteria are documented beside the implementation; it is not a public raw-CDP API.


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
