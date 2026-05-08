# Coverage

Codecov coverage is Rust execution-surface evidence.

## What it answers

> Did tests execute this Rust surface?

## What it does NOT answer

- whether the Python reference implementation and Rust implementation are behaviorally equivalent,
- whether proof plans are correct,
- whether the weighted set-cover solution is optimal for a policy,
- whether generated shell or GitHub Actions commands are semantically correct,
- whether fixture golden artifacts are current,
- whether publication dry-runs are sufficient,
- whether release readiness is proven.

Those are separate proof lanes.

## Workflow

The Coverage workflow runs on:

- `push` to `main`
- `workflow_dispatch` (manual trigger)
- PRs labeled `coverage`, `full-ci`, or `ci:full`

## Artifacts and receipts

Durable receipts are:

- `coverage.json` — machine-readable coverage data
- `coverage.txt` — human-readable coverage report
- `lcov.info` — LCOV format coverage data
- GitHub Actions coverage artifact (14-day retention)
- Codecov dashboard

Codecov comments and GitHub check annotations are disabled to keep signal clean.

## Coverage flag

The `rust-core` flag tracks Rust workspace coverage only.
It ignores:

- `target/` — build artifacts
- `reference/` — Python reference implementation
- `tests/` — test infrastructure
- `fixtures/` — sample repos and golden artifacts
- `schema/` — JSON schemas
- `crates/*/benches/**` — benchmark code
- `crates/*/examples/**` — example code

## Interpretation

Coverage gaps suggest untested code paths. Increasing coverage may indicate better test
thoroughness, but coverage alone does not prove proof-plan correctness, fixture golden
equivalence, or release readiness.
