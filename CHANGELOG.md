# Changelog

## Unreleased

- Initial publish-rail groundwork for Rust CLI release path.
- Added release and packaging metadata for `cargo-proofrun`.
- Added schema embedding for packaged crates (`crates/proofrun/schema/*.json`) to support publish checks.
- Added `proofrun` crate publish metadata so CLI dependency can be published.
- Added release guardrails: `proofrun` dry-run/package checks and explicit CLI publish ordering (`proofrun` then `cargo-proofrun`).
- Added crates.io propagation handling so CI and release workflows only dry-run or publish `cargo-proofrun` once the matching `proofrun` version is visible.
- Added GitHub release workflow that publishes platform binaries.
- Aligned README and roadmap language with the current Rust CLI status and unreleased public alpha state.
- Added `cargo proofrun explain --solver` and solver-aware omitted-surface cost/coverage explanation without changing the plan artifact schema.

## 0.1.0-alpha.1

- Internal: Rust port now includes semantic-trust conformance slices, golden fixture checks, and schema validation.
