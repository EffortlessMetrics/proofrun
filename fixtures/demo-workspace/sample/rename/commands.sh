#!/usr/bin/env bash
set -euo pipefail

# mutation.diff[pkg=core]
cargo mutants --in-diff H:/Code/Rust/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package core

# tests.pkg[pkg=core]
cargo nextest run --profile ci -E 'package(core)'

# workspace.docs
cargo doc --workspace --no-deps

# workspace.smoke
cargo test --workspace --quiet
