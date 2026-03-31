# Requirements Document

## Introduction

Port the existing Python reference implementation (`reference/proofrun_ref.py`) into a fully native Rust implementation across the existing crate scaffold in `crates/proofrun` and `crates/cargo-proofrun`. The Rust implementation must produce byte-identical plan and receipt artifacts for the same inputs, preserving the deterministic, fail-closed planning semantics. The Python reference stays as the oracle during porting.

## Glossary

- **Planner**: The `crates/proofrun` library crate that compiles repo policy plus a Git change range into a deterministic proof plan.
- **CLI**: The `crates/cargo-proofrun` binary crate that exposes the Planner as a `cargo proofrun` subcommand.
- **Config**: The `proofrun.toml` file checked into a repo root, defining profiles, surfaces, rules, and unknown-ownership policy.
- **Signal**: A fact derived from the Git diff (changed path, status, owning package).
- **Obligation**: A proof requirement emitted by rule evaluation (e.g. `pkg:core:tests`).
- **Surface**: A runnable proof step that can discharge one or more Obligations (e.g. `tests.pkg[pkg=core]`).
- **Candidate_Surface**: A Surface instance expanded from a Surface_Template with concrete bindings.
- **Surface_Template**: A parameterized surface definition in Config containing `{pkg}` and `{profile}` placeholders.
- **Plan**: The selected set of Surfaces that covers all Obligations at minimum weighted cost, serialized as `plan.json`.
- **Receipt**: The execution result of a Plan, serialized as `receipt.json`.
- **Workspace_Graph**: The set of Cargo packages, their directories, manifests, dependencies, and reverse dependencies discovered from `cargo metadata`.
- **Obligation_Compiler**: The module that evaluates Config rules against changed paths to produce Obligations.
- **Solver**: The exact weighted set cover algorithm (branch-and-bound) that selects the minimum-cost set of Candidate_Surfaces covering all Obligations.
- **Emitter**: A module that renders Plan data into a specific output format (Markdown, shell script, GitHub Actions YAML).
- **Execution_Engine**: The module that runs or dry-runs a Plan and produces a Receipt.
- **Doctor**: The diagnostic command that checks repo readiness.
- **Reference_Implementation**: The Python script at `reference/proofrun_ref.py` that defines authoritative behavior.
- **Merge_Base**: The Git merge-base commit between the `--base` and `--head` revisions, used as the actual diff anchor.
- **Glob_Pattern**: A path-matching pattern using `*`, `**`, and `?` wildcards as defined in Config rules.
- **Owner_Resolution**: The process of mapping a changed file path to its owning Cargo package by longest directory prefix match.
- **Fail_Closed**: The default unknown-ownership mode where unowned paths that match `{owner}`-bearing rules trigger fallback obligations rather than being silently skipped.

## Requirements

### Requirement 1: Workspace Discovery via cargo metadata

**User Story:** As a developer, I want the Planner to discover Cargo workspace packages using `cargo metadata`, so that package ownership, dependencies, and reverse dependencies are resolved accurately.

#### Acceptance Criteria

1. WHEN `cargo metadata --format-version 1` is invoked for a repo root, THE Workspace_Graph SHALL populate a list of packages with name, relative directory, manifest path, and workspace dependency names.
2. WHEN a package declares path-based dependencies, THE Workspace_Graph SHALL resolve the dependency package name from the target manifest rather than using the dependency key name.
3. THE Workspace_Graph SHALL compute reverse dependencies such that for each package, the list of packages that depend on the package is available and sorted alphabetically.
4. WHEN `cargo metadata` fails or returns no workspace packages, THE Workspace_Graph SHALL return a descriptive error.

### Requirement 2: Git Adapter with Merge-Base Resolution

**User Story:** As a developer, I want the Planner to resolve the merge-base between two revisions and collect changed paths from that merge-base, so that the diff accurately reflects the logical change set.

#### Acceptance Criteria

1. WHEN a base and head revision are provided, THE Planner SHALL resolve the merge-base via `git merge-base <base> <head>` and use the merge-base as the diff anchor.
2. WHEN collecting changed paths, THE Planner SHALL invoke `git diff --name-status <merge_base> <head>` and parse each line into a path and status.
3. WHEN a changed path has a rename status (R) or copy status (C), THE Planner SHALL use the destination path and normalize the status to the single letter `R` or `C`.
4. WHEN collecting the binary patch, THE Planner SHALL invoke `git diff --binary <merge_base> <head>` and capture the full output for the `diff.patch` artifact.
5. IF `git merge-base` or `git diff` fails, THEN THE Planner SHALL return a descriptive error including the stderr output from Git.

### Requirement 3: Glob-to-Regex Path Matching

**User Story:** As a developer, I want Config rule path patterns to be matched against changed paths using glob semantics, so that rules fire correctly for the intended file sets.

#### Acceptance Criteria

1. THE Planner SHALL convert glob patterns to regular expressions where `**` matches zero or more path segments (including separators), `*` matches zero or more characters within a single path segment (excluding `/`), and `?` matches exactly one non-separator character.
2. WHEN a pattern contains `**/` at any position, THE Planner SHALL match zero or more directory prefixes including the trailing separator.
3. WHEN a pattern contains a trailing `**`, THE Planner SHALL match any remaining path suffix.
4. THE Planner SHALL strip leading `/` from both the pattern and the path before matching.
5. FOR ALL valid path and pattern combinations, THE Planner SHALL produce match results identical to the Reference_Implementation `match_path` function.

### Requirement 4: Obligation Compilation

**User Story:** As a developer, I want the Obligation_Compiler to evaluate Config rules against changed paths and produce the correct set of Obligations, so that every changed file triggers the appropriate proof requirements.

#### Acceptance Criteria

1. WHEN a changed path matches a rule pattern, THE Obligation_Compiler SHALL expand each `emit` template by substituting `{owner}` with the owning package name from Owner_Resolution.
2. WHEN a changed path matches a rule pattern containing `{owner}` in its emit templates and the path has no owning package, THE Obligation_Compiler SHALL record a diagnostic message identifying the unowned path and rule index.
3. WHILE the Config unknown mode is `fail-closed` and a path is unowned, THE Obligation_Compiler SHALL emit each fallback obligation from the Config unknown fallback list with source `unknown-fallback`.
4. WHEN a profile defines `always` obligations, THE Obligation_Compiler SHALL include each always obligation with source `profile` regardless of changed paths.
5. IF no obligations are derived from any rule or profile and the Config unknown mode is `fail-closed`, THEN THE Obligation_Compiler SHALL emit each fallback obligation with source `empty-range-fallback`.
6. THE Obligation_Compiler SHALL associate each obligation with a list of reasons recording the source, path, rule index, and matched pattern.

### Requirement 5: Owner Resolution by Longest Prefix

**User Story:** As a developer, I want changed paths to be assigned to the Cargo package whose directory is the longest prefix match, so that ownership is unambiguous and deterministic.

#### Acceptance Criteria

1. WHEN multiple packages have directories that are prefixes of a changed path, THE Planner SHALL assign ownership to the package with the longest directory prefix.
2. WHEN no package directory is a prefix of a changed path, THE Planner SHALL assign ownership as `null` (no owner).
3. THE Planner SHALL normalize both the package directory and the changed path by stripping leading `./` and trailing `/` before comparison.

### Requirement 6: Candidate Surface Expansion

**User Story:** As a developer, I want Surface_Templates to be expanded into Candidate_Surfaces with concrete package bindings, so that the Solver has a complete set of options to choose from.

#### Acceptance Criteria

1. WHEN a Surface_Template contains `{pkg}` placeholders, THE Planner SHALL expand the template once per distinct package name extracted from `pkg:*` obligations.
2. WHEN a Surface_Template does not contain `{pkg}` placeholders, THE Planner SHALL expand the template exactly once with no package binding.
3. THE Planner SHALL substitute `{profile}` with the active profile name and `{artifacts.diff_patch}` with the relative path to the diff patch file in all `run` arguments.
4. THE Planner SHALL compute each Candidate_Surface covers list by matching expanded cover patterns against the obligation list using `fnmatch`-style glob matching.
5. WHEN a Candidate_Surface covers no obligations, THE Planner SHALL exclude the Candidate_Surface from the candidate list.
6. THE Planner SHALL assign each Candidate_Surface an id of the form `template_id[key=value,...]` when package bindings are present, or `template_id` when no bindings are present.
7. THE Planner SHALL deduplicate Candidate_Surfaces by (id, covers) tuple, keeping the last occurrence.
8. THE Planner SHALL sort the final candidate list by (cost ascending, covers count descending, id ascending).

### Requirement 7: Exact Weighted Set Cover Solver

**User Story:** As a developer, I want the Solver to find the minimum-cost set of Candidate_Surfaces that covers all Obligations exactly, so that the proof plan is optimal and deterministic.

#### Acceptance Criteria

1. THE Solver SHALL find the set of Candidate_Surfaces with minimum total cost that covers every obligation.
2. WHEN multiple solutions have equal total cost, THE Solver SHALL prefer the solution with fewer surfaces, then break ties by lexicographic sort of surface ids.
3. THE Solver SHALL use branch-and-bound pruning: if the current partial cost and count already meet or exceed the best known solution, the branch SHALL be pruned.
4. WHEN selecting the next obligation to branch on, THE Solver SHALL choose the obligation with the fewest covering candidates (most constrained first), breaking ties alphabetically.
5. IF any obligation has zero covering candidates, THEN THE Solver SHALL return an error identifying the uncoverable obligation.
6. THE Solver SHALL return the selected surfaces sorted by id ascending.
7. FOR ALL inputs, THE Solver SHALL produce results identical to the Reference_Implementation `solve_exact_cover` function.

### Requirement 8: Plan Assembly and Artifact Emission

**User Story:** As a developer, I want the Planner to assemble a complete Plan and write all artifacts to the output directory, so that the plan is portable and reviewable.

#### Acceptance Criteria

1. WHEN a plan is compiled, THE Planner SHALL write `diff.patch`, `plan.json`, `plan.md`, `commands.sh`, and `github-actions.yml` to the configured output directory.
2. THE Planner SHALL create the output directory if the output directory does not exist.
3. THE Planner SHALL compute `config_digest` as the SHA-256 hex digest of the canonical JSON serialization of the config (sorted keys, no whitespace separators).
4. THE Planner SHALL compute `plan_digest` as the SHA-256 hex digest of the canonical JSON serialization of the plan excluding the `plan_digest` field itself.
5. THE Planner SHALL serialize `plan.json` with 2-space indentation and sorted keys, terminated by a newline.
6. THE Planner SHALL include `changed_paths` sorted by (path, status), `obligations` sorted by id, and `selected_surfaces` sorted by id in the plan.
7. THE Planner SHALL include `omitted_surfaces` listing each candidate not selected, sorted by id, with reason `not selected by optimal weighted cover`.
8. THE Planner SHALL include a `workspace` field containing the sorted list of packages with name, dir, manifest, dependencies, and reverse_dependencies.
9. THE Planner SHALL include a `diagnostics` array containing any diagnostic messages from obligation compilation.

### Requirement 9: Plan Markdown Emitter

**User Story:** As a developer, I want the plan rendered as a Markdown document, so that the plan is human-readable and reviewable in pull requests.

#### Acceptance Criteria

1. THE Emitter SHALL produce Markdown output matching the Reference_Implementation `plan_markdown` function format, including sections for range metadata, changed paths, obligations with reasons, selected surfaces with cost and run commands, and diagnostics.
2. WHEN the plan has no diagnostics, THE Emitter SHALL omit the Diagnostics section.

### Requirement 10: Shell Script Emitter

**User Story:** As a developer, I want the plan emitted as a shell script, so that the proof steps can be executed directly in a terminal.

#### Acceptance Criteria

1. THE Emitter SHALL produce a bash script starting with `#!/usr/bin/env bash` and `set -euo pipefail`.
2. THE Emitter SHALL emit each selected surface as a comment with the surface id followed by the shell-escaped command on the next line, separated by blank lines.
3. THE Emitter SHALL use `shlex`-compatible shell escaping for command arguments.
4. THE Emitter SHALL terminate the script with a single trailing newline.

### Requirement 11: GitHub Actions Emitter

**User Story:** As a developer, I want the plan emitted as a GitHub Actions step, so that the proof steps can be integrated into CI workflows.

#### Acceptance Criteria

1. THE Emitter SHALL produce YAML output with a `steps:` array containing a single step named `Execute proof plan` with a multi-line `run` block.
2. THE Emitter SHALL list each selected surface command as a shell-escaped line within the `run` block.
3. THE Emitter SHALL terminate the output with a single trailing newline.

### Requirement 12: Execution Engine

**User Story:** As a developer, I want to execute or dry-run a Plan and produce a Receipt, so that proof results are recorded and auditable.

#### Acceptance Criteria

1. WHEN executing in dry-run mode, THE Execution_Engine SHALL create empty stdout and stderr log files for each step, record exit code 0 and duration 0ms, and set receipt status to `dry-run`.
2. WHEN executing in real mode, THE Execution_Engine SHALL run each surface command as a subprocess in the repo root, capture stdout and stderr to log files, and record the exit code and elapsed duration in milliseconds.
3. WHEN a step exits with a non-zero exit code during real execution, THE Execution_Engine SHALL set receipt status to `failed` and stop executing further steps.
4. WHEN all steps succeed during real execution, THE Execution_Engine SHALL set receipt status to `passed`.
5. THE Execution_Engine SHALL create a `logs/` subdirectory within the output directory for log files.
6. THE Execution_Engine SHALL name log files as `{index:02d}-{surface_id}.stdout.log` and `{index:02d}-{surface_id}.stderr.log` with 1-based indexing.
7. THE Execution_Engine SHALL write `receipt.json` to the output directory with 2-space indentation, sorted keys, and a trailing newline.
8. THE Execution_Engine SHALL include the `plan_digest` from the executed plan in the receipt.

### Requirement 13: Doctor Command

**User Story:** As a developer, I want a doctor command that checks repo readiness, so that I can diagnose configuration and workspace issues before planning.

#### Acceptance Criteria

1. THE Doctor SHALL report the repo root path, config file path, Cargo manifest path, package count, sorted package name list, and a list of issues.
2. WHEN `proofrun.toml` is missing, THE Doctor SHALL include `proofrun.toml missing; using built-in default config` in the issues list.
3. WHEN `Cargo.toml` is missing, THE Doctor SHALL include `missing Cargo.toml` in the issues list.
4. WHEN no Cargo packages are discovered, THE Doctor SHALL include `no Cargo packages discovered` in the issues list.
5. WHEN no profiles are configured, THE Doctor SHALL include `no profiles configured` in the issues list.
6. WHEN no surfaces are configured, THE Doctor SHALL include `no surfaces configured` in the issues list.
7. WHEN no rules are configured, THE Doctor SHALL include `no rules configured` in the issues list.

### Requirement 14: CLI Completeness

**User Story:** As a developer, I want the `cargo proofrun` CLI to support all commands from the Reference_Implementation, so that the Rust binary is a drop-in replacement.

#### Acceptance Criteria

1. THE CLI SHALL support the `plan` subcommand with `--base`, `--head`, `--profile` (default `ci`), and `--repo` (default `.`) arguments.
2. THE CLI SHALL support the `explain` subcommand with `--plan` (default `.proofrun/plan.json`) argument.
3. THE CLI SHALL support the `emit shell` subcommand with `--plan` (default `.proofrun/plan.json`) argument.
4. THE CLI SHALL support the `emit github-actions` subcommand with `--plan` (default `.proofrun/plan.json`) argument.
5. THE CLI SHALL support the `run` subcommand with `--plan` (default `.proofrun/plan.json`) and `--dry-run` flag arguments.
6. THE CLI SHALL support the `doctor` subcommand with `--repo` (default `.`) argument.
7. WHEN the `plan` subcommand completes, THE CLI SHALL print the plan JSON to stdout and write all plan artifacts to the output directory.

### Requirement 15: Artifact Parity with Reference Implementation

**User Story:** As a developer, I want the Rust implementation to produce artifacts identical to the Python reference for the same inputs, so that the port is verified correct.

#### Acceptance Criteria

1. FOR ALL fixture scenarios in `fixtures/demo-workspace`, THE Planner SHALL produce `plan.json` with identical `obligations`, `selected_surfaces`, `omitted_surfaces`, `changed_paths`, and `workspace` fields compared to the Reference_Implementation output.
2. FOR ALL fixture scenarios, THE Planner SHALL produce `plan.md`, `commands.sh`, and `github-actions.yml` content identical to the Reference_Implementation output.
3. FOR ALL fixture scenarios with dry-run execution, THE Execution_Engine SHALL produce `receipt.json` with identical `status`, `steps` count, step `id` values, and step `exit_code` values compared to the Reference_Implementation output.
4. THE Planner SHALL use version string `0.1.0-ref` to match the Reference_Implementation version during the porting phase.

### Requirement 16: Deterministic Output

**User Story:** As a developer, I want the Planner to produce identical output for identical inputs regardless of execution environment, so that plans are reproducible and auditable.

#### Acceptance Criteria

1. THE Planner SHALL sort all collections in plan artifacts using stable, lexicographic ordering.
2. THE Planner SHALL use canonical JSON serialization (sorted keys, no-whitespace separators) for digest computation.
3. THE Planner SHALL use UTC timestamps with second precision in ISO 8601 format ending with `Z`.
