# Implementation Plan: Proofrun Differentiation Features

## Overview

Incremental implementation of six feature areas extending the proofrun planning library and CLI. Each task builds on previous tasks, with checkpoints between major feature areas. All code lives in `crates/proofrun/src/` (library) and `crates/cargo-proofrun/src/main.rs` (CLI). New module: `compare.rs`. Property-based tests use `proptest` with minimum 100 cases per property.

## Tasks

- [x] 1. Local Diff Planning — Change Source Abstraction (R1–R4)
  - [x] 1.1 Add `head_sha`, `collect_staged_changes`, and `collect_working_tree_changes` to `git.rs`
    - Implement `head_sha(repo_root) -> Result<String>` using `git rev-parse HEAD`
    - Implement `collect_staged_changes` using `git diff --name-status --cached` and `git diff --cached --binary`, setting `base = HEAD_SHA`, `head = "STAGED"`, `merge_base = HEAD_SHA`
    - Implement `collect_working_tree_changes` using `git diff --name-status HEAD` and `git diff HEAD --binary`, setting `base = HEAD_SHA`, `head = "WORKING_TREE"`, `merge_base = HEAD_SHA`
    - Both reuse existing `parse_name_status_line`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 2.4_

  - [x] 1.2 Add `ChangeSource` enum and `plan_from_source` to `lib.rs`
    - Define `ChangeSource` enum with variants: `GitRange(GitRange)`, `Staged`, `WorkingTree`, `PathsFromStdin(Vec<String>)`
    - Implement `plan_from_source(repo_root, source, profile) -> Result<Plan>` that resolves each variant into `(changes, patch, base, head, merge_base)` then feeds into the existing pipeline
    - For `PathsFromStdin`: skip blank lines, trim whitespace, assign status `"M"`, set `base/head/merge_base = "STDIN"`, `patch = ""`
    - Export new types and function from `lib.rs`
    - _Requirements: 1.5, 1.6, 2.5, 3.1, 3.2, 3.3, 3.4, 3.5, 3.6_

  - [x] 1.3 Write property test for change source field mapping (Property 1)
    - **Property 1: Change source field mapping**
    - Generate random `ChangeSource` variants with random SHA strings and path lists; verify resolved plan fields match expected sentinel values
    - **Validates: Requirements 1.2, 1.3, 2.2, 2.3, 3.3, 3.4**

  - [x] 1.4 Write property test for stdin path parsing (Property 2)
    - **Property 2: Stdin path parsing**
    - Generate random string lists with blanks, whitespace-only lines, and valid paths; verify blank lines skipped, paths trimmed, all status `"M"`
    - **Validates: Requirements 3.1, 3.2, 3.5**

  - [x] 1.5 Extend CLI `Plan` command with change source flags and mutual exclusivity validation
    - Add `--staged`, `--working-tree`, `--paths-from-stdin` flags to `Plan` command in `main.rs`
    - Make `--base`/`--head` optional (currently required)
    - Validate exactly one change source is provided; exit with descriptive error otherwise
    - Map flags to `ChangeSource` enum and call `plan_from_source`
    - _Requirements: 4.1, 4.2, 4.3_

  - [x] 1.6 Write property test for change source mutual exclusivity (Property 3)
    - **Property 3: Change source mutual exclusivity**
    - Generate all 16 combinations of 4 boolean flags; verify acceptance iff exactly one is set
    - **Validates: Requirements 4.1, 4.2, 4.3**

- [x] 2. Checkpoint — Local Diff Planning
  - Ensure all tests pass, ask the user if questions arise.

- [x] 3. Traceability / Explain Engine (R5–R8)
  - [x] 3.1 Add structured query types and `query_path` to `explain.rs`
    - Define `PathExplanation`, `RuleMatch`, `ObligationExplanation`, `SurfaceExplanation`, `TraceOutput`, `PathTrace`, `ProfileObligation`, `FallbackObligation` structs with `Serialize`
    - Implement `query_path(plan, path) -> PathExplanation`: look up path in `changed_paths`, find matching obligations via reasons, find covering surfaces; set `found = false` if path not in change set
    - _Requirements: 5.1, 5.2, 5.3, 5.4_

  - [x] 3.2 Write property test for path query correctness (Property 4)
    - **Property 4: Path query correctness**
    - Generate random Plans with 1–5 changed paths, 1–3 obligations, 1–3 surfaces; verify `found`, `obligations ⊆ plan.obligations`, `surfaces ⊆ plan.selected_surfaces`; verify `found = false` for absent paths
    - **Validates: Requirements 5.1, 5.3, 5.4**

  - [x] 3.3 Add `query_obligation` and `query_surface` to `explain.rs`
    - Implement `query_obligation(plan, id) -> Result<ObligationExplanation>`: return reasons, selected surfaces whose `covers` includes the id, omitted surfaces that would cover it; `Err` if id not found
    - Implement `query_surface(plan, id) -> Result<SurfaceExplanation>`: return selected surface details or omitted surface reason; `Err` if id in neither
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 7.1, 7.2, 7.3, 7.4_

  - [x] 3.4 Write property tests for obligation and surface query correctness (Properties 5, 6)
    - **Property 5: Obligation query correctness**
    - Generate random Plans, pick obligation ids; verify reasons match, selected/omitted surfaces correct; verify error for absent ids
    - **Validates: Requirements 6.1, 6.2, 6.3**
    - **Property 6: Surface query correctness**
    - Generate random Plans, pick surface ids from selected/omitted; verify status, fields, error for absent ids
    - **Validates: Requirements 7.1, 7.2, 7.3**

  - [x] 3.5 Add `trace_plan` to `explain.rs`
    - Implement `trace_plan(plan) -> TraceOutput`: build full traceability chain — each changed path with rule matches, obligations, surfaces; include profile obligations and fallback obligations
    - _Requirements: 8.1, 8.2, 8.3, 8.4_

  - [x] 3.6 Write property test for trace completeness (Property 7)
    - **Property 7: Trace completeness**
    - Generate random Plans with profile and fallback obligations; verify every changed path appears, every profile obligation appears, every fallback obligation appears, every selected surface appears in at least one trace entry
    - **Validates: Requirements 8.1, 8.2, 8.3**

  - [x] 3.7 Extend CLI with `explain --path/--obligation/--surface` and `trace` subcommands
    - Add `--path`, `--obligation`, `--surface` options to `Explain` command in `main.rs`
    - Add `Trace` subcommand to CLI
    - Wire to `query_path`, `query_obligation`, `query_surface`, `trace_plan`; output JSON to stdout
    - _Requirements: 5.4, 6.4, 7.4, 8.4, 27.1, 27.2, 27.3_

- [x] 4. Checkpoint — Traceability / Explain
  - Ensure all tests pass, ask the user if questions arise.

- [x] 5. CI Matrix Emission (R9–R11)
  - [x] 5.1 Add `emit_matrix_json`, `emit_structured_json`, `emit_nextest_filtersets` to `emit.rs`
    - Define `MatrixEntry` struct with `Serialize`
    - Implement `emit_matrix_json(plan) -> String`: build `Vec<MatrixEntry>` sorted by id, serialize with 2-space indent + trailing newline; empty plan → `"[]\n"`
    - Implement `emit_structured_json(plan) -> String`: build JSON object with `selected_surfaces`, `omitted_surfaces`, `obligations`, metadata; sorted keys, 2-space indent, trailing newline
    - Implement `emit_nextest_filtersets(plan) -> String`: iterate selected surfaces sorted by id, find `-E` arg, output `{id}\t{expression}\n`; omit surfaces without `-E`
    - Export new functions from `lib.rs`
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 10.1, 10.2, 10.3, 11.1, 11.2, 11.3, 11.4_

  - [x] 5.2 Write property tests for new emitters (Properties 8, 9, 10, 19)
    - **Property 8: Matrix emitter structure and content**
    - Generate Plans with 0–5 surfaces; verify JSON array length, field correctness, sort order, formatting
    - **Validates: Requirements 9.1, 9.2, 9.3, 9.4**
    - **Property 9: Structured JSON emitter content**
    - Generate Plans; verify JSON object contains all expected fields matching plan data
    - **Validates: Requirements 10.1, 10.2**
    - **Property 10: Nextest filterset emitter**
    - Generate Plans with surfaces that have/lack `-E` in run args; verify line format, sort order, omission of non-`-E` surfaces
    - **Validates: Requirements 11.1, 11.2, 11.3, 11.4**
    - **Property 19: New emitter output determinism**
    - Call each emitter twice with same input; verify identical output
    - **Validates: Requirements 25.1, 25.4**

  - [x] 5.3 Extend CLI `Emit` subcommand with `Matrix`, `Json`, `NextestFiltersets` variants
    - Add `Matrix`, `Json`, `NextestFiltersets` to `EmitKind` enum in `main.rs`
    - Wire to `emit_matrix_json`, `emit_structured_json`, `emit_nextest_filtersets`
    - _Requirements: 9.1, 10.1, 11.1_

- [x] 6. Checkpoint — CI Matrix Emission
  - Ensure all tests pass, ask the user if questions arise.

- [x] 7. Semantic Doctor Extensions (R12–R17)
  - [x] 7.1 Add `DoctorFinding` type and extend `DoctorReport` in `doctor.rs`
    - Define `DoctorFinding { severity: String, code: String, message: String }` with `Serialize`/`Deserialize`
    - Add `findings: Vec<DoctorFinding>` field to `DoctorReport`
    - Migrate existing string-based issues to also populate `findings` with severity `"warning"`
    - _Requirements: 12.2, 17.3_

  - [x] 7.2 Implement duplicate surface ID detection in `doctor.rs`
    - Iterate `config.surfaces`, collect ids in `BTreeMap<String, usize>`; any id with count > 1 → `DoctorFinding { severity: "error", code: "duplicate-surface-id", message }`
    - _Requirements: 12.1, 12.2_

  - [x] 7.3 Write property test for duplicate surface detection (Property 11)
    - **Property 11: Doctor duplicate surface detection**
    - Generate random Configs with 1–6 surface templates, some with duplicate ids; verify finding present iff duplicates exist
    - **Validates: Requirements 12.1, 12.2**

  - [x] 7.4 Implement uncovered obligation and unreachable rule detection in `doctor.rs`
    - Uncovered obligations (R13): expand rule emit patterns with all workspace package bindings, check if any surface template covers each; unmatched → finding with severity `"warning"`, code `"uncovered-obligation"`
    - Unreachable rules (R14): for rules with `crates/*/` prefix patterns, check if any workspace package directory matches; no match → finding with severity `"warning"`, code `"unreachable-rule"`
    - _Requirements: 13.1, 13.2, 13.3, 14.1, 14.2, 14.3_

  - [x] 7.5 Implement unbound placeholder detection in `doctor.rs`
    - Scan surface template id, covers, and run strings for `{placeholder}` patterns; known: `{pkg}`, `{profile}`, `{artifacts.diff_patch}`; any other → `DoctorFinding { severity: "error", code: "unbound-placeholder", message }`
    - _Requirements: 15.1, 15.2_

  - [x] 7.6 Write property test for unbound placeholder detection (Property 12)
    - **Property 12: Doctor unbound placeholder detection**
    - Generate surface templates with random placeholder strings; verify finding present iff unknown placeholder exists
    - **Validates: Requirements 15.1, 15.2**

  - [x] 7.7 Implement missing tool detection in `doctor.rs`
    - Check PATH for `git`, `cargo`, `cargo-nextest`, `cargo-mutants` using `which`-style lookup
    - Missing `git`/`cargo` → severity `"error"`; missing `cargo-nextest`/`cargo-mutants` → severity `"warning"`
    - _Requirements: 16.1, 16.2, 16.3_

  - [x] 7.8 Add `--strict` flag to CLI `Doctor` command
    - After report generation, if `--strict` and any finding has severity `"error"`, exit code 1
    - Always output full report JSON to stdout
    - _Requirements: 17.1, 17.2, 17.3_

  - [x] 7.9 Write property test for strict mode exit behavior (Property 13)
    - **Property 13: Doctor strict mode exit behavior**
    - Generate DoctorReports with random findings of varying severity; verify exit code 1 iff strict and any error-severity finding
    - **Validates: Requirements 17.1, 17.2, 17.3**

- [x] 8. Checkpoint — Semantic Doctor
  - Ensure all tests pass, ask the user if questions arise.

- [x] 9. Run Resume / Failed-Only (R18–R19)
  - [x] 9.1 Implement `execute_with_resume` in `run.rs`
    - Verify `previous_receipt.plan_digest == plan.plan_digest`; return `Err` on mismatch
    - For each surface in plan: if previous receipt has matching step with `exit_code == 0`, carry forward; otherwise execute fresh
    - Produce new Receipt with all steps merged; set status to `"passed"` (or `"dry-run"`) if all steps pass
    - _Requirements: 18.1, 18.2, 18.3, 18.4, 18.5, 18.6_

  - [x] 9.2 Implement `execute_failed_only` in `run.rs`
    - Verify `previous_receipt.plan_digest == plan.plan_digest`; return `Err` on mismatch
    - For each surface in plan: if previous receipt has matching step with `exit_code != 0`, execute fresh; if `exit_code == 0`, carry forward; if not found, skip
    - Produce new Receipt with all steps; set status based on results
    - _Requirements: 19.1, 19.2, 19.3, 19.4, 19.5, 19.6_

  - [x] 9.3 Write property tests for resume and failed-only (Properties 14, 15, 16)
    - **Property 14: Resume and failed-only digest verification**
    - Generate Plan/Receipt pairs with matching and mismatching digests; verify error on mismatch, success on match
    - **Validates: Requirements 18.3, 18.4, 19.2, 19.3**
    - **Property 15: Resume step merging**
    - Generate Plans with 1–5 surfaces and Receipts with mixed pass/fail steps in dry-run mode; verify passed steps carry forward, failed steps re-executed, total step count correct
    - **Validates: Requirements 18.1, 18.2, 18.5, 18.6**
    - **Property 16: Failed-only step selection**
    - Generate Plans and Receipts in dry-run mode; verify only failed steps re-executed, passed steps carry forward
    - **Validates: Requirements 19.1, 19.4, 19.5**

  - [x] 9.4 Extend CLI `Run` command with `--resume` and `--failed-only` flags
    - Add `--resume <receipt-path>` and `--failed-only` flags with `--receipt <receipt-path>` to `Run` command in `main.rs`
    - Load receipt, call `execute_with_resume` or `execute_failed_only`
    - Export new functions from `lib.rs`
    - _Requirements: 18.1, 19.1_

- [x] 10. Checkpoint — Run Resume / Failed-Only
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. Plan Comparison and Budget Gates (R20–R24)
  - [x] 11.1 Create `compare.rs` with `compare_plans` function
    - Create new module `crates/proofrun/src/compare.rs`
    - Define `PlanComparison` struct with `Serialize`/`Deserialize`
    - Implement `compare_plans(old, new) -> PlanComparison`: use `BTreeSet` for set operations on obligation ids and surface ids; compute cost delta; check for new fallback obligations
    - Register module in `lib.rs` and export
    - _Requirements: 20.1, 20.2, 20.3_

  - [x] 11.2 Write property test for plan comparison (Property 17)
    - **Property 17: Plan comparison correctness**
    - Generate pairs of Plans with overlapping/disjoint obligations and surfaces; verify added/removed lists, cost delta, fallback detection; verify self-comparison yields empty diffs and zero cost delta
    - **Validates: Requirements 20.1, 20.2, 20.3**

  - [x] 11.3 Add `Compare` subcommand to CLI
    - Add `Compare { old_plan, new_plan }` to `Command` enum in `main.rs`
    - Load both plans, call `compare_plans`, output JSON to stdout with 2-space indent, sorted keys, trailing newline
    - _Requirements: 20.1, 20.2_

  - [x] 11.4 Implement budget gate checks in CLI `Plan` command
    - Add `--max-cost`, `--max-surfaces`, `--fail-on-fallback`, `--warn-on-workspace-smoke-only` flags to `Plan` command
    - After plan assembly and JSON output: check gates in order; exit code 1 on gate failure; `--warn-on-workspace-smoke-only` prints to stderr, exit code 0
    - _Requirements: 21.1, 21.2, 21.3, 22.1, 22.2, 22.3, 23.1, 23.2, 23.3, 24.1, 24.2, 24.3_

  - [x] 11.5 Write property test for budget gates (Property 18)
    - **Property 18: Budget gate correctness**
    - Generate Plans with random costs/surface counts and random thresholds; verify each gate fires iff condition met
    - **Validates: Requirements 21.1, 21.2, 22.1, 22.2, 23.1, 23.2, 24.1, 24.2**

- [x] 12. Checkpoint — Plan Comparison and Budget Gates
  - Ensure all tests pass, ask the user if questions arise.

- [x] 13. Cross-Cutting Concerns (R25–R27)
  - [x] 13.1 Verify deterministic output and artifact shape preservation
    - Ensure all new output collections use `BTreeMap`/`BTreeSet` for deterministic ordering
    - Ensure all new emitters produce identical output for identical inputs
    - Verify existing fixture conformance tests still pass (`core-change`, `docs-change`)
    - Verify `plan.json` and `receipt.json` conform to existing schemas in `schema/`
    - _Requirements: 25.1, 25.2, 25.3, 25.4, 26.1, 26.2, 26.3_

  - [x] 13.2 Verify plan loading for explain and trace commands
    - Ensure `--plan` argument works for `explain` and `trace` subcommands with default `.proofrun/plan.json`
    - Ensure descriptive error on missing or unparseable plan file
    - _Requirements: 27.1, 27.2, 27.3_

- [x] 14. Final Checkpoint — All Features Complete
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation between feature areas
- Property tests validate universal correctness properties from the design document
- All new collections use `BTreeMap`/`BTreeSet` for deterministic ordering per project conventions
- No changes to `plan.json` or `receipt.json` artifact shape
- No new config schema semantics
