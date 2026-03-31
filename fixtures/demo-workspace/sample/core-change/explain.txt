# proofrun plan

- range: `8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717..ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb`
- merge base: `8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717`
- profile: `ci`
- plan digest: `66c833f979ebd3f92cd7dcb1a350c67242fbbe614c55e5100487e9625167776c`

## Changed paths

- `M` `crates/core/src/lib.rs` → `core`

## Obligations

- `pkg:core:mutation-diff`
  - source=rule, path=crates/core/src/lib.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `pkg:core:tests`
  - source=rule, path=crates/core/src/lib.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `workspace:smoke`
  - source=profile, path=None, rule=ci, pattern=None

## Selected surfaces

- `mutation.diff[pkg=core]` — cost `13.0`
  - covers: pkg:core:mutation-diff
  - run: `cargo mutants --in-diff /mnt/data/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package core`
- `tests.pkg[pkg=core]` — cost `3.0`
  - covers: pkg:core:tests
  - run: `cargo nextest run --profile ci -E 'package(core)'`
- `workspace.smoke` — cost `2.0`
  - covers: workspace:smoke
  - run: `cargo test --workspace --quiet`
