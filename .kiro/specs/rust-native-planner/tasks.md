# Implementation Plan: Rust Native Planner

## Overview

Port `reference/proofrun_ref.py` into the existing Rust crate scaffold module-by-module, following data-flow dependency order. Each module is filled with logic ported directly from the reference, with property-based tests validating correctness alongside implementation. Fixture-based conformance tests serve as the primary acceptance gate.

## Tasks

- [ ] 1. Add crate dependencies and update model types
  - [ ] 1.1 Add `regex` and `proptest` dependencies to `crates/proofrun/Cargo.toml`
    - Add `regex = "1"` to `[dependencies]` and `proptest = "1"` to `[dev-dependencies]`
    - Add `regex` and `proptest` to `[workspace.dependencies]` in root `Cargo.toml`
    - _Requirements: 3.1, 3.5_

  - [ ] 1.2 Extend `model.rs` with missing types and fields
    - Add `WorkspaceInfo` and `WorkspacePackage` structs
    - Add `workspace: WorkspaceInfo` and `diagnostics: Vec<String>` fields to `Plan`
    - Ensure all structs derive `Serialize`, `Deserialize`, `Debug`, `Clone`
    - Ensure `ReceiptStep.duration_ms` is `u64` (not `u128`) to match reference JSON output
    - _Requirements: 8.8, 8.9, 1.1_

- [ ] 2. Implement config loading with default fallback
  - [ ] 2.1 Port `load_config` with built-in default config
    - Embed `DEFAULT_CONFIG_TOML` as a const string matching the reference
    - If `proofrun.toml` exists, read and parse it; otherwise parse the default
    - Apply serde defaults for `output_dir`, `fallback`, `mode`
    - _Requirements: 8.1, 13.2_

- [ ] 3. Implement utility functions: glob, template expansion, canonical JSON, digests
  - [ ] 3.1 Port `glob_to_regex` and `match_path` in `obligations.rs`
    - Translate `**/` → `(?:.*/)?`, trailing `**` → `.*`, `*` → `[^/]*`, `?` → `[^/]`
    - Strip leading `/` from both path and pattern before matching
    - Anchor with `^` and `$`
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

  - [ ]* 3.2 Write property test for glob-to-regex parity (Property 1)
    - **Property 1: Glob-to-regex parity with reference**
    - Generate random paths (segments of `[a-z0-9_.]` joined by `/`) and patterns (segments with `*`, `**`, `?` wildcards)
    - Verify `match_path` returns the same boolean as the Python reference for all generated inputs
    - **Validates: Requirements 3.1, 3.5**

  - [ ] 3.3 Port `expand_template` in `obligations.rs`
    - Use `regex` crate to match `\{([A-Za-z0-9_.-]+)\}` and substitute from a `BTreeMap`
    - Error on missing keys
    - _Requirements: 4.1, 6.3_

  - [ ] 3.4 Implement `canonical_json` and `sha256_hex` in `lib.rs` (or a new `util` module)
    - `canonical_json`: recursively walk `serde_json::Value`, emit sorted keys, no whitespace, `,` and `:` separators
    - `sha256_hex`: SHA-256 of UTF-8 bytes, lowercase hex digest
    - _Requirements: 8.3, 8.4, 16.2_

  - [ ]* 3.5 Write property test for digest determinism (Property 8)
    - **Property 8: Plan digest determinism**
    - Generate random `serde_json::Value` trees, verify `canonical_json(parse(canonical_json(x))) == canonical_json(x)`
    - Verify `sha256_hex` produces consistent output for same input
    - **Validates: Requirements 8.3, 8.4, 16.2**

  - [ ] 3.6 Implement `utc_now` timestamp helper
    - Produce ISO 8601 `YYYY-MM-DDTHH:MM:SSZ` format (UTC, second precision, `Z` suffix)
    - _Requirements: 16.3_

  - [ ]* 3.7 Write property test for timestamp format (Property 15)
    - **Property 15: Timestamp format**
    - Call `utc_now()` multiple times, verify each matches regex `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$`
    - **Validates: Requirements 16.3**

- [ ] 4. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Implement git adapter
  - [ ] 5.1 Port `collect_git_changes` in `git.rs`
    - Change return type to `GitChanges { merge_base, changes: Vec<ChangedPath>, patch }`
    - Run `git merge-base <base> <head>` → `merge_base`
    - Run `git diff --name-status <merge_base> <head>` → parse into `Vec<ChangedPath>` with `owner: None`
    - Handle R/C statuses: take destination path, normalize status to single letter
    - Run `git diff --binary <merge_base> <head>` → `patch` string
    - Return descriptive errors including stderr on failure
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5_

  - [ ]* 5.2 Write property test for git name-status parsing (Property 16)
    - **Property 16: Git name-status parsing**
    - Generate random well-formed name-status lines (M/A/D/R100/C100 + tab-separated paths)
    - Verify parser extracts correct status and path, using destination for R/C
    - **Validates: Requirements 2.2**

- [ ] 6. Implement workspace discovery
  - [ ] 6.1 Port `WorkspaceGraph::discover` in `cargo_workspace.rs`
    - Invoke `cargo metadata --format-version 1 --no-deps --manifest-path <root>/Cargo.toml`
    - Parse JSON, extract workspace members with name, relative dir, manifest, dependencies
    - Filter dependencies to workspace-local packages only
    - Compute `reverse_deps` by inverting the dependency graph, sorted alphabetically
    - Add `reverse_deps: BTreeMap<String, Vec<String>>` field to `WorkspaceGraph`
    - _Requirements: 1.1, 1.2, 1.3, 1.4_

  - [ ] 6.2 Port `owner_for_path` in `cargo_workspace.rs`
    - Implement longest-prefix matching: normalize path and package dirs by stripping leading `./` and trailing `/`
    - Return the package with the longest matching prefix, or `None`
    - Change signature to accept `&str` path (not `&Utf8Path`) to match usage in obligations
    - _Requirements: 5.1, 5.2, 5.3_

  - [ ]* 6.3 Write property test for reverse dependency graph (Property 2)
    - **Property 2: Reverse dependency graph is the inverse of the dependency graph**
    - Generate random directed graphs of 1-20 named packages
    - Verify B in `reverse_deps[A]` iff A in `dependencies[B]`, and each list is sorted
    - **Validates: Requirements 1.3**

  - [ ]* 6.4 Write property test for owner resolution (Property 3)
    - **Property 3: Owner resolution selects longest prefix**
    - Generate random package directory trees and file paths
    - Verify `owner_for_path` returns the package with the longest matching prefix or `None`
    - **Validates: Requirements 5.1**

- [ ] 7. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 8. Implement obligation compiler
  - [ ] 8.1 Port `compile_obligations` in `obligations.rs`
    - Accept `&mut [ChangedPath]` to set `owner` on each path during processing
    - Accept `&WorkspaceGraph` for owner resolution
    - For each changed path: resolve owner, check each rule's patterns via `match_path`
    - For matching rules: expand emit templates with `{owner}`, record obligation with reason
    - Handle unowned paths with `{owner}` templates: record diagnostic, emit fallback if fail-closed
    - Add profile `always` obligations with source `"profile"`
    - Add empty-range fallback if no obligations and fail-closed
    - Return `(BTreeMap<String, Vec<ObligationReason>>, Vec<String>)` for obligations and diagnostics
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6_

  - [ ]* 8.2 Write property test for obligation compiler (Property 4)
    - **Property 4: Obligation compiler produces correct obligations with reasons**
    - Generate random configs with 1-5 rules, 1-10 changed paths, random owners
    - Verify rule-match obligations have source `"rule"`, profile obligations have source `"profile"`, and all reasons are correct
    - **Validates: Requirements 4.1, 4.4, 4.6**

- [ ] 9. Implement candidate expansion and solver
  - [ ] 9.1 Port `build_candidates` in `planner.rs`
    - Compute bindings: `[{}]` plus `[{pkg: name}]` for each distinct package from `pkg:*` obligations
    - For each surface template: determine if it uses `{pkg}`, select active bindings
    - Expand cover patterns, match against obligations using fnmatch-style globbing
    - Build `CandidateSurface` with expanded id (`template[pkg=name]` format), covers, run
    - Deduplicate by `(id, covers)`, keeping last
    - Sort by `(cost asc, -covers.len(), id asc)`
    - Filter out candidates with empty covers
    - Change `CandidateSurface.covers` from `BTreeSet<String>` to `Vec<String>` (sorted) to match reference
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8_

  - [ ] 9.2 Port `solve_exact_cover` in `planner.rs`
    - Build obligation→candidates index
    - Check all obligations are coverable, error if not
    - Branch-and-bound recursion: pick most-constrained obligation, try candidates sorted by (cost, -covers.len, id)
    - Prune when `(cost, count) >= best[:2]`
    - Tie-break by `(cost, count, sorted_ids)` tuple
    - Return selected surfaces sorted by id ascending
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7_

  - [ ]* 9.3 Write property test for candidate expansion (Property 5)
    - **Property 5: Candidate expansion produces correct bindings and covers**
    - Generate random surface templates and obligation sets
    - Verify pkg-bearing templates expand per distinct package, non-pkg templates expand once, ids match expected format, all placeholders substituted, covers match fnmatch
    - **Validates: Requirements 6.1, 6.2, 6.3, 6.4, 6.6**

  - [ ]* 9.4 Write property test for candidate list invariants (Property 6)
    - **Property 6: Candidate list invariants**
    - Verify every candidate has non-empty covers, no duplicate `(id, covers)` tuples, list sorted by `(cost asc, covers count desc, id asc)`
    - **Validates: Requirements 6.5, 6.7, 6.8**

  - [ ]* 9.5 Write property test for solver optimality (Property 7)
    - **Property 7: Solver finds minimum-cost complete cover**
    - Generate small random instances (3-8 obligations, 4-12 candidates)
    - Verify solution covers all obligations, has minimum cost, prefers fewer surfaces then lexicographic ids, sorted by id
    - **Validates: Requirements 7.1, 7.6, 7.7**

- [ ] 10. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 11. Implement `plan_repo` orchestrator in `lib.rs`
  - [ ] 11.1 Wire the full planning pipeline in `plan_repo`
    - Call `load_config` → `collect_git_changes` → write `diff.patch` → `WorkspaceGraph::discover` → `compile_obligations` → `build_candidates` → `solve_exact_cover`
    - Assemble `Plan` struct with all fields: version `"0.1.0-ref"`, `created_at`, `repo_root`, base/head/merge_base, profile, artifacts, workspace, changed_paths (sorted), obligations (sorted), selected_surfaces (sorted), omitted_surfaces (sorted), diagnostics
    - Compute `config_digest` = `sha256_hex(canonical_json(config_as_value))`
    - Compute `plan_digest` = `sha256_hex(canonical_json(plan_without_plan_digest))`
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6, 8.7, 8.8, 8.9_

  - [ ]* 11.2 Write property test for plan collection sorting (Property 9)
    - **Property 9: Plan collections are sorted**
    - Generate random Plan structs, verify `changed_paths` sorted by `(path, status)`, `obligations` sorted by id, `selected_surfaces` sorted by id, `omitted_surfaces` sorted by id
    - **Validates: Requirements 8.6, 16.1**

- [ ] 12. Implement emitters and artifact writing
  - [ ] 12.1 Port `emit_plan_markdown` in `explain.rs`
    - Match the reference `plan_markdown` format exactly: header, range, merge_base, profile, plan_digest, changed paths with status and owner, obligations with reasons, selected surfaces with cost and shell-joined run, conditional diagnostics section
    - _Requirements: 9.1, 9.2_

  - [ ] 12.2 Fix `emit_commands_shell` in `emit.rs`
    - Ensure output starts with `#!/usr/bin/env bash\nset -euo pipefail\n`, has N comment-command blocks, ends with single trailing newline (matching reference `emit_shell` exactly)
    - Verify `shell_join` matches Python `shlex.join` behavior
    - _Requirements: 10.1, 10.2, 10.3, 10.4_

  - [ ] 12.3 Fix `emit_github_actions` in `emit.rs`
    - Ensure output starts with `steps:\n  - name: Execute proof plan\n    run: |\n`, has N indented command lines, ends with single trailing newline
    - _Requirements: 11.1, 11.2, 11.3_

  - [ ] 12.4 Implement `write_plan_artifacts` in `emit.rs`
    - Create output directory, write `diff.patch`, `plan.json` (sorted keys, 2-space indent, trailing newline), `plan.md`, `commands.sh` (set executable), `github-actions.yml`
    - Serialize `plan.json` through `serde_json::Value` to get sorted keys
    - _Requirements: 8.1, 8.2, 8.5_

  - [ ]* 12.5 Write property test for markdown emitter (Property 10)
    - **Property 10: Markdown emitter contains all plan data**
    - Generate random Plan structs, verify output contains range, merge_base, profile, plan_digest, all changed paths, all obligations, all surfaces, diagnostics section iff non-empty
    - **Validates: Requirements 9.1**

  - [ ]* 12.6 Write property test for shell emitter (Property 11)
    - **Property 11: Shell emitter structure**
    - Generate random Plan structs with 1-5 surfaces, verify bash header, N comment-command blocks, trailing newline
    - **Validates: Requirements 10.1, 10.2, 10.3**

  - [ ]* 12.7 Write property test for GitHub Actions emitter (Property 12)
    - **Property 12: GitHub Actions emitter structure**
    - Generate random Plan structs with 1-5 surfaces, verify YAML structure, N command lines, trailing newline
    - **Validates: Requirements 11.1, 11.2**

- [ ] 13. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 14. Implement execution engine
  - [ ] 14.1 Port `execute_plan` in `run.rs`
    - Accept `repo_root` path, `Plan`, and `ExecutionMode`
    - Create `logs/` subdirectory in output dir
    - Dry-run: create empty log files, record each step with exit_code=0, duration_ms=0, status `"dry-run"`
    - Real execution: spawn subprocess per surface in `repo_root`, capture stdout/stderr to log files, measure elapsed ms, stop on first non-zero exit
    - Log file naming: `{index:02}-{surface_id}.stdout.log` / `.stderr.log` (1-based)
    - Write `receipt.json` with sorted keys, 2-space indent, trailing newline
    - Include `plan_digest` from input plan in receipt
    - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.6, 12.7, 12.8_

  - [ ]* 14.2 Write property test for dry-run receipt (Property 13)
    - **Property 13: Dry-run receipt invariants**
    - Generate random Plan structs, execute in dry-run mode, verify status `"dry-run"`, one step per surface, exit_code 0, duration_ms 0, correct log paths, plan_digest matches
    - **Validates: Requirements 12.1, 12.6, 12.7, 12.8**

- [ ] 15. Implement doctor command
  - [ ] 15.1 Port `doctor_repo` in `doctor.rs`
    - Update `DoctorReport` struct to match reference: `repo_root`, `config_path`, `cargo_manifest_path`, `package_count`, `packages: Vec<String>`, `issues: Vec<String>`
    - Check: missing `Cargo.toml`, missing `proofrun.toml` (with specific message), no packages, no profiles, no surfaces, no rules
    - Load config (using default fallback), scan workspace for package list
    - _Requirements: 13.1, 13.2, 13.3, 13.4, 13.5, 13.6, 13.7_

- [ ] 16. Complete CLI command surface
  - [ ] 16.1 Add `emit` and `run` subcommands to `cargo-proofrun/src/main.rs`
    - Add `Emit { emit_kind: EmitKind, plan }` with `EmitKind::Shell` and `EmitKind::GithubActions`
    - Add `Run { plan, dry_run }` subcommand
    - Wire `plan` subcommand to call `plan_repo` and print JSON to stdout
    - Wire `explain` to load plan and print `emit_plan_markdown` (not the old `render_explanation`)
    - Wire `emit shell` and `emit github-actions` to load plan and print output
    - Wire `run` to load plan and call `execute_plan`, print receipt JSON
    - Wire `doctor` to call `doctor_repo` and print JSON
    - _Requirements: 14.1, 14.2, 14.3, 14.4, 14.5, 14.6, 14.7_

- [ ] 17. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 18. Fixture conformance tests
  - [ ] 18.1 Write fixture-based conformance tests for `core-change` and `docs-change` scenarios
    - Create integration tests in `crates/proofrun/tests/` that run `plan_repo` against `fixtures/demo-workspace/repo` with commit ranges from `fixtures/demo-workspace/sample/commits.json`
    - Compare output plan's `obligations`, `selected_surfaces`, `omitted_surfaces`, `changed_paths`, and `workspace` fields against recorded `sample/*/plan.json`
    - Compare emitted `plan.md`, `commands.sh`, `github-actions.yml` against recorded samples
    - Run dry-run execution and compare receipt structure (status, step count, step ids, exit codes) against recorded `receipt.json`
    - **Property 14: Fixture parity with reference implementation**
    - **Validates: Requirements 15.1, 15.2, 15.3, 15.4**

- [ ] 19. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The Python reference at `reference/proofrun_ref.py` stays as the oracle during porting
- All maps and sets must use `BTree` variants for deterministic iteration order
- Version string must be `"0.1.0-ref"` during the porting phase to match the reference
