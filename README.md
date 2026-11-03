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

Full-page screenshot captures the current document extent, not all lazy or infinite-scroll content. Use page evaluation and application-specific waits to prepare content; no universal exhaustion guarantee exists.

## Diagnostics and recovery

`diagnostics` returns page metadata, navigation/resource timings, the main HTTP status when observed, and normalized console, JavaScript-error, request, response, request-failed, and `capture-error` events. HTML installation has no HTTP status; HTTP status codes do not themselves determine whether navigation succeeds or fails. Capture is bounded and best-effort, not a complete waterfall, HAR, or audit log.

Each call atomically drains the event buffer. A full buffer rejects new records without waiting for space. `capacity-reached` describes the just-drained batch and can clear on a later drain; `dropped-events` remains cumulative for the page lifetime and does not count events lost upstream.

Normalization failures and callback exceptions become drainable `capture-error` records rather than permanently failing subsequent diagnostics. Callback exceptions share a bounded first-failure slot, reset with each drain. If the batch is full, that error replaces its last record and the displacement counts as a drop; further exceptions while the slot is occupied also count as drops. Native event payload conversion has a fixed **16 MiB (16,777,216-byte) encoding budget**. Exceeding it produces a `capture-error`, not a truncated payload.

Diagnostics also performs a live CDP evaluation: dependency failures, including an unexpectedly closed browser connection, surface as `chromium-error`. There is no independent disconnect notification or transport-health monitor; `is-open` describes local page ownership, not whether the native connection is still healthy.

## Security

The library does not sanitize HTML or JavaScript, restrict URL schemes or network destinations, or promise SSRF, filesystem, or network isolation. Active content, redirects, and browser vulnerabilities can access resources available under the host's privileges. Browser sandboxing remains enabled but is not network policy. Constrain untrusted content using caller-managed OS/container and network controls. Captures and diagnostics may expose private page data and secret-bearing URLs; redact and access-control them. Cookies are not returned deliberately. Operation timeouts are not overall CPU, memory, or network budgets.

No page clicking/typing, multi-page automation, downloads, raw CDP, or complete waterfall/HAR API is provided.

## Native boundary

Public types belong to `streaky/chromium`; native browser and tab handles are private. Terrane retains the `Arc<Tab>` returned at page creation. Navigation, typed CDP requests/responses, readiness, evaluation, viewport/screenshot capture, printing, and diagnostic aggregation are package source policy, not Rust adapters.

The private associated-type contract binds each operation to its projected response type. Source methods call the retained tab's generic CDP operation directly; no request wrapper allocation or packed numeric argument vector mediates that relationship. Evaluation converts a borrowed native JSON value directly into a document through the explicit JSON adapter, without serializing and reparsing it.

Source constructs `LaunchOptions` with the pinned upstream defaults and calls `Browser::new`; managed page state owns the browser, fresh tab, retained listener token and checked shared diagnostic buffer. The fresh tab avoids Chrome's privileged new-tab CSP, which blocks scripts in installed HTML. Executable discovery is projected and retains its native error; a small inline conversion maps executable text to owned `PathBuf`. Remove it when path construction projects.

Browser wall-clock and sleep primitives remain because the native `Instant` is opaque and `/core/time` is virtual under the native test runner. Remove those primitives when a projected host clock/sleep path exists. No retained Rust operation exists to avoid short-lived clones.

The backend uses `headless_chrome` 1.0.22. Native types are not part of the public API.

Typed events now arrive through the retained tab's native listener. The exact native `Weak<dyn EventListener<Event> + Send + Sync>` registration token is retained and passed back for removal. There is no additional CDP socket, polling worker, maintained Rust module, or direct WebSocket dependency.

`src/events.trn` owns event selection/normalization and bounded buffering. Normalization occurs outside the short checked shared-state lock; callback error documents are also materialized after releasing that lock. Explicit close removes the listener before releasing the tab/browser. Buffer limits and recovery semantics are described under [Diagnostics and recovery](#diagnostics-and-recovery).

The remaining inline event crossings extract borrowed serializable variant payloads (the upstream `Event` enum is Deserialize-only), adapt native JSON values, join a dynamic string list, and check weak-token liveness. Each has its local capability/removal rationale. No event is cloned or stringified and reparsed at this boundary.

The document runtime preserves exact numbers without enabling Serde JSON's private-number feature. The resolved upstream graph accepts fractional typed event timestamps such as `123.456`; console/network delivery is covered by real-browser tests. Upstream still invokes listeners while holding its registration mutex: callbacks must not reenter listener add/remove or issue blocking browser operations. Our callback only normalizes and enqueues.

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
