# proofrun

`proofrun` is a deterministic proof-plan compiler for Cargo workspaces.

It takes a Git change range plus checked-in repo policy and turns that into the smallest credible verification plan for the change, the exact commands to run, and portable artifacts that can be reviewed by humans, CI, or agents.

This bundle includes two parallel tracks:

1. a **runnable reference implementation** in `reference/proofrun_ref.py`
2. a **Rust workspace implementation** in `crates/` that carries the primary CLI, library, and release rail

The reference implementation remains the authoritative behavioral spec. The Rust workspace is the primary in-repo operator surface and the path intended for public release.

## What is in the box

- `reference/proofrun_ref.py` — stdlib-only reference CLI
- `crates/` — Rust library and Cargo subcommand implementation
- `schema/` — versioned JSON Schemas for config, plan, and receipt artifacts
- `examples/proofrun.toml` — example repo policy
- `fixtures/demo-workspace/` — sample Cargo workspace, Git history, and sample outputs
- `docs/` — architecture, policy model, CLI contract, scenario atlas, roadmap
- `AGENTS.md`, `MAINTAINERS.md`, `TESTING.md`, `RELEASE.md` — repo operating surfaces

## Commands

The primary in-repo CLI path is the Rust implementation:

```bash
cargo run -p cargo-proofrun -- doctor
cargo run -p cargo-proofrun -- plan --base <rev> --head <rev> --profile ci
cargo run -p cargo-proofrun -- explain --plan .proofrun/plan.json
cargo run -p cargo-proofrun -- run --plan .proofrun/plan.json --dry-run
```

The reference/spec path remains available:

```bash
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit github-actions --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
```

Installed usage is the intended public surface:

```bash
cargo proofrun plan --base <rev> --head <rev> --profile ci
cargo proofrun explain --plan .proofrun/plan.json
cargo proofrun run --plan .proofrun/plan.json
```

## Status

- `cargo-proofrun` is the CLI to operate locally in this repo.
- `reference/proofrun_ref.py` remains the authoritative behavioral reference and parity oracle.
- The supported public surface is the CLI first. The `proofrun` library crate is not yet documented as a stable public API.

## Install

When the public alpha is visible on crates.io, install it with Cargo:

```bash
cargo install cargo-proofrun
cargo proofrun --help
cargo proofrun plan --repo fixtures/demo-workspace/repo --base initial --head core_change --profile ci
```

Until that alpha is visible, use the in-repo Rust CLI:

```bash
cargo run -p cargo-proofrun -- --help
cargo run -p cargo-proofrun -- plan --repo fixtures/demo-workspace/repo --base initial --head core_change --profile ci
```

## What the tool does

A diff does not select tests directly. A diff creates **proof obligations**. `proofrun` selects the smallest set of **surfaces** that discharge those obligations.

Examples:

- editing `crates/core/src/lib.rs` can create:
  - `pkg:core:tests`
  - `pkg:core:mutation-diff`
  - `workspace:smoke`

- editing `crates/core/Cargo.toml` can create:
  - `pkg:core:tests`
  - `pkg:core:rdeps`
  - `workspace:smoke`

- editing only `docs/guide.md` can create:
  - `workspace:docs`
  - `workspace:smoke`

The planner solves an exact weighted set cover over candidate surfaces and emits:

- `plan.json`
- `plan.md`
- `commands.sh`
- `github-actions.yml`

Running the plan emits:

- `receipt.json`

## Current boundary

This bundle is intentionally narrow.

Included:

- Git range planning
- checked-in repo policy
- Cargo workspace scanning by manifest files
- path-to-package ownership
- package tests
- reverse-dependency tests
- docs surfaces
- mutation-on-diff surfaces
- workspace smoke nets
- exact deterministic planning
- portable artifacts

Deferred:

- ML-based prediction
- AST-level public API inference
- feature/unit-level Cargo graph reasoning
- polyglot repository graphs
- flaky analytics
- dashboards
- SaaS control plane

## Quick sample

The sample fixture repo has three commits recorded in:

- `fixtures/demo-workspace/sample/commits.json`

Two ready-made sample plans are included:

- `fixtures/demo-workspace/sample/core-change/plan.json`
- `fixtures/demo-workspace/sample/docs-change/plan.json`

## Repo map

```text
proofrun/
  AGENTS.md
  MAINTAINERS.md
  TESTING.md
  RELEASE.md
  examples/proofrun.toml
  schema/
  docs/
  reference/proofrun_ref.py
  fixtures/demo-workspace/
  crates/
```

## Notes

The Rust workspace covers the full planning pipeline plus extensions such as multiple change sources, budget gates, plan comparison, enriched diagnostics, and resume execution. The Python reference remains the authority for core behavior and regression parity.
