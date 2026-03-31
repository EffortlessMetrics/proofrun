# Architecture

## Position

`proofrun` is a **policy compiler**.

It does not try to be a build system, a test runner, or an ML test picker. It compiles checked-in repo policy plus a Git change range into a deterministic proof plan.

## Core nouns

- **signal** — fact from the diff or repo shape
- **obligation** — proof that must exist
- **surface** — runnable proof step that can discharge obligations
- **plan** — selected surfaces, commands, and rationale
- **receipt** — what actually ran

## Pipeline

```text
Git range
  -> changed paths + patch
  -> ownership resolution
  -> obligation derivation
  -> candidate surface expansion
  -> exact weighted cover
  -> plan artifact
  -> optional execution
  -> receipt artifact
```

## Architectural boundaries

### Repo policy

The repo owns its law in `proofrun.toml`.

### Planning core

The planning core should stay pure and deterministic.

### Adapters

Messy adapters sit at the edges:

- Git
- Cargo workspace discovery
- nextest command emission
- cargo-mutants command emission
- output writing
- process execution

### Artifact surfaces

The first-class products are:

- `plan.json`
- `plan.md`
- `commands.sh`
- `github-actions.yml`
- `receipt.json`

## Direction

Keep the product narrow:

- Cargo workspaces first
- package-level planning first
- explainability over prediction
- checked-in policy over inference
