# proofrun

`proofrun` is a deterministic proof-plan compiler for Cargo workspaces.

It takes a Git change range plus checked-in repo policy and turns that into the smallest credible verification plan for the change, the exact commands to run, and portable artifacts that can be reviewed by humans, CI, or agents.

This bundle includes two parallel tracks:

1. a **runnable reference implementation** in `reference/proofrun_ref.py`
2. a **target Rust workspace layout** in `crates/` that mirrors the intended production architecture

The reference implementation exists so the repo is useful immediately. The Rust workspace exists so the end-state architecture is explicit from day one.

## What is in the box

- `reference/proofrun_ref.py` — stdlib-only reference CLI
- `crates/` — target Rust crate layout
- `schema/` — versioned JSON Schemas for config, plan, and receipt artifacts
- `examples/proofrun.toml` — example repo policy
- `fixtures/demo-workspace/` — sample Cargo workspace, Git history, and sample outputs
- `docs/` — architecture, policy model, CLI contract, scenario atlas, roadmap
- `AGENTS.md`, `MAINTAINERS.md`, `TESTING.md`, `RELEASE.md` — repo operating surfaces

## Commands

The runnable path today is the reference CLI:

```bash
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit github-actions --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
```

The target product surface remains:

```bash
cargo proofrun plan --base <rev> --head <rev> --profile ci
cargo proofrun explain --plan .proofrun/plan.json
cargo proofrun run --plan .proofrun/plan.json
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

This build environment did not include a Rust toolchain, so the reference implementation is the runnable artifact today. The Rust workspace layout is still included so the intended production architecture is concrete and ready to continue.
