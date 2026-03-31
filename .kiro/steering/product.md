# Product: proofrun

proofrun is a deterministic proof-plan compiler for Cargo workspaces. It takes a Git change range plus checked-in repo policy (`proofrun.toml`) and produces the smallest credible verification plan for the change.

## Core concept

A diff does not select tests directly — it creates **proof obligations**. proofrun selects the smallest set of **surfaces** (runnable proof steps) that discharge those obligations via exact weighted set cover.

## Domain vocabulary

- **signal** — fact derived from the diff or repo shape
- **obligation** — proof that must exist for a change to be verified
- **surface** — runnable proof step that can discharge one or more obligations
- **plan** — selected surfaces, commands, and rationale (`plan.json`, `plan.md`)
- **receipt** — record of what actually ran (`receipt.json`)

## Key design principles

- Rules emit obligations, not commands. Surfaces cover obligations. The solver selects surfaces.
- Planning core must stay pure and deterministic.
- Fail-closed by default: unknown file ownership falls back to safety-net obligations.
- Explainability over prediction.
- Checked-in policy over inference.
- Scope is intentionally narrow: Cargo workspaces, package-level planning only.

## Dual-track implementation

1. `reference/proofrun_ref.py` — stdlib-only Python reference CLI (runnable today)
2. `crates/` — target Rust workspace (production architecture)

The reference implementation is authoritative for current behavior.
