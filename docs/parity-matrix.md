# Parity Matrix: Python Reference → Rust Implementation

This document tracks behavioral parity between the Python reference (`reference/proofrun_ref.py`) and the Rust production implementation (`crates/proofrun/`, `crates/cargo-proofrun/`).

## Core Pipeline

| Behavior | Python | Rust Module | Status | Notes |
|----------|--------|-------------|--------|-------|
| Config loading | `load_config()` | `config.rs` | Complete | Identical semantics; default config verified by tests |
| Workspace scanning | `scan_workspace()` | `cargo_workspace.rs` | Complete | Python: `os.walk` + TOML parse; Rust: `cargo metadata`. Semantically equivalent for standard workspaces |
| Path ownership | `owner_for_path()` | `cargo_workspace.rs` | Complete | Longest-prefix matching; property-tested |
| Git change collection | `collect_git_changes()` | `git.rs` | Complete | merge-base, name-status, binary patch |
| Name-status parsing | inline in `collect_git_changes` | `git.rs::parse_name_status_line()` | Complete | M/A/D/R/C handling; property-tested |
| Glob-to-regex | `glob_to_regex()` | `obligations.rs` | Complete | `**/`, `**`, `*`, `?` semantics match; property-tested |
| Path matching | `match_path()` | `obligations.rs` | Complete | Leading `/` stripped; regex-based |
| Template expansion | `expand_template()` | `obligations.rs` | Complete | `{key}` placeholders with error on missing keys |
| Obligation derivation | `derive_obligations()` | `obligations.rs::compile_obligations()` | Complete | Rule matching, owner resolution, fallback handling |
| Candidate building | `build_candidates()` | `planner.rs` | Complete | fnmatch cover matching, dedup, sorting |
| Set cover solver | `solve_exact_cover()` | `planner.rs` | Complete | Branch-and-bound with identical tie-breaking |
| Plan assembly | `build_plan()` | `lib.rs::plan_repo()` | Complete | Full pipeline orchestration with digest computation |
| Plan markdown | `plan_markdown()` | `emit.rs::emit_plan_markdown()` | Complete | Markdown rendering |
| Shell emit | `emit_shell()` | `emit.rs::emit_commands_shell()` | Complete | Bash script generation |
| GitHub Actions emit | `emit_github_actions()` | `emit.rs::emit_github_actions()` | Complete | YAML step generation |
| Plan execution | `execute_plan()` | `run.rs::execute_plan()` | Complete | Subprocess spawning, logging, timing |
| Doctor | `doctor()` | `doctor.rs::doctor_repo()` | Complete | Base checks match; Rust adds enriched findings |
| CLI: plan | `plan` subcommand | `cargo-proofrun plan` | Complete | Identical flags plus extensions |
| CLI: explain | `explain` subcommand | `cargo-proofrun explain` | Complete | Plus path/obligation/surface queries |
| CLI: emit shell | `emit shell` subcommand | `cargo-proofrun emit shell` | Complete | Identical |
| CLI: emit github-actions | `emit github-actions` subcommand | `cargo-proofrun emit github-actions` | Complete | Identical |
| CLI: run | `run` subcommand | `cargo-proofrun run` | Complete | Plus resume/failed-only |
| CLI: doctor | `doctor` subcommand | `cargo-proofrun doctor` | Complete | Plus `--strict` flag |
| Canonical JSON | `canonical_json()` | `lib.rs::canonical_json()` | Complete | Sorted keys, compact separators |
| SHA-256 digest | `sha256_text()` | `lib.rs::sha256_hex()` | Complete | UTF-8 bytes, lowercase hex |
| UTC timestamp | `utc_now()` | `lib.rs::utc_now()` | Complete | ISO 8601 with Z suffix, seconds precision |

## Rust Extensions (not in Python reference)

These features exist only in the Rust implementation. They are self-authoritative — not required to be backported to Python.

| Feature | Rust Location | Purpose |
|---------|---------------|---------|
| `ChangeSource::Staged` | `lib.rs`, `git.rs` | Plan from `git diff --cached` |
| `ChangeSource::WorkingTree` | `lib.rs`, `git.rs` | Plan from `git diff HEAD` |
| `ChangeSource::PathsFromStdin` | `lib.rs` | Plan from explicit path list |
| Budget gates | `lib.rs::check_budget_gates()` | `--max-cost`, `--max-surfaces`, `--fail-on-fallback` |
| Plan comparison | `compare.rs` | Structural diff between two plans |
| Trace command | `explain.rs::trace_plan()` | Full path→rule→obligation→surface traceability |
| Explain queries | `explain.rs` | `--path`, `--obligation`, `--surface` targeted queries |
| Matrix emit | `emit.rs::emit_matrix_json()` | CI matrix strategy JSON |
| Nextest filtersets emit | `emit.rs::emit_nextest_filtersets()` | Nextest `-E` expressions |
| Structured JSON emit | `emit.rs::emit_structured_json()` | Compact plan subset for pipelines |
| Resume execution | `run.rs::execute_with_resume()` | Skip passed steps from previous receipt |
| Failed-only execution | `run.rs::execute_failed_only()` | Re-run only failed steps |
| Enriched doctor findings | `doctor.rs` | Duplicate surfaces, uncovered obligations, unreachable rules, missing tools |
| Strict doctor mode | `doctor.rs::should_fail_strict()` | Exit non-zero on errors |

## Known Divergence: Workspace Discovery

The Python reference uses `os.walk()` + manual TOML parsing to discover packages. The Rust implementation uses `cargo metadata --format-version 1 --no-deps`.

Both approaches are semantically equivalent for standard Cargo workspaces. Potential edge cases where they could differ:
- Workspaces with optional members
- Path dependency overrides
- Non-standard manifest locations

This divergence is accepted. The Rust approach is preferred for production as it uses the official Cargo API.

## Parity Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Canonical JSON float formatting | Medium | Conformance tests compare outputs |
| Shell quoting edge cases | Medium | Property tests cover special characters |
| Glob pattern `a/**/b` semantics | Low | Property-tested with 200+ cases |
| Solver tie-breaking on equal costs | Low | Deterministic via lexicographic signature |
| Path separator normalization (Windows) | Medium | Explicit `\\` → `/` conversion in Rust |
