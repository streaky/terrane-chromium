# Changelog

## Unreleased

- Added a managed page API for navigation, HTML installation, readiness waits, evaluation, screenshots, PDF printing, diagnostics, and explicit cleanup.
- Added browser, viewport, load, screenshot, and expanded PDF options; diagnostics are bounded best-effort event snapshots.
- Documented security limits, caller-managed confinement, and current-extent capture behavior for lazy and infinite-scroll content.
- Preserved the existing HTML-to-PDF convenience operation and docs-converter call contract.
- Preserved console/network evidence under Terrane's exact-number JSON dependency features using a private target-scoped event observer; navigation readiness no longer relies on the incompatible upstream event parser.
