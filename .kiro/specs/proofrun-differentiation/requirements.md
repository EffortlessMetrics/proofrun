# Requirements Document

## Introduction

Six differentiation features for proofrun that build on the existing plan graph data (changed paths, owners, obligations, selected surfaces, reasons, receipts). These features close gaps with adjacent tools (Nx, Turborepo, Bazel, Launchable, cargo-nextest) and establish proofrun's whitespace: deterministic, explainable, repo-local proof planning with portable, reviewable, attestable proof artifacts.

All features compose with existing data structures and the existing planning core. No changes to plan or receipt artifact shape. No new config schema semantics. No network or service requirements. The planning core remains pure and deterministic.

## Glossary

- **Planner**: The `crates/proofrun` library crate that compiles repo policy plus a change set into a deterministic proof plan.
- **CLI**: The `crates/cargo-proofrun` binary crate that exposes the Planner as a `cargo proofrun` subcommand.
- **Config**: The `proofrun.toml` file checked into a repo root, defining profiles, surfaces, rules, and unknown-ownership policy.
- **Plan**: The selected set of Surfaces that covers all Obligations at minimum weighted cost, serialized as `plan.json`.
- **Receipt**: The execution result of a Plan, serialized as `receipt.json`.
- **ReceiptStep**: A single step within a Receipt, recording surface id, argv, exit code, duration, and log paths.
- **Obligation**: A proof requirement emitted by rule evaluation (e.g. `pkg:core:tests`).
- **ObligationRecord**: An Obligation with its associated list of reasons.
- **Surface**: A runnable proof step that can discharge one or more Obligations.
- **SelectedSurface**: A Surface chosen by the Solver for inclusion in the Plan.
- **OmittedSurface**: A candidate Surface not selected by the Solver, with a reason string.
- **CandidateSurface**: A Surface instance expanded from a Surface_Template with concrete bindings.
- **Obligation_Compiler**: The module that evaluates Config rules against changed paths to produce Obligations.
- **Solver**: The exact weighted set cover algorithm that selects the minimum-cost set of CandidateSurfaces covering all Obligations.
- **Emitter**: A module that renders Plan data into a specific output format.
- **Execution_Engine**: The module that runs or dry-runs a Plan and produces a Receipt.
- **Doctor**: The diagnostic command that checks repo readiness and policy correctness.
- **Change_Source**: The mechanism that provides changed paths to the Planner — either a Git range, staged changes, working tree changes, or an explicit path list.
- **Explain_Engine**: The module that provides deep traceability queries over Plan data, mapping paths to rules to obligations to surfaces.
- **Plan_Comparator**: The module that computes structural diffs between two Plans, reporting added/removed obligations, surfaces, cost deltas, and fallback changes.
- **Budget_Gate**: A threshold check applied to a Plan that fails the CLI with a non-zero exit code when a metric exceeds a configured limit.
- **Filterset_Expression**: A cargo-nextest `-E` filter expression string that selects tests matching a surface's coverage.
- **Matrix_Entry**: A JSON object representing a single selected surface suitable for use in a CI matrix strategy.
- **DoctorFinding**: A single diagnostic finding from the Doctor, with a severity level and descriptive message.
- **Fallback_Obligation**: An obligation emitted by the unknown-ownership or empty-range fallback policy.

## Requirements

### Requirement 1: Plan Against Staged Changes

**User Story:** As a developer, I want to plan against my staged (indexed) changes without committing, so that I can preview proof obligations at editor-time before creating a commit.

#### Acceptance Criteria

1. WHEN the `--staged` flag is provided to the `plan` subcommand, THE Planner SHALL collect changed paths by invoking `git diff --name-status --cached` against the repo working tree instead of requiring `--base` and `--head` arguments.
2. WHEN the `--staged` flag is provided, THE Planner SHALL set the Plan `base` field to the current HEAD commit SHA and the `head` field to the string `STAGED`.
3. WHEN the `--staged` flag is provided, THE Planner SHALL set the Plan `merge_base` field to the current HEAD commit SHA.
4. WHEN the `--staged` flag is provided, THE Planner SHALL capture the staged diff patch by invoking `git diff --cached --binary`.
5. WHEN no files are staged and the `--staged` flag is provided, THE Planner SHALL produce an empty change set and apply the same obligation derivation logic as a committed empty range (including empty-range fallback if fail-closed).
6. THE Planner SHALL feed staged changed paths into the same Obligation_Compiler, Solver, and Emitter pipeline used for committed Git ranges.

### Requirement 2: Plan Against Working Tree Changes

**User Story:** As a developer, I want to plan against all uncommitted working tree changes, so that I can see proof obligations for my entire in-progress work.

#### Acceptance Criteria

1. WHEN the `--working-tree` flag is provided to the `plan` subcommand, THE Planner SHALL collect changed paths by invoking `git diff --name-status HEAD` against the repo working tree instead of requiring `--base` and `--head` arguments.
2. WHEN the `--working-tree` flag is provided, THE Planner SHALL set the Plan `base` field to the current HEAD commit SHA and the `head` field to the string `WORKING_TREE`.
3. WHEN the `--working-tree` flag is provided, THE Planner SHALL set the Plan `merge_base` field to the current HEAD commit SHA.
4. WHEN the `--working-tree` flag is provided, THE Planner SHALL capture the working tree diff patch by invoking `git diff HEAD --binary`.
5. THE Planner SHALL feed working tree changed paths into the same Obligation_Compiler, Solver, and Emitter pipeline used for committed Git ranges.

### Requirement 3: Plan Against Explicit Path List

**User Story:** As a developer, I want to provide an explicit list of changed paths via stdin, so that I can integrate proofrun with external tools and custom change detection.

#### Acceptance Criteria

1. WHEN the `--paths-from-stdin` flag is provided to the `plan` subcommand, THE Planner SHALL read newline-delimited file paths from standard input and treat each path as a modified file.
2. WHEN the `--paths-from-stdin` flag is provided, THE Planner SHALL assign status `M` (modified) to each path read from stdin.
3. WHEN the `--paths-from-stdin` flag is provided, THE Planner SHALL set the Plan `base` field to the string `STDIN`, the `head` field to the string `STDIN`, and the `merge_base` field to the string `STDIN`.
4. WHEN the `--paths-from-stdin` flag is provided, THE Planner SHALL set the diff patch to an empty string since no Git diff is available.
5. THE Planner SHALL skip blank lines and strip leading and trailing whitespace from each path read from stdin.
6. THE Planner SHALL feed stdin-provided changed paths into the same Obligation_Compiler, Solver, and Emitter pipeline used for committed Git ranges.

### Requirement 4: Change Source Mutual Exclusivity

**User Story:** As a developer, I want the CLI to reject ambiguous change source combinations, so that the planning input is always unambiguous.

#### Acceptance Criteria

1. THE CLI SHALL accept exactly one change source: `--base`/`--head` (Git range), `--staged`, `--working-tree`, or `--paths-from-stdin`.
2. IF more than one change source is provided, THEN THE CLI SHALL exit with a non-zero exit code and a descriptive error message identifying the conflicting flags.
3. IF no change source is provided to the `plan` subcommand, THEN THE CLI SHALL exit with a non-zero exit code and a descriptive error message listing the available change source options.

### Requirement 5: Explain Path Traceability

**User Story:** As a developer, I want to query which rules fire for a given file path and what obligations and surfaces result, so that I can understand and debug proof policy behavior.

#### Acceptance Criteria

1. WHEN `explain --path <file>` is invoked with a Plan, THE Explain_Engine SHALL output which rules matched the path, which obligations were created from those rule matches, and which selected surfaces cover those obligations.
2. WHEN `explain --path <file>` is invoked and the path does not appear in the Plan changed paths, THE Explain_Engine SHALL output a message indicating the path was not part of the change set.
3. THE Explain_Engine SHALL include the rule index, matched glob pattern, and expanded obligation id for each rule match.
4. THE Explain_Engine SHALL output results as structured JSON to stdout.

### Requirement 6: Explain Obligation Traceability

**User Story:** As a developer, I want to query which paths and rules created a given obligation and which surfaces cover it, so that I can trace proof requirements back to their source.

#### Acceptance Criteria

1. WHEN `explain --obligation <id>` is invoked with a Plan, THE Explain_Engine SHALL output all reasons for the obligation including source, path, rule index, and matched pattern.
2. WHEN `explain --obligation <id>` is invoked with a Plan, THE Explain_Engine SHALL output which selected surfaces cover the obligation and which omitted surfaces would have covered the obligation.
3. WHEN `explain --obligation <id>` is invoked and the obligation id does not exist in the Plan, THE Explain_Engine SHALL exit with a non-zero exit code and a descriptive error message.
4. THE Explain_Engine SHALL output results as structured JSON to stdout.

### Requirement 7: Explain Surface Traceability

**User Story:** As a developer, I want to query what a given surface covers, why it was selected or omitted, and its cost, so that I can understand proof plan selection decisions.

#### Acceptance Criteria

1. WHEN `explain --surface <id>` is invoked with a Plan and the surface is selected, THE Explain_Engine SHALL output the surface id, template, cost, covered obligations, and run command.
2. WHEN `explain --surface <id>` is invoked with a Plan and the surface is omitted, THE Explain_Engine SHALL output the surface id and the omission reason.
3. WHEN `explain --surface <id>` is invoked and the surface id does not exist in either selected or omitted surfaces, THE Explain_Engine SHALL exit with a non-zero exit code and a descriptive error message.
4. THE Explain_Engine SHALL output results as structured JSON to stdout.

### Requirement 8: Trace Combined View

**User Story:** As a developer, I want a single command that shows the full traceability chain from changed paths through rules, obligations, and surfaces, so that I can review the entire proof derivation at once.

#### Acceptance Criteria

1. WHEN `trace` is invoked with a Plan, THE Explain_Engine SHALL output a combined view showing each changed path, the rules that matched the path, the obligations derived from those matches, and the surfaces selected to cover those obligations.
2. THE Explain_Engine SHALL include profile-sourced obligations (from `always` lists) in the trace output with source `profile`.
3. THE Explain_Engine SHALL include fallback obligations in the trace output with their respective source (`unknown-fallback` or `empty-range-fallback`).
4. THE Explain_Engine SHALL output results as structured JSON to stdout.

### Requirement 9: Emit CI Matrix JSON

**User Story:** As a CI engineer, I want to emit selected surfaces as a JSON array suitable for GitHub Actions matrix strategy, so that proofrun can orchestrate parallel CI jobs.

#### Acceptance Criteria

1. WHEN `emit matrix` is invoked with a Plan, THE Emitter SHALL output a JSON array where each element is an object containing the surface `id`, `template`, `cost`, `covers` list, and `run` command as a shell-escaped string.
2. THE Emitter SHALL sort the matrix entries by surface id ascending.
3. THE Emitter SHALL output the JSON array to stdout with 2-space indentation and a trailing newline.
4. WHEN the Plan has no selected surfaces, THE Emitter SHALL output an empty JSON array `[]`.

### Requirement 10: Emit Structured JSON

**User Story:** As a tool integrator, I want to emit the full selected surfaces data as structured JSON, so that downstream tools can consume proof plan data programmatically.

#### Acceptance Criteria

1. WHEN `emit json` is invoked with a Plan, THE Emitter SHALL output a JSON object containing the `selected_surfaces` array, `omitted_surfaces` array, `obligations` array, and plan metadata (`base`, `head`, `merge_base`, `profile`, `plan_digest`).
2. THE Emitter SHALL serialize the JSON with 2-space indentation, sorted keys, and a trailing newline.
3. THE Emitter SHALL output the JSON to stdout.

### Requirement 11: Emit Nextest Filterset Expressions

**User Story:** As a developer, I want to emit nextest -E filterset expressions for selected surfaces, so that I can run targeted test subsets directly with cargo-nextest.

#### Acceptance Criteria

1. WHEN `emit nextest-filtersets` is invoked with a Plan, THE Emitter SHALL output one line per selected surface whose run command contains a `-E` argument, printing the surface id followed by a tab character followed by the filterset expression.
2. WHEN a selected surface run command does not contain a `-E` argument, THE Emitter SHALL omit that surface from the output.
3. THE Emitter SHALL sort output lines by surface id ascending.
4. THE Emitter SHALL terminate the output with a trailing newline.

### Requirement 12: Doctor Duplicate Surface IDs

**User Story:** As a developer, I want the doctor to detect duplicate surface ids in my config, so that I can fix ambiguous surface definitions.

#### Acceptance Criteria

1. WHEN two or more surface templates in the Config share the same `id` value, THE Doctor SHALL include a finding identifying each duplicated surface id.
2. THE Doctor SHALL include the finding with severity `error`.

### Requirement 13: Doctor Uncovered Obligations

**User Story:** As a developer, I want the doctor to detect obligations that no surface template can cover, so that I can fix gaps in my proof policy before planning.

#### Acceptance Criteria

1. WHEN the Doctor evaluates the Config, THE Doctor SHALL expand all surface templates with all possible package bindings derived from rule emit patterns and check whether each possible obligation pattern has at least one covering surface template.
2. WHEN an obligation pattern from a rule emit template has no covering surface template, THE Doctor SHALL include a finding identifying the uncoverable obligation pattern.
3. THE Doctor SHALL include the finding with severity `warning`.

### Requirement 14: Doctor Unreachable Rules

**User Story:** As a developer, I want the doctor to detect rules whose path patterns can never match any file in the workspace, so that I can remove dead policy.

#### Acceptance Criteria

1. WHEN the Doctor evaluates the Config with access to the Workspace_Graph, THE Doctor SHALL check each rule's path patterns against the set of known workspace package directories.
2. WHEN a rule's path patterns use `crates/*/` prefix patterns and no workspace package directory matches the prefix, THE Doctor SHALL include a finding identifying the unreachable rule and pattern.
3. THE Doctor SHALL include the finding with severity `warning`.

### Requirement 15: Doctor Unbound Template Placeholders

**User Story:** As a developer, I want the doctor to detect surface templates with placeholders that can never be bound, so that I can fix broken template definitions.

#### Acceptance Criteria

1. WHEN a surface template contains a placeholder other than `{pkg}`, `{profile}`, and `{artifacts.diff_patch}`, THE Doctor SHALL include a finding identifying the unbound placeholder and the surface template id.
2. THE Doctor SHALL include the finding with severity `error`.

### Requirement 16: Doctor Missing Local Tools

**User Story:** As a developer, I want the doctor to check that required local tools are installed, so that I can fix my environment before running proof plans.

#### Acceptance Criteria

1. THE Doctor SHALL check for the presence of `git`, `cargo`, `cargo-nextest`, and `cargo-mutants` in the system PATH.
2. WHEN a required tool is not found in the system PATH, THE Doctor SHALL include a finding identifying the missing tool.
3. THE Doctor SHALL include the finding with severity `warning` for `cargo-nextest` and `cargo-mutants`, and severity `error` for `git` and `cargo`.

### Requirement 17: Doctor Strict Mode

**User Story:** As a CI engineer, I want a strict doctor mode that exits with a non-zero exit code on any finding, so that I can gate CI pipelines on policy health.

#### Acceptance Criteria

1. WHEN the `--strict` flag is provided to the `doctor` subcommand, THE Doctor SHALL exit with exit code 1 if any finding with severity `error` is present.
2. WHEN the `--strict` flag is not provided, THE Doctor SHALL exit with exit code 0 regardless of findings.
3. THE Doctor SHALL output the full DoctorReport to stdout as JSON regardless of the `--strict` flag.

### Requirement 18: Run Resume from Receipt

**User Story:** As a developer, I want to resume a failed proof plan from a previous receipt, skipping steps that already passed, so that I avoid re-running expensive proof steps.

#### Acceptance Criteria

1. WHEN `run --resume <receipt-path>` is invoked with a Plan, THE Execution_Engine SHALL load the referenced Receipt and skip all steps whose ReceiptStep has exit_code 0 in the previous Receipt.
2. WHEN `run --resume <receipt-path>` is invoked, THE Execution_Engine SHALL execute remaining steps (those not present in the previous Receipt or those with non-zero exit codes) in the order defined by the Plan.
3. WHEN `run --resume <receipt-path>` is invoked, THE Execution_Engine SHALL verify that the Receipt `plan_digest` matches the current Plan `plan_digest` before resuming.
4. IF the Receipt `plan_digest` does not match the Plan `plan_digest`, THEN THE Execution_Engine SHALL exit with a non-zero exit code and a descriptive error message.
5. THE Execution_Engine SHALL produce a new Receipt containing all steps: previously passed steps with their original exit codes and durations, and newly executed steps with fresh results.
6. WHEN all previously failed or missing steps succeed during resume, THE Execution_Engine SHALL set the new Receipt status to `passed`.

### Requirement 19: Run Failed-Only from Receipt

**User Story:** As a developer, I want to rerun only the failed steps from a previous receipt, so that I can quickly iterate on fixing specific proof failures.

#### Acceptance Criteria

1. WHEN `run --failed-only --receipt <receipt-path>` is invoked with a Plan, THE Execution_Engine SHALL load the referenced Receipt and execute only steps whose ReceiptStep has a non-zero exit_code.
2. WHEN `run --failed-only --receipt <receipt-path>` is invoked, THE Execution_Engine SHALL verify that the Receipt `plan_digest` matches the current Plan `plan_digest` before executing.
3. IF the Receipt `plan_digest` does not match the Plan `plan_digest`, THEN THE Execution_Engine SHALL exit with a non-zero exit code and a descriptive error message.
4. THE Execution_Engine SHALL produce a new Receipt containing all steps: previously passed steps with their original results, and re-executed failed steps with fresh results.
5. WHEN all re-executed steps succeed, THE Execution_Engine SHALL set the new Receipt status to `passed`.
6. WHEN any re-executed step fails, THE Execution_Engine SHALL set the new Receipt status to `failed` and stop executing further steps.

### Requirement 20: Compare Plans

**User Story:** As a reviewer, I want to compare two plans and see what changed in obligations, surfaces, cost, and fallbacks, so that I can review proof selection changes in pull requests.

#### Acceptance Criteria

1. WHEN `compare <old-plan.json> <new-plan.json>` is invoked, THE Plan_Comparator SHALL compute and output: obligations added, obligations removed, surfaces added, surfaces removed, total cost delta, and whether any new Fallback_Obligations were introduced.
2. THE Plan_Comparator SHALL output the comparison as structured JSON to stdout with 2-space indentation, sorted keys, and a trailing newline.
3. WHEN the two plans are identical in obligations and surfaces, THE Plan_Comparator SHALL output a comparison with empty added/removed lists and zero cost delta.

### Requirement 21: Budget Gate Max Cost

**User Story:** As a CI engineer, I want to fail the build if the total plan cost exceeds a threshold, so that I can prevent proof plan bloat.

#### Acceptance Criteria

1. WHEN `--max-cost <threshold>` is provided to the `plan` subcommand, THE Planner SHALL compute the total cost of all selected surfaces and exit with exit code 1 if the total cost exceeds the threshold.
2. WHEN the total cost does not exceed the threshold, THE Planner SHALL proceed with normal plan output.
3. THE Planner SHALL output the plan JSON to stdout before exiting, regardless of whether the budget gate fails.

### Requirement 22: Budget Gate Max Surfaces

**User Story:** As a CI engineer, I want to fail the build if the number of selected surfaces exceeds a threshold, so that I can prevent proof plan fragmentation.

#### Acceptance Criteria

1. WHEN `--max-surfaces <threshold>` is provided to the `plan` subcommand, THE Planner SHALL count the selected surfaces and exit with exit code 1 if the count exceeds the threshold.
2. WHEN the surface count does not exceed the threshold, THE Planner SHALL proceed with normal plan output.
3. THE Planner SHALL output the plan JSON to stdout before exiting, regardless of whether the budget gate fails.

### Requirement 23: Budget Gate Fail on Fallback

**User Story:** As a CI engineer, I want to fail the build if any fallback obligation was triggered, so that I can enforce complete ownership coverage.

#### Acceptance Criteria

1. WHEN `--fail-on-fallback` is provided to the `plan` subcommand, THE Planner SHALL inspect the Plan obligations for any reason with source `unknown-fallback` or `empty-range-fallback` and exit with exit code 1 if any such reason exists.
2. WHEN no fallback obligations exist, THE Planner SHALL proceed with normal plan output.
3. THE Planner SHALL output the plan JSON to stdout before exiting, regardless of whether the budget gate fails.

### Requirement 24: Budget Gate Warn on Workspace Smoke Only

**User Story:** As a CI engineer, I want a warning when the only selected surface is the safety-net workspace smoke test, so that I can detect under-specified proof policy.

#### Acceptance Criteria

1. WHEN `--warn-on-workspace-smoke-only` is provided to the `plan` subcommand and the only selected surface has template `workspace.smoke`, THE Planner SHALL output a warning message to stderr.
2. WHEN `--warn-on-workspace-smoke-only` is provided and more than one surface is selected or the single surface is not `workspace.smoke`, THE Planner SHALL produce no warning.
3. THE Planner SHALL exit with exit code 0 regardless of whether the warning is emitted (this is a warning, not a gate).

### Requirement 25: Deterministic Output Preservation

**User Story:** As a developer, I want all new features to preserve the existing deterministic output guarantees, so that plans remain reproducible and auditable.

#### Acceptance Criteria

1. THE Planner SHALL sort all new output collections using stable, lexicographic ordering consistent with existing Plan sorting conventions.
2. THE Planner SHALL use the same canonical JSON serialization for any new digest computations.
3. THE Planner SHALL use UTC timestamps with second precision in ISO 8601 format ending with `Z` for any new timestamp fields.
4. FOR ALL new emitter outputs, THE Emitter SHALL produce identical output for identical inputs regardless of execution environment.

### Requirement 26: Artifact Shape Preservation

**User Story:** As a developer, I want the existing plan.json and receipt.json artifact shapes to remain unchanged, so that existing consumers and fixtures are not broken.

#### Acceptance Criteria

1. THE Planner SHALL produce `plan.json` artifacts conforming to the existing `schema/plan.schema.json` schema without adding or removing required fields.
2. THE Execution_Engine SHALL produce `receipt.json` artifacts conforming to the existing `schema/receipt.schema.json` schema without adding or removing required fields.
3. THE Planner SHALL produce identical `plan.json` output for the existing fixture scenarios (`core-change`, `docs-change`) when invoked with the same Git range inputs.

### Requirement 27: Explain and Trace Plan Loading

**User Story:** As a developer, I want the explain and trace commands to operate on a previously generated plan.json file, so that I can query plans without re-running the planner.

#### Acceptance Criteria

1. THE CLI SHALL accept a `--plan` argument (default `.proofrun/plan.json`) for the `explain` subcommand variants (`--path`, `--obligation`, `--surface`) and the `trace` subcommand.
2. THE CLI SHALL load and deserialize the Plan from the specified JSON file before executing the query.
3. IF the plan file does not exist or cannot be parsed, THEN THE CLI SHALL exit with a non-zero exit code and a descriptive error message.
