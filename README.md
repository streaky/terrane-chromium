# streaky/chromium

A Terrane library for Chromium-backed operations. Its current surface is deliberately small: installed-browser discovery and HTML-to-PDF rendering. It extracts the browser boundary previously owned by docs-converter, without introducing a second document renderer or a general browser-automation API.

## Manual

The [Terrane manual IR publication](docs/src/manual.yaml) follows the same `docs/src/` layout as dbkit. It contains an [introduction and runtime setup](docs/src/records/guides/introduction.yaml), a [complete render-and-save tutorial](docs/src/records/tutorials/render-pdf.yaml), and the [public API reference](docs/src/records/api/pdf.yaml), including options, errors, asset handling, and security boundaries.

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

## HTML to PDF

```terrane
from /streaky/chromium import browser-options, pdf-options, render-pdf

browser = instance browser-options;
printing = instance pdf-options;
printing.print-background = true
printing.prefer-css-page-size = true
html = '<!doctype html><h1>Report</h1>'
pdf = render-pdf; html, browser, printing
```

`render-pdf` borrows the HTML and options, returns PDF `bytes`, and throws `chromium-error` on browser, document-installation, asset-readiness, or printing failure. Call it from a function whose error contract admits `chromium-error`, or catch that error. The caller owns persistence and any atomic-file-write policy; rendering itself never writes an output file.

### Browser options

- `executable string|none`, default `none`: an explicit Chromium/Chrome executable, or automatic installed-browser discovery.
- `find-executable;` returns an installed executable filename or `none`. Discovery honors `CHROME` and the backend's normal platform search locations. This is discovery, not a browser launch or download.

The package launches a headless browser with its sandbox enabled and a temporary private profile. It does not download Chromium or silently disable its sandbox. Install Chromium/Chrome, set `CHROME`, or supply `browser.executable`. On Unix, keep `TMPDIR` short enough for Chromium's Unix-domain sockets; the normal `/tmp` default works.

### Print options

All four fields default to `false`:

- `display-header-footer`: include Chromium's print headers and footers.
- `print-background`: print background graphics.
- `prefer-css-page-size`: honor the page size declared by the document's CSS.
- `expand-disclosures`: open HTML `details` elements before printing. This is opt-in; ordinary rendering preserves the document's disclosure state.

Rendering installs the supplied HTML DOM directly, waits for embedded images to decode and document fonts to be ready, then invokes Chromium's print engine. It does not convert through Markdown. Relative URLs have no caller-selected base URL; supply self-contained HTML, embedded assets, or deliberately absolute URLs. HTML is active browser content, not sanitized documentation IR: use trusted HTML or apply your own content policy before calling the library. Browser sandboxing is not a network-access policy.

## Boundaries and future use

Public types belong to `streaky/chromium`, not to its Rust backend. The current one-shot operation owns and closes its browser/tab lifetime before returning, and exposes no native handles. A future reusable session or another Chromium operation can be added alongside this convenience function without changing its byte-returning contract. No session, screenshot, navigation, arbitrary-script, browser-download, or sandbox-disable API is implemented or promised here.

A narrow maintained [Rust module](rust/backend.rs) owns the current browser integration boundary because configured browser construction and operations through its opaque `Arc<Tab>` are not admitted by Terrane projection. It is not retained to avoid small, short-lived clones. Comments in that file explain each operation, distinguish policy from projection gaps, and state the removal criterion. The small native path-to-text discovery conversion lives directly in a Terrane `rust` block. See the manual's [native-boundary accounting](docs/src/records/guides/introduction.yaml) for the current gaps. Document structure and output delivery stay with callers.

The backend is pinned to `headless_chrome` 1.0.22. [Upstream documentation](https://github.com/rust-headless-chrome/rust-headless-chrome) describes its installed-browser discovery and Chromium integration; those native types are not part of this package's public API.

## Development

With `terrane` available on `PATH`, run from this package directory:

```text
terrane check .
terrane test .
```

The unit regression checks typed failure for a nonexistent explicit executable and does not require an installed browser. The converter's real-PDF regression remains the end-to-end consumer check for code/prose boundaries, image/font readiness, disclosure visibility, pagination, and atomic output delivery.
