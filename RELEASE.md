# RELEASE

This bundle is a scaffold plus runnable reference implementation.

## Before publishing

- choose a final license
- install a Rust toolchain
- port the reference implementation into `crates/proofrun`
- wire the Cargo subcommand in `crates/cargo-proofrun`
- replace placeholder CI steps with real Rust builds

## Draft release process

1. run the Python reference tests
2. regenerate fixture artifacts
3. review schemas and docs
4. archive the repo
5. tag the version
