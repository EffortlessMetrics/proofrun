use anyhow::Context;
use camino::Utf8Path;

use crate::model::Plan;

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
}
