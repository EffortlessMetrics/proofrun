# Roadmap

## Done in this bundle

- runnable reference CLI
- checked-in schemas
- sample policy
- fixture repo with recorded plans
- Rust planning core in `crates/proofrun`
- Cargo subcommand in `crates/cargo-proofrun`
- semantic-trust layer: golden diffing, schema validation, conformance expansion
- publish rail: package-safe schema embedding, publish metadata, release workflow, crates.io propagation gate
- repo operating docs

## Next

1. merge the public-truth updates and verify the first public alpha release end-to-end
2. add `--explain-solver` and stronger omitted-surface rationale
3. add policy linting, dead-rule detection, and safer rule preview tooling
4. add PR/check-run integration and standard operating modes
5. add shadow-mode calibration against full CI
6. improve blast-radius fidelity for API, feature, and build-script changes
