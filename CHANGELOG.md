# Changelog

## Unreleased

- Extracted HTML-to-PDF rendering from docs-converter into `streaky/chromium`.
- Exposed package-owned browser discovery, browser/print options, PDF bytes, and `chromium-error`; native Chromium types remain private.
- Preserved installed-browser discovery, sandboxed private profiles, image/font readiness, and optional disclosure expansion. Output persistence remains caller-owned.
