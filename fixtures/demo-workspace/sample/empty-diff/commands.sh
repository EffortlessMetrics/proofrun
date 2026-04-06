#!/usr/bin/env bash
set -euo pipefail

# workspace.smoke
cargo test --workspace --quiet
