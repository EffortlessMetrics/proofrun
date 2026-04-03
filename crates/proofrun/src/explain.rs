use crate::emit::shell_join;
use crate::model::{ObligationReason, Plan};
use serde::Serialize;

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

// ── Structured query types ──

#[derive(Debug, Clone, Serialize)]
pub struct PathExplanation {
    pub path: String,
    pub found: bool,
    pub rule_matches: Vec<RuleMatch>,
    pub obligations: Vec<String>,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleMatch {
    pub rule_index: usize,
    pub pattern: String,
    pub obligations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObligationExplanation {
    pub obligation_id: String,
    pub reasons: Vec<ObligationReason>,
    pub selected_surfaces: Vec<String>,
    pub omitted_surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceExplanation {
    pub surface_id: String,
    pub status: String,
    pub template: Option<String>,
    pub cost: Option<f64>,
    pub covers: Option<Vec<String>>,
    pub run: Option<Vec<String>>,
    pub omission_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceOutput {
    pub paths: Vec<PathTrace>,
    pub profile_obligations: Vec<ProfileObligation>,
    pub fallback_obligations: Vec<FallbackObligation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathTrace {
    pub path: String,
    pub status: String,
    pub owner: Option<String>,
    pub rule_matches: Vec<RuleMatch>,
    pub obligations: Vec<String>,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileObligation {
    pub obligation_id: String,
    pub source: String,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FallbackObligation {
    pub obligation_id: String,
    pub source: String,
    pub surfaces: Vec<String>,
}

/// Query a path's traceability through the plan: which rules matched,
/// which obligations were created, and which surfaces cover them.
pub fn query_path(plan: &Plan, path: &str) -> PathExplanation {
    let found = plan.changed_paths.iter().any(|cp| cp.path == path);

    if !found {
        return PathExplanation {
            path: path.to_string(),
            found: false,
            rule_matches: vec![],
            obligations: vec![],
            surfaces: vec![],
        };
    }

    // Find obligations that have a reason referencing this path.
    let mut obligation_ids: Vec<String> = Vec::new();
    let mut rule_matches: Vec<RuleMatch> = Vec::new();

    for obligation in &plan.obligations {
        let matching_reasons: Vec<&ObligationReason> = obligation
            .reasons
            .iter()
            .filter(|r| r.path.as_deref() == Some(path))
            .collect();

        if !matching_reasons.is_empty() {
            obligation_ids.push(obligation.id.clone());

            for reason in matching_reasons {
                if let Some(rule) = &reason.rule {
                    // Parse rule index from "rule:N" format.
                    let rule_index = rule
                        .strip_prefix("rule:")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    let pattern = reason.pattern.clone().unwrap_or_default();

                    // Check if we already have a RuleMatch for this rule_index + pattern.
                    if let Some(existing) = rule_matches
                        .iter_mut()
                        .find(|rm| rm.rule_index == rule_index && rm.pattern == pattern)
                    {
                        if !existing.obligations.contains(&obligation.id) {
                            existing.obligations.push(obligation.id.clone());
                        }
                    } else {
                        rule_matches.push(RuleMatch {
                            rule_index,
                            pattern,
                            obligations: vec![obligation.id.clone()],
                        });
                    }
                }
            }
        }
    }

    // Find surfaces that cover any of the matched obligations.
    let surfaces: Vec<String> = plan
        .selected_surfaces
        .iter()
        .filter(|s| s.covers.iter().any(|cover| obligation_ids.contains(cover)))
        .map(|s| s.id.clone())
        .collect();

    PathExplanation {
        path: path.to_string(),
        found: true,
        rule_matches,
        obligations: obligation_ids,
        surfaces,
    }
}

/// Query an obligation's traceability: reasons, selected surfaces that cover it,
/// and omitted surfaces that would have covered it.
///
/// Returns `Err` if the obligation id is not found in the plan.
pub fn query_obligation(plan: &Plan, obligation_id: &str) -> anyhow::Result<ObligationExplanation> {
    let obligation = plan
        .obligations
        .iter()
        .find(|o| o.id == obligation_id)
        .ok_or_else(|| anyhow::anyhow!("obligation id '{}' not found in plan", obligation_id))?;

    let reasons = obligation.reasons.clone();

    let selected_surfaces: Vec<String> = plan
        .selected_surfaces
        .iter()
        .filter(|s| s.covers.iter().any(|c| c == obligation_id))
        .map(|s| s.id.clone())
        .collect();

    // OmittedSurface only has id and reason — no covers field.
    // We cannot determine which omitted surfaces would have covered this obligation
    // from the Plan data alone, so we return an empty list.
    let omitted_surfaces: Vec<String> = vec![];

    Ok(ObligationExplanation {
        obligation_id: obligation_id.to_string(),
        reasons,
        selected_surfaces,
        omitted_surfaces,
    })
}

/// Query a surface's details: selected surface fields or omitted surface reason.
///
/// Returns `Err` if the surface id is in neither selected nor omitted surfaces.
pub fn query_surface(plan: &Plan, surface_id: &str) -> anyhow::Result<SurfaceExplanation> {
    // Check selected surfaces first.
    if let Some(surface) = plan.selected_surfaces.iter().find(|s| s.id == surface_id) {
        return Ok(SurfaceExplanation {
            surface_id: surface_id.to_string(),
            status: "selected".to_string(),
            template: Some(surface.template.clone()),
            cost: Some(surface.cost),
            covers: Some(surface.covers.clone()),
            run: Some(surface.run.clone()),
            omission_reason: None,
        });
    }

    // Check omitted surfaces.
    if let Some(omitted) = plan.omitted_surfaces.iter().find(|s| s.id == surface_id) {
        return Ok(SurfaceExplanation {
            surface_id: surface_id.to_string(),
            status: "omitted".to_string(),
            template: None,
            cost: None,
            covers: None,
            run: None,
            omission_reason: Some(omitted.reason.clone()),
        });
    }

    Err(anyhow::anyhow!(
        "surface id '{}' not found in plan (neither selected nor omitted)",
        surface_id
    ))
}

/// Build the full traceability chain for a plan: each changed path with its
/// rule matches, obligations, and surfaces; plus profile and fallback obligations.
pub fn trace_plan(plan: &Plan) -> TraceOutput {
    // 1. Build a PathTrace for each changed path.
    let paths: Vec<PathTrace> = plan
        .changed_paths
        .iter()
        .map(|cp| {
            // Reuse the same logic as query_path to find obligations and rule matches.
            let mut obligation_ids: Vec<String> = Vec::new();
            let mut rule_matches: Vec<RuleMatch> = Vec::new();

            for obligation in &plan.obligations {
                let matching_reasons: Vec<&ObligationReason> = obligation
                    .reasons
                    .iter()
                    .filter(|r| r.path.as_deref() == Some(cp.path.as_str()))
                    .collect();

                if !matching_reasons.is_empty() {
                    obligation_ids.push(obligation.id.clone());

                    for reason in matching_reasons {
                        if let Some(rule) = &reason.rule {
                            let rule_index = rule
                                .strip_prefix("rule:")
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(0);
                            let pattern = reason.pattern.clone().unwrap_or_default();

                            if let Some(existing) = rule_matches
                                .iter_mut()
                                .find(|rm| rm.rule_index == rule_index && rm.pattern == pattern)
                            {
                                if !existing.obligations.contains(&obligation.id) {
                                    existing.obligations.push(obligation.id.clone());
                                }
                            } else {
                                rule_matches.push(RuleMatch {
                                    rule_index,
                                    pattern,
                                    obligations: vec![obligation.id.clone()],
                                });
                            }
                        }
                    }
                }
            }

            let surfaces: Vec<String> = plan
                .selected_surfaces
                .iter()
                .filter(|s| s.covers.iter().any(|cover| obligation_ids.contains(cover)))
                .map(|s| s.id.clone())
                .collect();

            PathTrace {
                path: cp.path.clone(),
                status: cp.status.clone(),
                owner: cp.owner.clone(),
                rule_matches,
                obligations: obligation_ids,
                surfaces,
            }
        })
        .collect();

    // 2. Profile obligations: obligations where any reason has source == "profile".
    let profile_obligations: Vec<ProfileObligation> = plan
        .obligations
        .iter()
        .filter(|o| o.reasons.iter().any(|r| r.source == "profile"))
        .map(|o| {
            let surfaces: Vec<String> = plan
                .selected_surfaces
                .iter()
                .filter(|s| s.covers.iter().any(|c| c == &o.id))
                .map(|s| s.id.clone())
                .collect();
            ProfileObligation {
                obligation_id: o.id.clone(),
                source: "profile".to_string(),
                surfaces,
            }
        })
        .collect();

    // 3. Fallback obligations: obligations where any reason has source
    //    "unknown-fallback" or "empty-range-fallback".
    let fallback_obligations: Vec<FallbackObligation> = plan
        .obligations
        .iter()
        .filter(|o| {
            o.reasons
                .iter()
                .any(|r| r.source == "unknown-fallback" || r.source == "empty-range-fallback")
        })
        .map(|o| {
            let source = o
                .reasons
                .iter()
                .find(|r| r.source == "unknown-fallback" || r.source == "empty-range-fallback")
                .map(|r| r.source.clone())
                .unwrap_or_default();
            let surfaces: Vec<String> = plan
                .selected_surfaces
                .iter()
                .filter(|s| s.covers.iter().any(|c| c == &o.id))
                .map(|s| s.id.clone())
                .collect();
            FallbackObligation {
                obligation_id: o.id.clone(),
                source,
                surfaces,
            }
        })
        .collect();

    TraceOutput {
        paths,
        profile_obligations,
        fallback_obligations,
    }
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

    // ── query_path unit tests ──

    #[test]
    fn test_query_path_found_returns_obligations_and_surfaces() {
        let plan = make_core_change_plan();
        let result = query_path(&plan, "crates/core/src/lib.rs");

        assert!(result.found);
        assert_eq!(result.path, "crates/core/src/lib.rs");
        assert_eq!(
            result.obligations,
            vec!["pkg:core:mutation-diff", "pkg:core:tests"]
        );
        assert_eq!(result.rule_matches.len(), 1);
        assert_eq!(result.rule_matches[0].rule_index, 1);
        assert_eq!(result.rule_matches[0].pattern, "crates/*/src/**/*.rs");
        assert_eq!(
            result.rule_matches[0].obligations,
            vec!["pkg:core:mutation-diff", "pkg:core:tests"]
        );
        assert_eq!(
            result.surfaces,
            vec!["mutation.diff[pkg=core]", "tests.pkg[pkg=core]"]
        );
    }

    #[test]
    fn test_query_path_not_found_returns_false() {
        let plan = make_core_change_plan();
        let result = query_path(&plan, "nonexistent/file.rs");

        assert!(!result.found);
        assert_eq!(result.path, "nonexistent/file.rs");
        assert!(result.rule_matches.is_empty());
        assert!(result.obligations.is_empty());
        assert!(result.surfaces.is_empty());
    }

    #[test]
    fn test_query_path_profile_obligation_not_linked_to_path() {
        // The workspace:smoke obligation has source "profile" with no path,
        // so it should NOT appear in the query_path result for any file.
        let plan = make_core_change_plan();
        let result = query_path(&plan, "crates/core/src/lib.rs");

        assert!(!result.obligations.contains(&"workspace:smoke".to_string()));
    }

    // ── query_obligation unit tests ──

    #[test]
    fn test_query_obligation_found_returns_reasons_and_surfaces() {
        let plan = make_core_change_plan();
        let result = query_obligation(&plan, "pkg:core:mutation-diff").unwrap();

        assert_eq!(result.obligation_id, "pkg:core:mutation-diff");
        assert_eq!(result.reasons.len(), 1);
        assert_eq!(result.reasons[0].source, "rule");
        assert_eq!(result.selected_surfaces, vec!["mutation.diff[pkg=core]"]);
        assert!(result.omitted_surfaces.is_empty());
    }

    #[test]
    fn test_query_obligation_profile_obligation() {
        let plan = make_core_change_plan();
        let result = query_obligation(&plan, "workspace:smoke").unwrap();

        assert_eq!(result.obligation_id, "workspace:smoke");
        assert_eq!(result.reasons.len(), 1);
        assert_eq!(result.reasons[0].source, "profile");
        assert_eq!(result.selected_surfaces, vec!["workspace.smoke"]);
    }

    #[test]
    fn test_query_obligation_not_found_returns_error() {
        let plan = make_core_change_plan();
        let result = query_obligation(&plan, "nonexistent:obligation");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("nonexistent:obligation"));
        assert!(err_msg.contains("not found"));
    }

    // ── query_surface unit tests ──

    #[test]
    fn test_query_surface_selected_returns_details() {
        let plan = make_core_change_plan();
        let result = query_surface(&plan, "tests.pkg[pkg=core]").unwrap();

        assert_eq!(result.surface_id, "tests.pkg[pkg=core]");
        assert_eq!(result.status, "selected");
        assert_eq!(result.template.as_deref(), Some("tests.pkg"));
        assert_eq!(result.cost, Some(3.0));
        assert_eq!(
            result.covers.as_deref(),
            Some(vec!["pkg:core:tests".to_string()].as_slice())
        );
        assert!(result.run.is_some());
        assert!(result.omission_reason.is_none());
    }

    #[test]
    fn test_query_surface_omitted_returns_reason() {
        let plan = make_core_change_plan();
        let result = query_surface(&plan, "workspace.all-tests").unwrap();

        assert_eq!(result.surface_id, "workspace.all-tests");
        assert_eq!(result.status, "omitted");
        assert_eq!(
            result.omission_reason.as_deref(),
            Some("not selected by optimal weighted cover")
        );
        assert!(result.template.is_none());
        assert!(result.cost.is_none());
        assert!(result.covers.is_none());
        assert!(result.run.is_none());
    }

    #[test]
    fn test_query_surface_not_found_returns_error() {
        let plan = make_core_change_plan();
        let result = query_surface(&plan, "nonexistent.surface");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("nonexistent.surface"));
        assert!(err_msg.contains("not found"));
    }

    // ── trace_plan unit tests ──

    #[test]
    fn test_trace_plan_core_change_paths() {
        let plan = make_core_change_plan();
        let trace = trace_plan(&plan);

        // One changed path
        assert_eq!(trace.paths.len(), 1);
        let pt = &trace.paths[0];
        assert_eq!(pt.path, "crates/core/src/lib.rs");
        assert_eq!(pt.status, "M");
        assert_eq!(pt.owner.as_deref(), Some("core"));
        assert_eq!(
            pt.obligations,
            vec!["pkg:core:mutation-diff", "pkg:core:tests"]
        );
        assert_eq!(
            pt.surfaces,
            vec!["mutation.diff[pkg=core]", "tests.pkg[pkg=core]"]
        );
        assert_eq!(pt.rule_matches.len(), 1);
        assert_eq!(pt.rule_matches[0].rule_index, 1);
    }

    #[test]
    fn test_trace_plan_profile_obligations() {
        let plan = make_core_change_plan();
        let trace = trace_plan(&plan);

        // workspace:smoke has source "profile"
        assert_eq!(trace.profile_obligations.len(), 1);
        assert_eq!(
            trace.profile_obligations[0].obligation_id,
            "workspace:smoke"
        );
        assert_eq!(trace.profile_obligations[0].source, "profile");
        assert_eq!(
            trace.profile_obligations[0].surfaces,
            vec!["workspace.smoke"]
        );
    }

    #[test]
    fn test_trace_plan_no_fallback_obligations() {
        let plan = make_core_change_plan();
        let trace = trace_plan(&plan);

        // No fallback obligations in the core-change plan
        assert!(trace.fallback_obligations.is_empty());
    }

    #[test]
    fn test_trace_plan_with_fallback_obligation() {
        let mut plan = make_core_change_plan();
        plan.obligations.push(ObligationRecord {
            id: "fallback:safety".to_string(),
            reasons: vec![ObligationReason {
                source: "unknown-fallback".to_string(),
                path: Some("unknown/file.txt".to_string()),
                rule: None,
                pattern: None,
            }],
        });
        plan.changed_paths.push(ChangedPath {
            path: "unknown/file.txt".to_string(),
            status: "A".to_string(),
            owner: None,
        });

        let trace = trace_plan(&plan);

        assert_eq!(trace.fallback_obligations.len(), 1);
        assert_eq!(
            trace.fallback_obligations[0].obligation_id,
            "fallback:safety"
        );
        assert_eq!(trace.fallback_obligations[0].source, "unknown-fallback");
    }

    #[test]
    fn test_trace_plan_empty_plan() {
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
            obligations: vec![],
            selected_surfaces: vec![],
            omitted_surfaces: vec![],
            diagnostics: vec![],
        };

        let trace = trace_plan(&plan);
        assert!(trace.paths.is_empty());
        assert!(trace.profile_obligations.is_empty());
        assert!(trace.fallback_obligations.is_empty());
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
    #[allow(clippy::too_many_arguments)]
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

    /// Build a Plan with proper cross-references between paths, obligations, and surfaces.
    /// Each obligation has reasons referencing one or more of the changed paths.
    /// Each surface covers one or more of the obligation ids.
    fn make_cross_referenced_plan(
        changed_paths: Vec<ChangedPath>,
        obligation_count: usize,
        surface_count: usize,
        // For each obligation, which changed-path indices it references (non-empty)
        obligation_path_indices: Vec<Vec<usize>>,
        // For each surface, which obligation indices it covers (non-empty)
        surface_obligation_indices: Vec<Vec<usize>>,
    ) -> Plan {
        let obligations: Vec<ObligationRecord> = (0..obligation_count)
            .map(|i| {
                let path_indices = &obligation_path_indices[i];
                let reasons = path_indices
                    .iter()
                    .map(|&pi| {
                        let cp = &changed_paths[pi];
                        ObligationReason {
                            source: "rule".to_string(),
                            path: Some(cp.path.clone()),
                            rule: Some(format!("rule:{}", i)),
                            pattern: Some(format!("pattern-{}", i)),
                        }
                    })
                    .collect();
                ObligationRecord {
                    id: format!("obl:{}", i),
                    reasons,
                }
            })
            .collect();

        let selected_surfaces: Vec<SelectedSurface> = (0..surface_count)
            .map(|i| {
                let obl_indices = &surface_obligation_indices[i];
                let covers = obl_indices
                    .iter()
                    .map(|&oi| obligations[oi].id.clone())
                    .collect();
                SelectedSurface {
                    id: format!("surf.{}", i),
                    template: format!("tmpl.{}", i),
                    cost: (i + 1) as f64,
                    covers,
                    run: vec!["echo".to_string(), format!("surface-{}", i)],
                }
            })
            .collect();

        make_arb_plan(
            "aaa".to_string(),
            "bbb".to_string(),
            "aaa".to_string(),
            "ci".to_string(),
            "digest".to_string(),
            changed_paths,
            obligations,
            selected_surfaces,
            vec![],
        )
    }

    /// Strategy that generates a cross-referenced Plan suitable for query_path testing.
    fn arb_cross_referenced_plan() -> impl Strategy<Value = Plan> {
        // 1-5 changed paths
        prop::collection::vec(arb_changed_path(), 1..=5).prop_flat_map(|paths| {
            let path_count = paths.len();
            // 1-3 obligations, each referencing at least one path
            (1..=3usize)
                .prop_flat_map(move |obl_count| {
                    let pc = path_count;
                    // For each obligation, pick 1+ path indices
                    prop::collection::vec(
                        prop::collection::vec(0..pc, 1..=pc.min(3)),
                        obl_count..=obl_count,
                    )
                    .prop_flat_map(move |obl_path_indices| {
                        let oc = obl_path_indices.len();
                        // 1-3 surfaces, each covering 1+ obligation indices
                        (1..=3usize)
                            .prop_flat_map(move |surf_count| {
                                let oc2 = oc;
                                prop::collection::vec(
                                    prop::collection::vec(0..oc2, 1..=oc2.min(3)),
                                    surf_count..=surf_count,
                                )
                                .prop_map(move |surf_obl_indices| surf_obl_indices)
                            })
                            .prop_map({
                                let obl_path_indices = obl_path_indices.clone();
                                move |surf_obl_indices| (obl_path_indices.clone(), surf_obl_indices)
                            })
                    })
                })
                .prop_map(move |(obl_path_indices, surf_obl_indices)| {
                    let obl_count = obl_path_indices.len();
                    let surf_count = surf_obl_indices.len();
                    make_cross_referenced_plan(
                        paths.clone(),
                        obl_count,
                        surf_count,
                        obl_path_indices,
                        surf_obl_indices,
                    )
                })
        })
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

    /// Strategy for an OmittedSurface with a unique id and reason.
    fn arb_omitted_surface(index: usize) -> impl Strategy<Value = OmittedSurface> {
        "[a-z ]{5,30}".prop_map(move |reason| OmittedSurface {
            id: format!("omitted.{}", index),
            reason,
        })
    }

    /// Strategy that generates a cross-referenced Plan with omitted surfaces,
    /// suitable for obligation and surface query testing.
    fn arb_cross_referenced_plan_with_omitted() -> impl Strategy<Value = Plan> {
        // 1-5 changed paths
        prop::collection::vec(arb_changed_path(), 1..=5).prop_flat_map(|paths| {
            let path_count = paths.len();
            // 1-3 obligations, each referencing at least one path
            (1..=3usize)
                .prop_flat_map(move |obl_count| {
                    let pc = path_count;
                    prop::collection::vec(
                        prop::collection::vec(0..pc, 1..=pc.min(3)),
                        obl_count..=obl_count,
                    )
                    .prop_flat_map(move |obl_path_indices| {
                        let oc = obl_path_indices.len();
                        // 1-3 surfaces, each covering 1+ obligation indices
                        (1..=3usize)
                            .prop_flat_map(move |surf_count| {
                                let oc2 = oc;
                                prop::collection::vec(
                                    prop::collection::vec(0..oc2, 1..=oc2.min(3)),
                                    surf_count..=surf_count,
                                )
                            })
                            .prop_flat_map(move |surf_obl_indices| {
                                // 0-2 omitted surfaces
                                let soi = surf_obl_indices.clone();
                                (0..=2usize).prop_flat_map(move |omit_count| {
                                    let soi2 = soi.clone();
                                    let strats: Vec<_> = (0..omit_count)
                                        .map(|i| arb_omitted_surface(i).boxed())
                                        .collect();
                                    strats
                                        .into_iter()
                                        .collect::<Vec<_>>()
                                        .prop_map(move |omitted| (soi2.clone(), omitted))
                                })
                            })
                            .prop_map({
                                let obl_path_indices = obl_path_indices.clone();
                                move |(surf_obl_indices, omitted)| {
                                    (obl_path_indices.clone(), surf_obl_indices, omitted)
                                }
                            })
                    })
                })
                .prop_map(
                    move |(obl_path_indices, surf_obl_indices, omitted_surfaces)| {
                        let obl_count = obl_path_indices.len();
                        let surf_count = surf_obl_indices.len();
                        let mut plan = make_cross_referenced_plan(
                            paths.clone(),
                            obl_count,
                            surf_count,
                            obl_path_indices,
                            surf_obl_indices,
                        );
                        plan.omitted_surfaces = omitted_surfaces;
                        plan
                    },
                )
        })
    }

    // Feature: proofrun-differentiation, Property 4: Path query correctness
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 5.1, 5.3, 5.4**
        ///
        /// For any Plan with cross-referenced paths, obligations, and surfaces:
        /// (a) query_path returns found=true for every changed path,
        /// (b) returned obligation ids are a subset of the plan's obligation ids,
        /// (c) returned surface ids are a subset of the plan's selected surface ids,
        /// (d) query_path returns found=false for a path not in changed_paths.
        #[test]
        fn prop_path_query_correctness(plan in arb_cross_referenced_plan()) {
            let all_obligation_ids: std::collections::HashSet<&str> = plan
                .obligations
                .iter()
                .map(|o| o.id.as_str())
                .collect();
            let all_surface_ids: std::collections::HashSet<&str> = plan
                .selected_surfaces
                .iter()
                .map(|s| s.id.as_str())
                .collect();

            // For each changed path, query_path must return found=true
            // with obligations ⊆ plan.obligations and surfaces ⊆ plan.selected_surfaces.
            for cp in &plan.changed_paths {
                let result = query_path(&plan, &cp.path);

                prop_assert!(
                    result.found,
                    "query_path('{}') must return found=true for a changed path",
                    cp.path
                );

                // Each obligation in the result must exist in the plan
                for obl_id in &result.obligations {
                    prop_assert!(
                        all_obligation_ids.contains(obl_id.as_str()),
                        "obligation '{}' from query_path('{}') must be in plan obligations {:?}",
                        obl_id, cp.path, all_obligation_ids
                    );
                }

                // Each obligation in rule_matches must exist in the plan
                for rm in &result.rule_matches {
                    for obl_id in &rm.obligations {
                        prop_assert!(
                            all_obligation_ids.contains(obl_id.as_str()),
                            "rule_match obligation '{}' from query_path('{}') must be in plan obligations",
                            obl_id, cp.path
                        );
                    }
                }

                // Each surface in the result must exist in the plan
                for surf_id in &result.surfaces {
                    prop_assert!(
                        all_surface_ids.contains(surf_id.as_str()),
                        "surface '{}' from query_path('{}') must be in plan selected_surfaces {:?}",
                        surf_id, cp.path, all_surface_ids
                    );
                }
            }

            // For a path NOT in changed_paths, query_path must return found=false.
            let absent_path = "zzz_absent_path_not_in_plan/file.xyz";
            let absent_result = query_path(&plan, absent_path);
            prop_assert!(
                !absent_result.found,
                "query_path('{}') must return found=false for absent path",
                absent_path
            );
            prop_assert!(
                absent_result.obligations.is_empty(),
                "query_path for absent path must have empty obligations"
            );
            prop_assert!(
                absent_result.surfaces.is_empty(),
                "query_path for absent path must have empty surfaces"
            );
            prop_assert!(
                absent_result.rule_matches.is_empty(),
                "query_path for absent path must have empty rule_matches"
            );
        }
    }

    // Feature: proofrun-differentiation, Property 5: Obligation query correctness
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 6.1, 6.2, 6.3**
        ///
        /// For any Plan with cross-referenced obligations and surfaces:
        /// (a) query_obligation returns Ok with matching reasons for every obligation in the plan,
        /// (b) selected_surfaces contains exactly those selected surface ids whose covers includes
        ///     the obligation id,
        /// (c) query_obligation returns Err for a non-existent obligation id.
        #[test]
        fn prop_obligation_query_correctness(plan in arb_cross_referenced_plan_with_omitted()) {
            for obligation in &plan.obligations {
                let result = query_obligation(&plan, &obligation.id);
                prop_assert!(
                    result.is_ok(),
                    "query_obligation('{}') must return Ok for an existing obligation",
                    obligation.id
                );
                let explanation = result.unwrap();

                // (a) reasons must match the plan's obligation reasons
                prop_assert_eq!(
                    explanation.reasons.len(),
                    obligation.reasons.len(),
                    "reasons count mismatch for obligation '{}'",
                    obligation.id
                );
                for (i, reason) in explanation.reasons.iter().enumerate() {
                    prop_assert_eq!(
                        &reason.source, &obligation.reasons[i].source,
                        "reason[{}].source mismatch for obligation '{}'", i, obligation.id
                    );
                    prop_assert_eq!(
                        &reason.path, &obligation.reasons[i].path,
                        "reason[{}].path mismatch for obligation '{}'", i, obligation.id
                    );
                    prop_assert_eq!(
                        &reason.rule, &obligation.reasons[i].rule,
                        "reason[{}].rule mismatch for obligation '{}'", i, obligation.id
                    );
                    prop_assert_eq!(
                        &reason.pattern, &obligation.reasons[i].pattern,
                        "reason[{}].pattern mismatch for obligation '{}'", i, obligation.id
                    );
                }

                // (b) selected_surfaces must be exactly those whose covers includes this obligation id
                let expected_selected: Vec<String> = plan
                    .selected_surfaces
                    .iter()
                    .filter(|s| s.covers.contains(&obligation.id))
                    .map(|s| s.id.clone())
                    .collect();
                prop_assert_eq!(
                    &explanation.selected_surfaces,
                    &expected_selected,
                    "selected_surfaces mismatch for obligation '{}'",
                    obligation.id
                );
            }

            // (c) query_obligation with a non-existent id must return Err
            let absent_id = "zzz_absent_obligation_not_in_plan";
            let absent_result = query_obligation(&plan, absent_id);
            prop_assert!(
                absent_result.is_err(),
                "query_obligation('{}') must return Err for absent obligation",
                absent_id
            );
        }
    }

    // Feature: proofrun-differentiation, Property 6: Surface query correctness
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 7.1, 7.2, 7.3**
        ///
        /// For any Plan with selected and omitted surfaces:
        /// (a) query_surface returns status="selected" with correct fields for each selected surface,
        /// (b) query_surface returns status="omitted" with correct omission_reason for each omitted surface,
        /// (c) query_surface returns Err for a non-existent surface id.
        #[test]
        fn prop_surface_query_correctness(plan in arb_cross_referenced_plan_with_omitted()) {
            // (a) Each selected surface must return correct details
            for surface in &plan.selected_surfaces {
                let result = query_surface(&plan, &surface.id);
                prop_assert!(
                    result.is_ok(),
                    "query_surface('{}') must return Ok for a selected surface",
                    surface.id
                );
                let explanation = result.unwrap();

                prop_assert_eq!(
                    &explanation.status, "selected",
                    "status must be 'selected' for surface '{}'", surface.id
                );
                prop_assert_eq!(
                    explanation.template.as_deref(), Some(surface.template.as_str()),
                    "template mismatch for surface '{}'", surface.id
                );
                prop_assert_eq!(
                    explanation.cost, Some(surface.cost),
                    "cost mismatch for surface '{}'", surface.id
                );
                prop_assert_eq!(
                    explanation.covers.as_ref(), Some(&surface.covers),
                    "covers mismatch for surface '{}'", surface.id
                );
                prop_assert_eq!(
                    explanation.run.as_ref(), Some(&surface.run),
                    "run mismatch for surface '{}'", surface.id
                );
                prop_assert!(
                    explanation.omission_reason.is_none(),
                    "omission_reason must be None for selected surface '{}'", surface.id
                );
            }

            // (b) Each omitted surface must return correct details
            for omitted in &plan.omitted_surfaces {
                let result = query_surface(&plan, &omitted.id);
                prop_assert!(
                    result.is_ok(),
                    "query_surface('{}') must return Ok for an omitted surface",
                    omitted.id
                );
                let explanation = result.unwrap();

                prop_assert_eq!(
                    &explanation.status, "omitted",
                    "status must be 'omitted' for surface '{}'", omitted.id
                );
                prop_assert_eq!(
                    explanation.omission_reason.as_deref(), Some(omitted.reason.as_str()),
                    "omission_reason mismatch for surface '{}'", omitted.id
                );
                prop_assert!(
                    explanation.template.is_none(),
                    "template must be None for omitted surface '{}'", omitted.id
                );
                prop_assert!(
                    explanation.cost.is_none(),
                    "cost must be None for omitted surface '{}'", omitted.id
                );
                prop_assert!(
                    explanation.covers.is_none(),
                    "covers must be None for omitted surface '{}'", omitted.id
                );
                prop_assert!(
                    explanation.run.is_none(),
                    "run must be None for omitted surface '{}'", omitted.id
                );
            }

            // (c) query_surface with a non-existent id must return Err
            let absent_id = "zzz_absent_surface_not_in_plan";
            let absent_result = query_surface(&plan, absent_id);
            prop_assert!(
                absent_result.is_err(),
                "query_surface('{}') must return Err for absent surface",
                absent_id
            );
        }
    }

    /// Strategy that generates a Plan with mixed obligation types:
    /// - 1-5 changed paths with rule-sourced obligations
    /// - 1-2 profile obligations (source="profile", no path)
    /// - 0-1 fallback obligations (source="unknown-fallback" or "empty-range-fallback", no path)
    /// - Selected surfaces that cover ALL obligations
    fn arb_trace_plan() -> impl Strategy<Value = Plan> {
        (
            // 1-5 changed paths
            prop::collection::vec(arb_changed_path(), 1..=5),
            // 1-2 profile obligation count
            1..=2usize,
            // 0-1 fallback obligation count
            0..=1usize,
            // fallback source variant
            prop::bool::ANY,
        )
            .prop_flat_map(
                |(paths, profile_count, fallback_count, use_unknown_fallback)| {
                    let path_count = paths.len();
                    // Generate 1-3 rule obligations, each referencing at least one path
                    let rule_obl_strat = prop::collection::vec(
                        prop::collection::vec(0..path_count, 1..=path_count.min(3)),
                        1..=3usize,
                    );
                    rule_obl_strat.prop_map(move |rule_path_indices| {
                        let mut obligations = Vec::new();
                        let mut all_obl_ids = Vec::new();

                        // Rule-sourced obligations (linked to changed paths)
                        for (i, path_indices) in rule_path_indices.iter().enumerate() {
                            let reasons = path_indices
                                .iter()
                                .map(|&pi| ObligationReason {
                                    source: "rule".to_string(),
                                    path: Some(paths[pi].path.clone()),
                                    rule: Some(format!("rule:{}", i)),
                                    pattern: Some(format!("pattern-{}", i)),
                                })
                                .collect();
                            let id = format!("rule:obl:{}", i);
                            all_obl_ids.push(id.clone());
                            obligations.push(ObligationRecord { id, reasons });
                        }

                        // Profile obligations (no path)
                        for i in 0..profile_count {
                            let id = format!("profile:obl:{}", i);
                            all_obl_ids.push(id.clone());
                            obligations.push(ObligationRecord {
                                id,
                                reasons: vec![ObligationReason {
                                    source: "profile".to_string(),
                                    path: None,
                                    rule: Some("ci".to_string()),
                                    pattern: None,
                                }],
                            });
                        }

                        // Fallback obligations (no path)
                        for i in 0..fallback_count {
                            let source = if use_unknown_fallback {
                                "unknown-fallback"
                            } else {
                                "empty-range-fallback"
                            };
                            let id = format!("fallback:obl:{}", i);
                            all_obl_ids.push(id.clone());
                            obligations.push(ObligationRecord {
                                id,
                                reasons: vec![ObligationReason {
                                    source: source.to_string(),
                                    path: None,
                                    rule: None,
                                    pattern: None,
                                }],
                            });
                        }

                        // Build surfaces that cover ALL obligations.
                        // Surface 0 covers all rule obligations,
                        // Surface 1 covers all profile obligations,
                        // Surface 2 covers all fallback obligations (if any).
                        let rule_obl_ids: Vec<String> = all_obl_ids
                            .iter()
                            .filter(|id| id.starts_with("rule:"))
                            .cloned()
                            .collect();
                        let profile_obl_ids: Vec<String> = all_obl_ids
                            .iter()
                            .filter(|id| id.starts_with("profile:"))
                            .cloned()
                            .collect();
                        let fallback_obl_ids: Vec<String> = all_obl_ids
                            .iter()
                            .filter(|id| id.starts_with("fallback:"))
                            .cloned()
                            .collect();

                        let mut surfaces = Vec::new();
                        if !rule_obl_ids.is_empty() {
                            surfaces.push(SelectedSurface {
                                id: "surf.rule".to_string(),
                                template: "tmpl.rule".to_string(),
                                cost: 5.0,
                                covers: rule_obl_ids,
                                run: vec!["echo".to_string(), "rule-surface".to_string()],
                            });
                        }
                        if !profile_obl_ids.is_empty() {
                            surfaces.push(SelectedSurface {
                                id: "surf.profile".to_string(),
                                template: "tmpl.profile".to_string(),
                                cost: 2.0,
                                covers: profile_obl_ids,
                                run: vec!["echo".to_string(), "profile-surface".to_string()],
                            });
                        }
                        if !fallback_obl_ids.is_empty() {
                            surfaces.push(SelectedSurface {
                                id: "surf.fallback".to_string(),
                                template: "tmpl.fallback".to_string(),
                                cost: 1.0,
                                covers: fallback_obl_ids,
                                run: vec!["echo".to_string(), "fallback-surface".to_string()],
                            });
                        }

                        make_arb_plan(
                            "aaa".to_string(),
                            "bbb".to_string(),
                            "aaa".to_string(),
                            "ci".to_string(),
                            "digest".to_string(),
                            paths.clone(),
                            obligations,
                            surfaces,
                            vec![],
                        )
                    })
                },
            )
    }

    // Feature: proofrun-differentiation, Property 7: Trace completeness
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 8.1, 8.2, 8.3**
        ///
        /// For any Plan with rule, profile, and fallback obligations:
        /// (a) every changed path appears in trace.paths,
        /// (b) every profile obligation appears in trace.profile_obligations,
        /// (c) every fallback obligation appears in trace.fallback_obligations,
        /// (d) every selected surface id appears in at least one path's surfaces list
        ///     or in a profile/fallback obligation's surfaces list.
        #[test]
        fn prop_trace_completeness(plan in arb_trace_plan()) {
            let trace = trace_plan(&plan);

            // (a) Every changed path appears in trace.paths
            let trace_path_set: std::collections::HashSet<&str> = trace
                .paths
                .iter()
                .map(|pt| pt.path.as_str())
                .collect();
            for cp in &plan.changed_paths {
                prop_assert!(
                    trace_path_set.contains(cp.path.as_str()),
                    "changed path '{}' must appear in trace.paths",
                    cp.path
                );
            }

            // (b) Every profile obligation appears in trace.profile_obligations
            let trace_profile_ids: std::collections::HashSet<&str> = trace
                .profile_obligations
                .iter()
                .map(|po| po.obligation_id.as_str())
                .collect();
            for obl in &plan.obligations {
                if obl.reasons.iter().any(|r| r.source == "profile") {
                    prop_assert!(
                        trace_profile_ids.contains(obl.id.as_str()),
                        "profile obligation '{}' must appear in trace.profile_obligations",
                        obl.id
                    );
                }
            }

            // (c) Every fallback obligation appears in trace.fallback_obligations
            let trace_fallback_ids: std::collections::HashSet<&str> = trace
                .fallback_obligations
                .iter()
                .map(|fo| fo.obligation_id.as_str())
                .collect();
            for obl in &plan.obligations {
                if obl.reasons.iter().any(|r| {
                    r.source == "unknown-fallback" || r.source == "empty-range-fallback"
                }) {
                    prop_assert!(
                        trace_fallback_ids.contains(obl.id.as_str()),
                        "fallback obligation '{}' must appear in trace.fallback_obligations",
                        obl.id
                    );
                }
            }

            // (d) Every selected surface id appears in at least one trace entry's surfaces list
            let mut all_traced_surface_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for pt in &trace.paths {
                for sid in &pt.surfaces {
                    all_traced_surface_ids.insert(sid.clone());
                }
            }
            for po in &trace.profile_obligations {
                for sid in &po.surfaces {
                    all_traced_surface_ids.insert(sid.clone());
                }
            }
            for fo in &trace.fallback_obligations {
                for sid in &fo.surfaces {
                    all_traced_surface_ids.insert(sid.clone());
                }
            }
            for surface in &plan.selected_surfaces {
                prop_assert!(
                    all_traced_surface_ids.contains(&surface.id),
                    "selected surface '{}' must appear in at least one trace entry's surfaces list",
                    surface.id
                );
            }
        }
    }
}
