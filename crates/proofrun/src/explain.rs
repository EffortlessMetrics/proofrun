use crate::emit::shell_join;
use crate::model::Plan;

/// Format an f64 cost to match Python's JSON float formatting.
/// Python's `json.dumps` always includes a `.0` for whole-number floats.
fn format_cost(cost: f64) -> String {
    if cost.fract() == 0.0 && cost.is_finite() {
        format!("{cost:.1}")
    } else {
        format!("{cost}")
    }
}

/// Render a Plan as a Markdown string matching the reference `plan_markdown` format.
pub fn render_explanation(plan: &Plan) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("# proofrun plan".to_string());
    lines.push(String::new());
    lines.push(format!("- range: `{}..{}`", plan.base, plan.head));
    lines.push(format!("- merge base: `{}`", plan.merge_base));
    lines.push(format!("- profile: `{}`", plan.profile));
    lines.push(format!("- plan digest: `{}`", plan.plan_digest));
    lines.push(String::new());

    lines.push("## Changed paths".to_string());
    lines.push(String::new());
    for change in &plan.changed_paths {
        let owner = change.owner.as_deref().unwrap_or("unowned");
        lines.push(format!(
            "- `{}` `{}` \u{2192} `{}`",
            change.status, change.path, owner
        ));
    }
    lines.push(String::new());

    lines.push("## Obligations".to_string());
    lines.push(String::new());
    for obligation in &plan.obligations {
        lines.push(format!("- `{}`", obligation.id));
        for reason in &obligation.reasons {
            let source = &reason.source;
            let path = reason
                .path
                .as_deref()
                .map_or("None".to_string(), |p| p.to_string());
            let rule = reason
                .rule
                .as_deref()
                .map_or("None".to_string(), |r| r.to_string());
            let pattern = reason
                .pattern
                .as_deref()
                .map_or("None".to_string(), |p| p.to_string());
            lines.push(format!(
                "  - source={source}, path={path}, rule={rule}, pattern={pattern}"
            ));
        }
    }
    lines.push(String::new());

    lines.push("## Selected surfaces".to_string());
    lines.push(String::new());
    for surface in &plan.selected_surfaces {
        lines.push(format!(
            "- `{}` \u{2014} cost `{}`",
            surface.id,
            format_cost(surface.cost)
        ));
        lines.push(format!("  - covers: {}", surface.covers.join(", ")));
        lines.push(format!("  - run: `{}`", shell_join(&surface.run)));
    }

    if !plan.diagnostics.is_empty() {
        lines.push(String::new());
        lines.push("## Diagnostics".to_string());
        lines.push(String::new());
        for diagnostic in &plan.diagnostics {
            lines.push(format!("- {diagnostic}"));
        }
    }

    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn make_core_change_plan() -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-03-31T12:02:55Z".to_string(),
            repo_root: "/mnt/data/proofrun/fixtures/demo-workspace/repo".to_string(),
            base: "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717".to_string(),
            head: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            merge_base: "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717".to_string(),
            profile: "ci".to_string(),
            config_digest: "859f3ea0c010418cabf3ca3b1c07cb6bbaa57398515df52a54b017d2a3bbc8a1"
                .to_string(),
            plan_digest: "66c833f979ebd3f92cd7dcb1a350c67242fbbe614c55e5100487e9625167776c"
                .to_string(),
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo {
                packages: vec![
                    WorkspacePackage {
                        name: "app".to_string(),
                        dir: "crates/app".to_string(),
                        manifest: "crates/app/Cargo.toml".to_string(),
                        dependencies: vec!["core".to_string()],
                        reverse_dependencies: vec![],
                    },
                    WorkspacePackage {
                        name: "core".to_string(),
                        dir: "crates/core".to_string(),
                        manifest: "crates/core/Cargo.toml".to_string(),
                        dependencies: vec![],
                        reverse_dependencies: vec!["app".to_string()],
                    },
                ],
            },
            changed_paths: vec![ChangedPath {
                path: "crates/core/src/lib.rs".to_string(),
                status: "M".to_string(),
                owner: Some("core".to_string()),
            }],
            obligations: vec![
                ObligationRecord {
                    id: "pkg:core:mutation-diff".to_string(),
                    reasons: vec![ObligationReason {
                        source: "rule".to_string(),
                        path: Some("crates/core/src/lib.rs".to_string()),
                        rule: Some("rule:1".to_string()),
                        pattern: Some("crates/*/src/**/*.rs".to_string()),
                    }],
                },
                ObligationRecord {
                    id: "pkg:core:tests".to_string(),
                    reasons: vec![ObligationReason {
                        source: "rule".to_string(),
                        path: Some("crates/core/src/lib.rs".to_string()),
                        rule: Some("rule:1".to_string()),
                        pattern: Some("crates/*/src/**/*.rs".to_string()),
                    }],
                },
                ObligationRecord {
                    id: "workspace:smoke".to_string(),
                    reasons: vec![ObligationReason {
                        source: "profile".to_string(),
                        path: None,
                        rule: Some("ci".to_string()),
                        pattern: None,
                    }],
                },
            ],
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
            omitted_surfaces: vec![OmittedSurface {
                id: "workspace.all-tests".to_string(),
                reason: "not selected by optimal weighted cover".to_string(),
            }],
            diagnostics: vec![],
        }
    }

    #[test]
    fn test_core_change_plan_markdown_matches_fixture() {
        let plan = make_core_change_plan();
        let actual = render_explanation(&plan);
        let expected = include_str!("../../../fixtures/demo-workspace/sample/core-change/plan.md");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_docs_change_plan_markdown_matches_fixture() {
        let plan = Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-03-31T12:03:10Z".to_string(),
            repo_root: "/mnt/data/proofrun/fixtures/demo-workspace/repo".to_string(),
            base: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            head: "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd".to_string(),
            merge_base: "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: "76bbed98f4c351b58080b822b279043a1338c1170bc1060d68fa0c36a2c23e7f"
                .to_string(),
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
            obligations: vec![
                ObligationRecord {
                    id: "workspace:docs".to_string(),
                    reasons: vec![ObligationReason {
                        source: "rule".to_string(),
                        path: Some("docs/guide.md".to_string()),
                        rule: Some("rule:3".to_string()),
                        pattern: Some("docs/**".to_string()),
                    }],
                },
                ObligationRecord {
                    id: "workspace:smoke".to_string(),
                    reasons: vec![ObligationReason {
                        source: "profile".to_string(),
                        path: None,
                        rule: Some("ci".to_string()),
                        pattern: None,
                    }],
                },
            ],
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
        };
        let actual = render_explanation(&plan);
        let expected = include_str!("../../../fixtures/demo-workspace/sample/docs-change/plan.md");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_diagnostics_section_included_when_nonempty() {
        let mut plan = make_core_change_plan();
        plan.diagnostics = vec!["unowned path foo/bar.rs matched rule 2".to_string()];
        let md = render_explanation(&plan);
        assert!(md.contains("## Diagnostics"));
        assert!(md.contains("- unowned path foo/bar.rs matched rule 2"));
    }

    #[test]
    fn test_diagnostics_section_omitted_when_empty() {
        let plan = make_core_change_plan();
        let md = render_explanation(&plan);
        assert!(!md.contains("## Diagnostics"));
    }

    #[test]
    fn test_none_fields_render_as_none_string() {
        let plan = Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/tmp".to_string(),
            base: "aaa".to_string(),
            head: "bbb".to_string(),
            merge_base: "aaa".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: "abc123".to_string(),
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
            obligations: vec![ObligationRecord {
                id: "workspace:smoke".to_string(),
                reasons: vec![ObligationReason {
                    source: "profile".to_string(),
                    path: None,
                    rule: None,
                    pattern: None,
                }],
            }],
            selected_surfaces: vec![],
            omitted_surfaces: vec![],
            diagnostics: vec![],
        };
        let md = render_explanation(&plan);
        assert!(md.contains("source=profile, path=None, rule=None, pattern=None"));
    }

    // ── Property-based tests ──

    use proptest::prelude::*;

    /// Strategy for a hex-like string (8-40 hex chars).
    fn arb_hex() -> impl Strategy<Value = String> {
        "[0-9a-f]{8,40}"
    }

    /// Strategy for a simple profile name.
    fn arb_profile() -> impl Strategy<Value = String> {
        "[a-z]{2,8}"
    }

    /// Strategy for a file path.
    fn arb_path() -> impl Strategy<Value = String> {
        "[a-z]{1,5}(/[a-z]{1,5}){0,3}\\.[a-z]{1,3}"
    }

    /// Strategy for a git status letter.
    fn arb_status() -> impl Strategy<Value = String> {
        prop_oneof![Just("M"), Just("A"), Just("D"), Just("R"), Just("C")].prop_map(String::from)
    }

    /// Strategy for a ChangedPath with optional owner.
    fn arb_changed_path() -> impl Strategy<Value = ChangedPath> {
        (arb_path(), arb_status(), proptest::option::of("[a-z]{2,6}")).prop_map(
            |(path, status, owner)| ChangedPath {
                path,
                status,
                owner,
            },
        )
    }

    /// Strategy for an ObligationReason.
    fn arb_reason() -> impl Strategy<Value = ObligationReason> {
        (
            "[a-z]{3,8}",
            proptest::option::of(arb_path()),
            proptest::option::of("[a-z]{1,4}:[0-9]{1,2}"),
            proptest::option::of("[a-z/*]{2,10}"),
        )
            .prop_map(|(source, path, rule, pattern)| ObligationReason {
                source,
                path,
                rule,
                pattern,
            })
    }

    /// Strategy for an ObligationRecord with 1-2 reasons.
    fn arb_obligation() -> impl Strategy<Value = ObligationRecord> {
        (
            "[a-z]{2,6}(:[a-z]{2,6}){0,2}",
            prop::collection::vec(arb_reason(), 1..=2),
        )
            .prop_map(|(id, reasons)| ObligationRecord { id, reasons })
    }

    /// Strategy for a SelectedSurface with random data.
    fn arb_surface() -> impl Strategy<Value = SelectedSurface> {
        (
            "[a-z]{2,6}(\\.[a-z]{2,4}){0,2}",
            1.0f64..100.0f64,
            prop::collection::vec("[a-z]{2,6}(:[a-z]{2,6}){0,2}", 1..=3),
            prop::collection::vec("[a-z0-9./-]{1,12}", 1..=4),
        )
            .prop_map(|(id, cost, covers, run)| SelectedSurface {
                id: id.clone(),
                template: id,
                cost,
                covers,
                run,
            })
    }

    /// Build a Plan from generated components.
    fn make_arb_plan(
        base: String,
        head: String,
        merge_base: String,
        profile: String,
        plan_digest: String,
        changed_paths: Vec<ChangedPath>,
        obligations: Vec<ObligationRecord>,
        selected_surfaces: Vec<SelectedSurface>,
        diagnostics: Vec<String>,
    ) -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/tmp".to_string(),
            base,
            head,
            merge_base,
            profile,
            config_digest: String::new(),
            plan_digest,
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths,
            obligations,
            selected_surfaces,
            omitted_surfaces: vec![],
            diagnostics,
        }
    }

    // Feature: rust-native-planner, Property 10: Markdown emitter contains all plan data
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 9.1**
        ///
        /// For any Plan, render_explanation produces output containing:
        /// the base..head range, merge_base, profile, plan_digest,
        /// every changed path with status, every obligation id,
        /// every selected surface id, and a Diagnostics section
        /// if and only if diagnostics is non-empty.
        #[test]
        fn prop_markdown_emitter_contains_all_plan_data(
            base in arb_hex(),
            head in arb_hex(),
            merge_base in arb_hex(),
            profile in arb_profile(),
            plan_digest in arb_hex(),
            changed_paths in prop::collection::vec(arb_changed_path(), 1..=3),
            obligations in prop::collection::vec(arb_obligation(), 1..=3),
            selected_surfaces in prop::collection::vec(arb_surface(), 1..=3),
            diagnostics in prop::collection::vec("[a-z ]{5,30}", 0..=2),
        ) {
            let plan = make_arb_plan(
                base.clone(), head.clone(), merge_base.clone(),
                profile.clone(), plan_digest.clone(),
                changed_paths, obligations, selected_surfaces, diagnostics,
            );
            let md = render_explanation(&plan);

            // Range
            let range_str = format!("{}..{}", base, head);
            prop_assert!(
                md.contains(&range_str),
                "output must contain range '{}', got:\n{}", range_str, md
            );

            // Merge base
            prop_assert!(
                md.contains(&merge_base),
                "output must contain merge_base '{}'", merge_base
            );

            // Profile
            prop_assert!(
                md.contains(&format!("`{}`", profile)),
                "output must contain profile '{}'", profile
            );

            // Plan digest
            prop_assert!(
                md.contains(&plan_digest),
                "output must contain plan_digest '{}'", plan_digest
            );

            // Every changed path with its status
            for cp in &plan.changed_paths {
                prop_assert!(
                    md.contains(&format!("`{}`", cp.status)),
                    "output must contain status '{}' for path '{}'", cp.status, cp.path
                );
                prop_assert!(
                    md.contains(&format!("`{}`", cp.path)),
                    "output must contain path '{}'", cp.path
                );
            }

            // Every obligation id
            for obl in &plan.obligations {
                prop_assert!(
                    md.contains(&format!("`{}`", obl.id)),
                    "output must contain obligation id '{}'", obl.id
                );
            }

            // Every selected surface id
            for surf in &plan.selected_surfaces {
                prop_assert!(
                    md.contains(&format!("`{}`", surf.id)),
                    "output must contain surface id '{}'", surf.id
                );
            }

            // Diagnostics section iff non-empty
            if plan.diagnostics.is_empty() {
                prop_assert!(
                    !md.contains("## Diagnostics"),
                    "output must NOT contain Diagnostics section when diagnostics is empty"
                );
            } else {
                prop_assert!(
                    md.contains("## Diagnostics"),
                    "output must contain Diagnostics section when diagnostics is non-empty"
                );
                for diag in &plan.diagnostics {
                    prop_assert!(
                        md.contains(diag),
                        "output must contain diagnostic '{}'", diag
                    );
                }
            }
        }
    }
}
