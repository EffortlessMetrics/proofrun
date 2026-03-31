#!/usr/bin/env bash
set -euo pipefail

# workspace.docs
cargo doc --workspace --no-deps

# workspace.smoke
cargo test --workspace --quiet
