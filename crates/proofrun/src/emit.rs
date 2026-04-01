use anyhow::Context;
use camino::Utf8Path;
use serde::Serialize;

use crate::model::Plan;

/// A single entry in the CI matrix output.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixEntry {
    pub id: String,
    pub template: String,
    pub cost: f64,
    pub covers: Vec<String>,
    pub run: String,
}

/// Emit selected surfaces as a JSON array for CI matrix strategy.
///
/// Builds a `Vec<MatrixEntry>` sorted by id, serializes with 2-space indent
/// and trailing newline. Empty plan → `"[]\n"`.
pub fn emit_matrix_json(plan: &Plan) -> String {
    let mut entries: Vec<MatrixEntry> = plan
        .selected_surfaces
        .iter()
        .map(|s| MatrixEntry {
            id: s.id.clone(),
            template: s.template.clone(),
            cost: s.cost,
            covers: s.covers.clone(),
            run: shell_join(&s.run),
        })
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    serde_json::to_string_pretty(&entries).unwrap() + "\n"
}

/// Emit structured JSON with selected/omitted surfaces, obligations, and metadata.
///
/// Builds a JSON object with sorted keys, 2-space indent, trailing newline.
pub fn emit_structured_json(plan: &Plan) -> String {
    let mut map = serde_json::Map::new();
    map.insert(
        "base".to_string(),
        serde_json::Value::String(plan.base.clone()),
    );
    map.insert(
        "head".to_string(),
        serde_json::Value::String(plan.head.clone()),
    );
    map.insert(
        "merge_base".to_string(),
        serde_json::Value::String(plan.merge_base.clone()),
    );
    map.insert(
        "obligations".to_string(),
        serde_json::to_value(&plan.obligations).unwrap(),
    );
    map.insert(
        "omitted_surfaces".to_string(),
        serde_json::to_value(&plan.omitted_surfaces).unwrap(),
    );
    map.insert(
        "plan_digest".to_string(),
        serde_json::Value::String(plan.plan_digest.clone()),
    );
    map.insert(
        "profile".to_string(),
        serde_json::Value::String(plan.profile.clone()),
    );
    map.insert(
        "selected_surfaces".to_string(),
        serde_json::to_value(&plan.selected_surfaces).unwrap(),
    );
    let value = serde_json::Value::Object(map);
    serde_json::to_string_pretty(&value).unwrap() + "\n"
}

/// Emit nextest filterset expressions for surfaces with `-E` arguments.
///
/// Iterates selected surfaces sorted by id, finds `-E` in run args,
/// outputs `{id}\t{expression}\n`. Surfaces without `-E` are omitted.
pub fn emit_nextest_filtersets(plan: &Plan) -> String {
    let mut surfaces = plan.selected_surfaces.clone();
    surfaces.sort_by(|a, b| a.id.cmp(&b.id));
    let mut output = String::new();
    for surface in &surfaces {
        if let Some(pos) = surface.run.iter().position(|arg| arg == "-E") {
            if let Some(expr) = surface.run.get(pos + 1) {
                output.push_str(&format!("{}\t{}\n", surface.id, expr));
            }
        }
    }
    output
}

/// Write all plan artifacts to the output directory.
///
/// Creates the output directory, then writes:
/// - `diff.patch` — raw patch string
/// - `plan.json` — sorted keys, 2-space indent, trailing newline
/// - `plan.md` — Markdown plan summary
/// - `commands.sh` — bash script (set executable on Unix)
/// - `github-actions.yml` — GitHub Actions step
pub fn write_plan_artifacts(repo_root: &Utf8Path, plan: &Plan, patch: &str) -> anyhow::Result<()> {
    let out_dir = repo_root.join(&plan.artifacts.output_dir);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create output directory {out_dir}"))?;

    // diff.patch
    let diff_path = repo_root.join(&plan.artifacts.diff_patch);
    std::fs::write(&diff_path, patch).with_context(|| format!("failed to write {diff_path}"))?;

    // plan.json — serialize through serde_json::Value for sorted keys
    let plan_value =
        serde_json::to_value(plan).context("failed to serialize plan to JSON value")?;
    let plan_json = serde_json::to_string_pretty(&plan_value)
        .context("failed to serialize plan to pretty JSON")?
        + "\n";
    let plan_json_path = repo_root.join(&plan.artifacts.plan_json);
    std::fs::write(&plan_json_path, &plan_json)
        .with_context(|| format!("failed to write {plan_json_path}"))?;

    // plan.md
    let plan_md_path = repo_root.join(&plan.artifacts.plan_markdown);
    std::fs::write(&plan_md_path, emit_plan_markdown(plan))
        .with_context(|| format!("failed to write {plan_md_path}"))?;

    // commands.sh
    let shell_path = repo_root.join(&plan.artifacts.commands_shell);
    std::fs::write(&shell_path, emit_commands_shell(plan))
        .with_context(|| format!("failed to write {shell_path}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&shell_path, perms)
            .with_context(|| format!("failed to set executable permission on {shell_path}"))?;
    }

    // github-actions.yml
    let gha_path = repo_root.join(&plan.artifacts.github_actions);
    std::fs::write(&gha_path, emit_github_actions(plan))
        .with_context(|| format!("failed to write {gha_path}"))?;

    Ok(())
}

pub fn emit_plan_markdown(plan: &Plan) -> String {
    crate::explain::render_explanation(plan)
}

/// Emit a bash script matching the reference `emit_shell` exactly.
///
/// Format:
/// ```text
/// #!/usr/bin/env bash
/// set -euo pipefail
///
/// # surface_id_1
/// command1 arg1 arg2
///
/// # surface_id_2
/// command2 arg1 arg2
/// ```
///
/// Ends with a single trailing newline.
pub fn emit_commands_shell(plan: &Plan) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("#!/usr/bin/env bash".to_string());
    lines.push("set -euo pipefail".to_string());
    lines.push(String::new());
    for surface in &plan.selected_surfaces {
        lines.push(format!("# {}", surface.id));
        lines.push(shell_join(&surface.run));
        lines.push(String::new());
    }
    // Match Python: "\n".join(lines).rstrip() + "\n"
    let joined = lines.join("\n");
    joined.trim_end().to_string() + "\n"
}

/// Emit GitHub Actions YAML matching the reference `emit_github_actions` exactly.
///
/// Format:
/// ```text
/// steps:
///   - name: Execute proof plan
///     run: |
///       command1 arg1 arg2
///       command2 arg1 arg2
/// ```
///
/// Ends with a single trailing newline.
pub fn emit_github_actions(plan: &Plan) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("steps:".to_string());
    lines.push("  - name: Execute proof plan".to_string());
    lines.push("    run: |".to_string());
    for surface in &plan.selected_surfaces {
        lines.push(format!("      {}", shell_join(&surface.run)));
    }
    lines.join("\n") + "\n"
}

/// Join arguments into a shell command string, matching Python's `shlex.join`.
pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quote a single argument for shell use, matching Python's `shlex.quote`.
/// Python considers safe: `\w` (alphanumeric + underscore) and `@%+=:,./-`
/// Everything else triggers single-quoting.
fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "_@%+=:,./-".contains(ch))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    /// Build the Plan struct matching the core-change fixture data.
    fn make_core_change_plan() -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-03-31T12:02:55Z".to_string(),
            repo_root: "/mnt/data/proofrun/fixtures/demo-workspace/repo".to_string(),
            base: "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717".to_string(),
            head: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            merge_base: "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: String::new(),
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths: vec![ChangedPath {
                path: "crates/core/src/lib.rs".to_string(),
                status: "M".to_string(),
                owner: Some("core".to_string()),
            }],
            obligations: vec![],
            selected_surfaces: vec![
                SelectedSurface {
                    id: "mutation.diff[pkg=core]".to_string(),
                    template: "mutation.diff".to_string(),
                    cost: 13.0,
                    covers: vec!["pkg:core:mutation-diff".to_string()],
                    run: vec![
                        "cargo".to_string(),
                        "mutants".to_string(),
                        "--in-diff".to_string(),
                        "/mnt/data/proofrun/fixtures/demo-workspace/repo/.proofrun/diff.patch"
                            .to_string(),
                        "--package".to_string(),
                        "core".to_string(),
                    ],
                },
                SelectedSurface {
                    id: "tests.pkg[pkg=core]".to_string(),
                    template: "tests.pkg".to_string(),
                    cost: 3.0,
                    covers: vec!["pkg:core:tests".to_string()],
                    run: vec![
                        "cargo".to_string(),
                        "nextest".to_string(),
                        "run".to_string(),
                        "--profile".to_string(),
                        "ci".to_string(),
                        "-E".to_string(),
                        "package(core)".to_string(),
                    ],
                },
                SelectedSurface {
                    id: "workspace.smoke".to_string(),
                    template: "workspace.smoke".to_string(),
                    cost: 2.0,
                    covers: vec!["workspace:smoke".to_string()],
                    run: vec![
                        "cargo".to_string(),
                        "test".to_string(),
                        "--workspace".to_string(),
                        "--quiet".to_string(),
                    ],
                },
            ],
            omitted_surfaces: vec![],
            diagnostics: vec![],
        }
    }

    /// Build the Plan struct matching the docs-change fixture data.
    fn make_docs_change_plan() -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-03-31T12:03:10Z".to_string(),
            repo_root: "/mnt/data/proofrun/fixtures/demo-workspace/repo".to_string(),
            base: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            head: "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd".to_string(),
            merge_base: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: String::new(),
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths: vec![ChangedPath {
                path: "docs/guide.md".to_string(),
                status: "M".to_string(),
                owner: None,
            }],
            obligations: vec![],
            selected_surfaces: vec![
                SelectedSurface {
                    id: "workspace.docs".to_string(),
                    template: "workspace.docs".to_string(),
                    cost: 4.0,
                    covers: vec!["workspace:docs".to_string()],
                    run: vec![
                        "cargo".to_string(),
                        "doc".to_string(),
                        "--workspace".to_string(),
                        "--no-deps".to_string(),
                    ],
                },
                SelectedSurface {
                    id: "workspace.smoke".to_string(),
                    template: "workspace.smoke".to_string(),
                    cost: 2.0,
                    covers: vec!["workspace:smoke".to_string()],
                    run: vec![
                        "cargo".to_string(),
                        "test".to_string(),
                        "--workspace".to_string(),
                        "--quiet".to_string(),
                    ],
                },
            ],
            omitted_surfaces: vec![],
            diagnostics: vec![],
        }
    }

    // ── shell_quote / shell_join unit tests ──

    #[test]
    fn test_shell_quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn test_shell_quote_safe() {
        assert_eq!(shell_quote("cargo"), "cargo");
        assert_eq!(shell_quote("--workspace"), "--workspace");
        assert_eq!(shell_quote("path/to/file.rs"), "path/to/file.rs");
        assert_eq!(shell_quote("key=value"), "key=value");
    }

    #[test]
    fn test_shell_quote_needs_quoting() {
        assert_eq!(shell_quote("package(core)"), "'package(core)'");
        assert_eq!(shell_quote("hello world"), "'hello world'");
    }

    #[test]
    fn test_shell_quote_single_quote_in_arg() {
        // Python shlex.quote("it's") wraps in single quotes and replaces
        // each embedded ' with '"'"' (end quote, double-quoted quote, reopen).
        // Result: 'it'"'"'s'
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn test_shell_join_basic() {
        let args: Vec<String> = vec![
            "cargo".to_string(),
            "nextest".to_string(),
            "run".to_string(),
            "-E".to_string(),
            "package(core)".to_string(),
        ];
        assert_eq!(shell_join(&args), "cargo nextest run -E 'package(core)'");
    }

    // ── emit_commands_shell fixture tests ──

    #[test]
    fn test_core_change_commands_shell_matches_fixture() {
        let plan = make_core_change_plan();
        let actual = emit_commands_shell(&plan);
        let expected =
            include_str!("../../../fixtures/demo-workspace/sample/core-change/commands.sh");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_docs_change_commands_shell_matches_fixture() {
        let plan = make_docs_change_plan();
        let actual = emit_commands_shell(&plan);
        let expected =
            include_str!("../../../fixtures/demo-workspace/sample/docs-change/commands.sh");
        assert_eq!(actual, expected);
    }

    // ── emit_github_actions fixture tests ──

    #[test]
    fn test_core_change_github_actions_matches_fixture() {
        let plan = make_core_change_plan();
        let actual = emit_github_actions(&plan);
        let expected =
            include_str!("../../../fixtures/demo-workspace/sample/core-change/github-actions.yml");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_docs_change_github_actions_matches_fixture() {
        let plan = make_docs_change_plan();
        let actual = emit_github_actions(&plan);
        let expected =
            include_str!("../../../fixtures/demo-workspace/sample/docs-change/github-actions.yml");
        assert_eq!(actual, expected);
    }

    // ── Structural tests ──

    #[test]
    fn test_shell_starts_with_shebang() {
        let plan = make_docs_change_plan();
        let output = emit_commands_shell(&plan);
        assert!(output.starts_with("#!/usr/bin/env bash\nset -euo pipefail\n"));
    }

    #[test]
    fn test_shell_ends_with_single_trailing_newline() {
        let plan = make_docs_change_plan();
        let output = emit_commands_shell(&plan);
        assert!(output.ends_with('\n'));
        assert!(!output.ends_with("\n\n"));
    }

    #[test]
    fn test_github_actions_starts_with_header() {
        let plan = make_docs_change_plan();
        let output = emit_github_actions(&plan);
        assert!(output.starts_with("steps:\n  - name: Execute proof plan\n    run: |\n"));
    }

    #[test]
    fn test_github_actions_ends_with_single_trailing_newline() {
        let plan = make_docs_change_plan();
        let output = emit_github_actions(&plan);
        assert!(output.ends_with('\n'));
        assert!(!output.ends_with("\n\n"));
    }

    // ── Property-based tests ──

    use proptest::prelude::*;

    /// Strategy for generating a random SelectedSurface with simple safe args.
    fn arb_surface() -> impl Strategy<Value = SelectedSurface> {
        (
            "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}",
            prop::collection::vec("[a-z0-9./-]{1,12}", 1..=5),
        )
            .prop_map(|(id, run)| SelectedSurface {
                id: id.clone(),
                template: id,
                cost: 1.0,
                covers: vec![],
                run,
            })
    }

    fn make_plan_with_surfaces(surfaces: Vec<SelectedSurface>) -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/tmp".to_string(),
            base: "aaa".to_string(),
            head: "bbb".to_string(),
            merge_base: "aaa".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: String::new(),
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths: vec![],
            obligations: vec![],
            selected_surfaces: surfaces,
            omitted_surfaces: vec![],
            diagnostics: vec![],
        }
    }

    // Feature: rust-native-planner, Property 11: Shell emitter structure
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 10.1, 10.2, 10.3**
        ///
        /// For any Plan with N selected surfaces, emit_commands_shell produces
        /// a string that starts with the bash header, contains N comment-command
        /// blocks, and ends with a single trailing newline.
        #[test]
        fn prop_shell_emitter_structure(
            surfaces in prop::collection::vec(arb_surface(), 1..=5)
        ) {
            let n = surfaces.len();
            let plan = make_plan_with_surfaces(surfaces);
            let output = emit_commands_shell(&plan);

            // (a) starts with shebang + set
            prop_assert!(
                output.starts_with("#!/usr/bin/env bash\nset -euo pipefail\n"),
                "shell output must start with bash header"
            );

            // (b) exactly N comment lines (# ...)
            let comment_count = output.lines().filter(|l| l.starts_with("# ")).count();
            prop_assert_eq!(
                comment_count, n,
                "expected {} comment lines, got {}",
                n, comment_count
            );

            // (c) ends with single trailing newline
            prop_assert!(output.ends_with('\n'), "must end with newline");
            prop_assert!(!output.ends_with("\n\n"), "must not end with double newline");
        }
    }

    // Feature: rust-native-planner, Property 12: GitHub Actions emitter structure
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 11.1, 11.2**
        ///
        /// For any Plan with N selected surfaces, emit_github_actions produces
        /// a string that starts with the YAML header, contains N indented command
        /// lines, and ends with a single trailing newline.
        #[test]
        fn prop_github_actions_emitter_structure(
            surfaces in prop::collection::vec(arb_surface(), 1..=5)
        ) {
            let n = surfaces.len();
            let plan = make_plan_with_surfaces(surfaces);
            let output = emit_github_actions(&plan);

            // (a) starts with YAML header
            prop_assert!(
                output.starts_with("steps:\n  - name: Execute proof plan\n    run: |\n"),
                "github actions output must start with YAML header"
            );

            // (b) exactly N indented command lines (6-space indent)
            let cmd_count = output.lines().filter(|l| l.starts_with("      ")).count();
            prop_assert_eq!(
                cmd_count, n,
                "expected {} command lines, got {}",
                n, cmd_count
            );

            // (c) ends with single trailing newline
            prop_assert!(output.ends_with('\n'), "must end with newline");
            prop_assert!(!output.ends_with("\n\n"), "must not end with double newline");
        }
    }

    // ── Helpers for new emitter property tests ──

    /// Strategy for generating a SelectedSurface with unique id, non-zero cost,
    /// covers list, and run args that optionally include `-E <expression>`.
    fn arb_surface_rich() -> impl Strategy<Value = SelectedSurface> {
        (
            "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}(\\[pkg=[a-z]{1,4}\\])?",
            "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}",
            1.0..100.0_f64,
            prop::collection::vec("[a-z0-9:._-]{1,12}", 1..=3),
            prop::collection::vec("[a-z0-9./-]{1,12}", 1..=3),
            prop::bool::ANY,
            "[a-z()]{3,15}",
        )
            .prop_map(|(id, template, cost, covers, base_run, has_e, expr)| {
                let mut run = base_run;
                if has_e {
                    run.push("-E".to_string());
                    run.push(expr);
                }
                SelectedSurface {
                    id,
                    template,
                    cost,
                    covers,
                    run,
                }
            })
    }

    /// Strategy for generating an OmittedSurface.
    fn arb_omitted_surface() -> impl Strategy<Value = OmittedSurface> {
        (
            "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}",
            "covered-by-[a-z]{2,6}|redundant|cost-exceeded",
        )
            .prop_map(|(id, reason)| OmittedSurface { id, reason })
    }

    /// Strategy for generating an ObligationRecord.
    fn arb_obligation() -> impl Strategy<Value = ObligationRecord> {
        (
            "[a-z]{1,4}:[a-z]{1,6}:[a-z]{1,6}",
            prop::collection::vec(
                (
                    "profile|rule|unknown-fallback",
                    proptest::option::of("[a-z/]{3,12}"),
                    proptest::option::of("[a-z*]{2,8}"),
                    proptest::option::of("[a-z*]{2,8}"),
                ),
                1..=2,
            ),
        )
            .prop_map(|(id, reasons)| ObligationRecord {
                id,
                reasons: reasons
                    .into_iter()
                    .map(|(source, path, rule, pattern)| ObligationReason {
                        source,
                        path,
                        rule,
                        pattern,
                    })
                    .collect(),
            })
    }

    /// Build a Plan with rich data: surfaces, omitted surfaces, obligations, and metadata.
    fn make_rich_plan(
        surfaces: Vec<SelectedSurface>,
        omitted: Vec<OmittedSurface>,
        obligations: Vec<ObligationRecord>,
    ) -> Plan {
        let mut plan = make_plan_with_surfaces(surfaces);
        plan.omitted_surfaces = omitted;
        plan.obligations = obligations;
        plan.base = "abc123".to_string();
        plan.head = "def456".to_string();
        plan.merge_base = "abc123".to_string();
        plan.profile = "ci".to_string();
        plan.plan_digest = "sha256:deadbeef".to_string();
        plan
    }

    // ── Feature: proofrun-differentiation, Property 8: Matrix emitter structure and content ──
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4**
        ///
        /// For any Plan with N selected surfaces, emit_matrix_json produces:
        /// (a) a valid JSON array with exactly N entries,
        /// (b) each entry containing id, template, cost, covers, run fields
        ///     matching the corresponding selected surface,
        /// (c) entries sorted by id ascending,
        /// (d) 2-space indented JSON with a trailing newline.
        /// For a Plan with zero selected surfaces, the output is "[]\n".
        #[test]
        fn prop_matrix_emitter_structure(
            surfaces in prop::collection::vec(arb_surface_rich(), 0..=5)
        ) {
            let n = surfaces.len();
            let plan = make_plan_with_surfaces(surfaces.clone());
            let output = emit_matrix_json(&plan);

            // (d) trailing newline
            prop_assert!(output.ends_with('\n'), "must end with newline");

            // Parse as JSON
            let parsed: serde_json::Value = serde_json::from_str(&output)
                .expect("emit_matrix_json must produce valid JSON");

            let arr = parsed.as_array().expect("output must be a JSON array");

            // (a) correct length
            prop_assert_eq!(arr.len(), n, "array length must match surface count");

            // Empty case
            if n == 0 {
                prop_assert_eq!(output, "[]\n", "empty plan must produce \"[]\\n\"");
                return Ok(());
            }

            // (b) each entry has correct fields
            // Build expected sorted surfaces
            let mut expected = plan.selected_surfaces.clone();
            expected.sort_by(|a, b| a.id.cmp(&b.id));

            for (i, entry) in arr.iter().enumerate() {
                let obj = entry.as_object().expect("each entry must be an object");
                prop_assert!(obj.contains_key("id"), "entry must have 'id'");
                prop_assert!(obj.contains_key("template"), "entry must have 'template'");
                prop_assert!(obj.contains_key("cost"), "entry must have 'cost'");
                prop_assert!(obj.contains_key("covers"), "entry must have 'covers'");
                prop_assert!(obj.contains_key("run"), "entry must have 'run'");

                // Verify values match the expected sorted surface
                let exp_surface = &expected[i];
                prop_assert_eq!(
                    obj["id"].as_str().unwrap(),
                    &exp_surface.id,
                    "id mismatch at index {}",
                    i
                );
                prop_assert_eq!(
                    obj["template"].as_str().unwrap(),
                    &exp_surface.template,
                    "template mismatch at index {}",
                    i
                );
                // Cost: compare with tolerance for f64 JSON round-trip
                let parsed_cost = obj["cost"].as_f64().unwrap();
                prop_assert!(
                    (parsed_cost - exp_surface.cost).abs() < 1e-10,
                    "cost mismatch at index {}: got {}, expected {}",
                    i,
                    parsed_cost,
                    exp_surface.cost
                );
                let covers: Vec<String> = obj["covers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                prop_assert_eq!(
                    covers,
                    exp_surface.covers.clone(),
                    "covers mismatch at index {}",
                    i
                );
                // run is a shell-joined string
                prop_assert_eq!(
                    obj["run"].as_str().unwrap(),
                    &shell_join(&exp_surface.run),
                    "run mismatch at index {}",
                    i
                );
            }

            // (c) entries sorted by id ascending
            let ids: Vec<&str> = arr
                .iter()
                .map(|e| e["id"].as_str().unwrap())
                .collect();
            for w in ids.windows(2) {
                prop_assert!(
                    w[0] <= w[1],
                    "entries must be sorted by id: {:?} > {:?}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    // ── Feature: proofrun-differentiation, Property 9: Structured JSON emitter content ──
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 10.1, 10.2**
        ///
        /// For any Plan, emit_structured_json produces a JSON object containing:
        /// (a) selected_surfaces matching the Plan's selected surfaces,
        /// (b) omitted_surfaces matching the Plan's omitted surfaces,
        /// (c) obligations matching the Plan's obligations,
        /// (d) base, head, merge_base, profile, plan_digest matching the Plan's fields,
        /// (e) sorted keys, 2-space indentation, and trailing newline.
        #[test]
        fn prop_structured_json_content(
            surfaces in prop::collection::vec(arb_surface_rich(), 0..=3),
            omitted in prop::collection::vec(arb_omitted_surface(), 0..=2),
            obligations in prop::collection::vec(arb_obligation(), 0..=3),
        ) {
            let plan = make_rich_plan(surfaces, omitted, obligations);
            let output = emit_structured_json(&plan);

            // (e) trailing newline
            prop_assert!(output.ends_with('\n'), "must end with newline");

            // Parse as JSON object
            let parsed: serde_json::Value = serde_json::from_str(&output)
                .expect("emit_structured_json must produce valid JSON");
            let obj = parsed.as_object().expect("output must be a JSON object");

            // (d) metadata fields match plan
            prop_assert_eq!(obj["base"].as_str().unwrap(), &plan.base);
            prop_assert_eq!(obj["head"].as_str().unwrap(), &plan.head);
            prop_assert_eq!(obj["merge_base"].as_str().unwrap(), &plan.merge_base);
            prop_assert_eq!(obj["profile"].as_str().unwrap(), &plan.profile);
            prop_assert_eq!(obj["plan_digest"].as_str().unwrap(), &plan.plan_digest);

            // (a) selected_surfaces
            let sel = obj.get("selected_surfaces").expect("must have selected_surfaces");
            let sel_arr = sel.as_array().expect("selected_surfaces must be array");
            prop_assert_eq!(
                sel_arr.len(),
                plan.selected_surfaces.len(),
                "selected_surfaces count mismatch"
            );

            // (b) omitted_surfaces
            let omit = obj.get("omitted_surfaces").expect("must have omitted_surfaces");
            let omit_arr = omit.as_array().expect("omitted_surfaces must be array");
            prop_assert_eq!(
                omit_arr.len(),
                plan.omitted_surfaces.len(),
                "omitted_surfaces count mismatch"
            );

            // (c) obligations
            let obls = obj.get("obligations").expect("must have obligations");
            let obls_arr = obls.as_array().expect("obligations must be array");
            prop_assert_eq!(
                obls_arr.len(),
                plan.obligations.len(),
                "obligations count mismatch"
            );

            // (e) sorted keys: verify keys are in lexicographic order
            let keys: Vec<&String> = obj.keys().collect();
            for w in keys.windows(2) {
                prop_assert!(
                    w[0] <= w[1],
                    "keys must be sorted: {:?} > {:?}",
                    w[0],
                    w[1]
                );
            }

            // Verify the 8 expected keys are present
            let expected_keys = [
                "base", "head", "merge_base", "obligations",
                "omitted_surfaces", "plan_digest", "profile", "selected_surfaces",
            ];
            for key in &expected_keys {
                prop_assert!(
                    obj.contains_key(*key),
                    "missing expected key: {}",
                    key
                );
            }
        }
    }

    // ── Feature: proofrun-differentiation, Property 10: Nextest filterset emitter ──
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 11.1, 11.2, 11.3, 11.4**
        ///
        /// For any Plan, emit_nextest_filtersets produces output where:
        /// (a) each line corresponds to a selected surface whose run command
        ///     contains a `-E` argument,
        /// (b) each line has format `{id}\t{expression}`,
        /// (c) lines are sorted by surface id ascending,
        /// (d) surfaces without `-E` in their run command are omitted,
        /// (e) the output ends with a trailing newline (or is empty if no
        ///     surfaces qualify).
        #[test]
        fn prop_nextest_filterset_emitter(
            surfaces in prop::collection::vec(arb_surface_rich(), 0..=5)
        ) {
            let plan = make_plan_with_surfaces(surfaces.clone());
            let output = emit_nextest_filtersets(&plan);

            // Compute expected: surfaces with -E, sorted by id
            let mut expected_lines: Vec<(String, String)> = Vec::new();
            let mut sorted_surfaces = surfaces.clone();
            sorted_surfaces.sort_by(|a, b| a.id.cmp(&b.id));
            for s in &sorted_surfaces {
                if let Some(pos) = s.run.iter().position(|arg| arg == "-E") {
                    if let Some(expr) = s.run.get(pos + 1) {
                        expected_lines.push((s.id.clone(), expr.clone()));
                    }
                }
            }

            if expected_lines.is_empty() {
                // (e) empty output when no surfaces qualify
                prop_assert_eq!(output, "", "output must be empty when no surfaces have -E");
                return Ok(());
            }

            // (e) trailing newline
            prop_assert!(output.ends_with('\n'), "must end with newline");

            let lines: Vec<&str> = output.trim_end_matches('\n').split('\n').collect();

            // (a) correct number of lines
            prop_assert_eq!(
                lines.len(),
                expected_lines.len(),
                "line count mismatch: expected {}, got {}",
                expected_lines.len(),
                lines.len()
            );

            for (i, line) in lines.iter().enumerate() {
                // (b) format: {id}\t{expression}
                let parts: Vec<&str> = line.splitn(2, '\t').collect();
                prop_assert_eq!(
                    parts.len(),
                    2,
                    "line {} must have id<tab>expression format",
                    i
                );
                let (exp_id, exp_expr) = &expected_lines[i];
                prop_assert_eq!(parts[0], exp_id.as_str(), "id mismatch at line {}", i);
                prop_assert_eq!(parts[1], exp_expr.as_str(), "expression mismatch at line {}", i);
            }

            // (c) lines sorted by surface id
            let ids: Vec<&str> = lines.iter().map(|l| l.splitn(2, '\t').next().unwrap()).collect();
            for w in ids.windows(2) {
                prop_assert!(
                    w[0] <= w[1],
                    "lines must be sorted by id: {:?} > {:?}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    // ── Feature: proofrun-differentiation, Property 19: New emitter output determinism ──
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 25.1, 25.4**
        ///
        /// For any Plan, calling the same emitter function twice with the same
        /// input produces identical output.
        #[test]
        fn prop_new_emitter_determinism(
            surfaces in prop::collection::vec(arb_surface_rich(), 0..=5),
            omitted in prop::collection::vec(arb_omitted_surface(), 0..=2),
            obligations in prop::collection::vec(arb_obligation(), 0..=3),
        ) {
            let plan = make_rich_plan(surfaces, omitted, obligations);

            // emit_matrix_json determinism
            let matrix1 = emit_matrix_json(&plan);
            let matrix2 = emit_matrix_json(&plan);
            prop_assert_eq!(&matrix1, &matrix2, "emit_matrix_json must be deterministic");

            // emit_structured_json determinism
            let json1 = emit_structured_json(&plan);
            let json2 = emit_structured_json(&plan);
            prop_assert_eq!(&json1, &json2, "emit_structured_json must be deterministic");

            // emit_nextest_filtersets determinism
            let nf1 = emit_nextest_filtersets(&plan);
            let nf2 = emit_nextest_filtersets(&plan);
            prop_assert_eq!(&nf1, &nf2, "emit_nextest_filtersets must be deterministic");
        }
    }
}
