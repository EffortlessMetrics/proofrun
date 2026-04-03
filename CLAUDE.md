# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

proofrun is a deterministic proof-plan compiler for Cargo workspaces. Given a Git change range and checked-in policy (`proofrun.toml`), it derives proof obligations from the diff, then solves an exact weighted set cover to select the smallest set of verification surfaces that discharge those obligations.

**Dual-track repo:** `reference/proofrun_ref.py` is the authoritative, runnable reference implementation (stdlib-only Python). `crates/` contains the target Rust production architecture (substantially complete). The reference defines current behavior; Rust must mirror it.

## Commands

```bash
# Run tests (reference implementation)
python3 -m unittest discover -s tests -p 'test_*.py'

# Rust build/check
cargo build --workspace
cargo clippy --workspace
cargo fmt --check
cargo test --workspace

# Reference CLI
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run

# Just shortcuts
just test          # run Python tests
just doctor        # check environment
just plan <base> <head> [profile]
just explain       # explain last plan
just dry-run       # dry-run last plan
```

## Architecture

### Domain Model (signal -> obligation -> surface -> plan)

- **signal** -- fact from the diff (e.g., "crates/core/src/lib.rs changed")
- **obligation** -- proof requirement derived from signals (e.g., `pkg:core:tests`)
- **surface** -- runnable verification step that covers obligations (e.g., `cargo nextest run`)
- **plan** -- solver-selected surfaces with commands and traceability
- **receipt** -- execution record with exit codes and timings

### Pipeline

```
Git range -> changed paths -> rule matching (path->package ownership)
  -> obligations -> candidate surfaces -> exact weighted set cover
  -> plan artifacts (plan.json, plan.md, commands.sh, github-actions.yml)
  -> [optional] execute -> receipt.json
```

### Key Boundaries

- **Planning core is pure:** `model.rs`, `planner.rs`, `obligations.rs` must be deterministic with zero I/O.
- **Adapters at edges:** `git.rs`, `cargo_workspace.rs`, `run.rs` handle I/O.
- **Rules emit obligations, not commands.** Surfaces cover obligations. The solver selects surfaces.
- **Fail-closed:** Unknown file ownership triggers fallback obligations and diagnostics.

### Workspace Crates

- `crates/proofrun/` -- planning library (model, config, solver, emitters, explain)
- `crates/cargo-proofrun/` -- `cargo proofrun` subcommand binary
- `crates/xtask/` -- repo automation tasks

## Key Design Rules

1. Rules emit obligations, not commands. Surfaces cover obligations. Solver selects.
2. Planning core stays pure and deterministic -- no I/O in model/planner/obligations.
3. Fail-closed by default -- unknown ownership triggers fallback obligations.
4. Reference implementation (`proofrun_ref.py`) is source of truth for current behavior.
5. Changes to planning logic must regenerate and review fixture artifacts.
6. Artifact shape changes (plan.json, receipt.json) require JSON schema updates in `schema/`.

## Escalation Triggers

Stop and ask before proceeding if a change would:
- Require new `proofrun.toml` config schema semantics
- Alter plan or receipt artifact shape without schema update
- Weaken fail-closed behavior
- Introduce network or service requirements
- Widen scope beyond Cargo workspaces without a design update

## Testing

- **Unit tests:** `python3 -m unittest discover -s tests -p 'test_*.py'`
- **Fixture repo:** `fixtures/demo-workspace/repo` with scenarios `core-change` and `docs-change`
- **Golden artifacts:** `fixtures/demo-workspace/sample/{scenario}/` -- regenerate when planning logic changes
- **Scenario atlas:** `docs/scenario-atlas.md` documents expected behavior per change type

## Key Files

- `AGENTS.md` -- crate map, escalation rules, proof expectations
- `TESTING.md` -- test layers, fixture update process
- `MAINTAINERS.md` -- merge criteria, release checks
- `docs/architecture.md` -- pipeline details and boundaries
- `docs/policy.md` -- why rules emit obligations, not commands
- `.kiro/steering/` -- product vision, tech stack, project structure
- `proofrun.toml` -- this repo's own policy (good example config)
- `schema/` -- JSON Schemas for config, plan, and receipt artifacts
