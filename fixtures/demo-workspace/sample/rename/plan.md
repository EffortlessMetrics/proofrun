# proofrun plan

- range: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb..91ebd2444afe6b8a600f6dc984171fa58bf7d4ac`
- merge base: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb`
- profile: `ci`
- plan digest: `31c7b6d43b09925620c3da9e43546d44759ec01ef5706f39b9bc25c50c810a18`

## Changed paths

- `R` `crates/core/src/core_renamed.rs` → `core`
- `M` `docs/guide.md` → `unowned`

## Obligations

- `pkg:core:mutation-diff`
  - source=rule, path=crates/core/src/core_renamed.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `pkg:core:tests`
  - source=rule, path=crates/core/src/core_renamed.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `workspace:docs`
  - source=rule, path=docs/guide.md, rule=rule:3, pattern=docs/**
- `workspace:smoke`
  - source=profile, path=None, rule=ci, pattern=None

## Selected surfaces

- `mutation.diff[pkg=core]` — cost `13.0`
  - covers: pkg:core:mutation-diff
  - run: `cargo mutants --in-diff H:/Code/Rust/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package core`
- `tests.pkg[pkg=core]` — cost `3.0`
  - covers: pkg:core:tests
  - run: `cargo nextest run --profile ci -E 'package(core)'`
- `workspace.docs` — cost `4.0`
  - covers: workspace:docs
  - run: `cargo doc --workspace --no-deps`
- `workspace.smoke` — cost `2.0`
  - covers: workspace:smoke
  - run: `cargo test --workspace --quiet`
