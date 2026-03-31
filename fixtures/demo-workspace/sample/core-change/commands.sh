#!/usr/bin/env bash
set -euo pipefail

# mutation.diff[pkg=core]
cargo mutants --in-diff /mnt/data/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package core

# tests.pkg[pkg=core]
cargo nextest run --profile ci -E 'package(core)'

# workspace.smoke
cargo test --workspace --quiet
