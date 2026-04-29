# proofrun plan

- range: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb..75883c2489a7eb2807c62e886076a459df88b45b`
- merge base: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb`
- profile: `ci`
- plan digest: `f8b1bb07f55eab18ebd6f0964e92c12a8adb1232f79e46e4243a67474cefcecf`

## Changed paths

- `D` `crates/app/src/main.rs` → `app`
- `R` `crates/core/src/core_renamed.rs` → `core`
- `M` `docs/guide.md` → `unowned`

## Obligations

- `pkg:app:mutation-diff`
  - source=rule, path=crates/app/src/main.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `pkg:app:tests`
  - source=rule, path=crates/app/src/main.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `pkg:core:mutation-diff`
  - source=rule, path=crates/core/src/core_renamed.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `pkg:core:tests`
  - source=rule, path=crates/core/src/core_renamed.rs, rule=rule:1, pattern=crates/*/src/**/*.rs
- `workspace:docs`
  - source=rule, path=docs/guide.md, rule=rule:3, pattern=docs/**
- `workspace:smoke`
  - source=profile, path=None, rule=ci, pattern=None

## Selected surfaces

- `mutation.diff[pkg=app]` — cost `13.0`
  - covers: pkg:app:mutation-diff
  - run: `cargo mutants --in-diff H:/Code/Rust/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package app`
- `mutation.diff[pkg=core]` — cost `13.0`
  - covers: pkg:core:mutation-diff
  - run: `cargo mutants --in-diff H:/Code/Rust/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch --package core`
- `tests.pkg[pkg=app]` — cost `3.0`
  - covers: pkg:app:tests
  - run: `cargo nextest run --profile ci -E 'package(app)'`
- `tests.pkg[pkg=core]` — cost `3.0`
  - covers: pkg:core:tests
  - run: `cargo nextest run --profile ci -E 'package(core)'`
- `workspace.docs` — cost `4.0`
  - covers: workspace:docs
  - run: `cargo doc --workspace --no-deps`
- `workspace.smoke` — cost `2.0`
  - covers: workspace:smoke
  - run: `cargo test --workspace --quiet`
