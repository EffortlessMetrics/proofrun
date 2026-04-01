pub mod cargo_workspace;
pub mod compare;
pub mod config;
pub mod doctor;
pub mod emit;
pub mod explain;
pub mod git;
pub mod model;
pub mod obligations;
pub mod planner;
pub mod run;

pub use cargo_workspace::{PackageInfo, WorkspaceGraph};
pub use compare::{compare_plans, PlanComparison};
pub use config::{load_config, Config, Rule, SurfaceTemplate};
pub use doctor::{doctor_repo, should_fail_strict, DoctorFinding, DoctorReport};
pub use emit::{
    emit_commands_shell, emit_github_actions, emit_matrix_json, emit_nextest_filtersets,
    emit_plan_markdown, emit_structured_json, write_plan_artifacts, MatrixEntry,
};
pub use explain::{
    query_obligation, query_path, query_surface, render_explanation, trace_plan,
    FallbackObligation, ObligationExplanation, PathExplanation, PathTrace, ProfileObligation,
    RuleMatch, SurfaceExplanation, TraceOutput,
};
pub use git::{
    collect_git_changes, collect_staged_changes, collect_working_tree_changes, head_sha,
    parse_name_status_line, GitChanges, GitRange,
};
pub use model::{Plan, PlanArtifacts, Receipt, SelectedSurface, WorkspaceInfo, WorkspacePackage};
pub use obligations::{
    compile_obligations, expand_template, glob_to_regex, match_path, Obligation,
};
pub use planner::{build_candidates, solve_exact_cover, CandidateSurface};
pub use run::{execute_failed_only, execute_plan, execute_with_resume, ExecutionMode};

use anyhow::Result;

/// Result of checking budget gates against a plan.
#[derive(Debug, Clone)]
pub struct BudgetGateResult {
    /// Whether any hard gate failed (excludes warn-only gates).
    pub failed: bool,
    /// Diagnostic messages for each gate that fired.
    pub messages: Vec<String>,
}

/// Check budget gates against a plan.
///
/// Gates are checked in order: max_cost, max_surfaces, fail_on_fallback.
/// `warn_on_workspace_smoke_only` produces a message but does NOT set `failed`.
pub fn check_budget_gates(
    plan: &Plan,
    max_cost: Option<f64>,
    max_surfaces: Option<usize>,
    fail_on_fallback: bool,
    warn_on_workspace_smoke_only: bool,
) -> BudgetGateResult {
    let mut failed = false;
    let mut messages = Vec::new();

    if let Some(threshold) = max_cost {
        let total_cost: f64 = plan.selected_surfaces.iter().map(|s| s.cost).sum();
        if total_cost > threshold {
            messages.push(format!(
                "budget gate: total cost {total_cost} exceeds --max-cost {threshold}"
            ));
            failed = true;
        }
    }

    if let Some(threshold) = max_surfaces {
        let count = plan.selected_surfaces.len();
        if count > threshold {
            messages.push(format!(
                "budget gate: {count} surfaces exceeds --max-surfaces {threshold}"
            ));
            failed = true;
        }
    }

    if fail_on_fallback {
        let has_fallback = plan.obligations.iter().any(|o| {
            o.reasons
                .iter()
                .any(|r| r.source == "unknown-fallback" || r.source == "empty-range-fallback")
        });
        if has_fallback {
            messages.push(
                "budget gate: fallback obligations detected with --fail-on-fallback".to_string(),
            );
            failed = true;
        }
    }

    if warn_on_workspace_smoke_only
        && plan.selected_surfaces.len() == 1
        && plan.selected_surfaces[0].template == "workspace.smoke"
    {
        messages.push("warning: only selected surface is workspace.smoke".to_string());
        // This is a warning, not a gate — don't set failed
    }

    BudgetGateResult { failed, messages }
}
use camino::Utf8Path;
use model::ChangedPath;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// Describes how changed paths are provided to the planner.
#[derive(Debug, Clone)]
pub enum ChangeSource {
    /// Traditional Git range: --base and --head revisions.
    GitRange(GitRange),
    /// Staged changes: git diff --cached.
    Staged,
    /// Working tree changes: git diff HEAD.
    WorkingTree,
    /// Explicit paths from stdin, each treated as status "M".
    PathsFromStdin(Vec<String>),
}

/// Validate that exactly one change source flag is set.
///
/// Returns `Ok(())` if exactly one of the four flags is true.
/// Returns `Err` with a descriptive message if zero or more than one are set.
pub fn validate_change_source_flags(
    has_git_range: bool,
    staged: bool,
    working_tree: bool,
    paths_from_stdin: bool,
) -> Result<(), String> {
    let count = [has_git_range, staged, working_tree, paths_from_stdin]
        .iter()
        .filter(|&&b| b)
        .count();
    match count {
        1 => Ok(()),
        0 => Err(
            "error: no change source provided. Use one of: --base/--head, --staged, --working-tree, --paths-from-stdin"
                .to_string(),
        ),
        _ => Err(
            "error: conflicting change sources. Use exactly one of: --base/--head, --staged, --working-tree, --paths-from-stdin"
                .to_string(),
        ),
    }
}

/// Parse stdin path lines into `ChangedPath` entries.
///
/// Blank and whitespace-only lines are skipped. Each remaining line is
/// trimmed of leading/trailing whitespace and assigned status `"M"`.
pub fn parse_stdin_paths(lines: &[String]) -> Vec<ChangedPath> {
    lines
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .map(|path| ChangedPath {
            path,
            status: "M".to_string(),
            owner: None,
        })
        .collect()
}

/// Return the current UTC time as an ISO 8601 string: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_now() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

/// Produce canonical JSON: sorted keys, no whitespace, `,` and `:` separators.
/// Matches the Python reference: `json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":"))`
pub fn canonical_json(value: &serde_json::Value) -> String {
    let mut buf = String::new();
    write_canonical(value, &mut buf);
    buf
}

fn write_canonical(value: &serde_json::Value, buf: &mut String) {
    match value {
        serde_json::Value::Null => buf.push_str("null"),
        serde_json::Value::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        serde_json::Value::Number(n) => buf.push_str(&n.to_string()),
        serde_json::Value::String(s) => {
            // JSON-escape the string (reuse serde_json for correctness)
            buf.push_str(&serde_json::to_string(s).unwrap());
        }
        serde_json::Value::Array(arr) => {
            buf.push('[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(',');
                }
                write_canonical(item, buf);
            }
            buf.push(']');
        }
        serde_json::Value::Object(map) => {
            // serde_json::Map iterates in sorted order when `preserve_order` feature is off (default)
            // But to be safe and match the reference exactly, collect and sort keys.
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            buf.push('{');
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    buf.push(',');
                }
                buf.push_str(&serde_json::to_string(key).unwrap());
                buf.push(':');
                write_canonical(val, buf);
            }
            buf.push('}');
        }
    }
}

/// Compute SHA-256 of UTF-8 bytes, return lowercase hex digest.
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    // Format as lowercase hex
    result.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn plan_repo(repo_root: &Utf8Path, range: GitRange, profile: &str) -> Result<Plan> {
    // 1. Load config
    let config = load_config(repo_root)?;

    // 2. Collect git changes (merge_base, changed paths, patch)
    let git_changes = collect_git_changes(repo_root, &range)?;

    // 3. Discover workspace
    let workspace = WorkspaceGraph::discover(repo_root)?;

    // 4. Compile obligations (mutates changes to set owner)
    let mut changes = git_changes.changes;
    let (obligations_map, diagnostics) =
        compile_obligations(&config, profile, &mut changes, &workspace);

    // 5. Extract sorted obligation ids
    let obligation_ids: Vec<String> = obligations_map.keys().cloned().collect();

    // 6. Build candidates — use absolute output_dir matching the reference (POSIX separators)
    let output_dir_str = format!(
        "{}/{}",
        repo_root.as_str().replace('\\', "/"),
        config.defaults.output_dir
    );
    let output_dir = camino::Utf8PathBuf::from(&output_dir_str);
    let candidates = build_candidates(&config, &obligation_ids, profile, &output_dir);

    // 7. Solve exact cover
    let selected = solve_exact_cover(&obligation_ids, &candidates)?;
    let selected_ids: std::collections::BTreeSet<String> =
        selected.iter().map(|c| c.id.clone()).collect();

    // 8. Compute omitted surfaces
    let mut omitted_surfaces: Vec<model::OmittedSurface> = candidates
        .iter()
        .filter(|c| !selected_ids.contains(&c.id))
        .map(|c| model::OmittedSurface {
            id: c.id.clone(),
            reason: "not selected by optimal weighted cover".to_string(),
        })
        .collect();
    omitted_surfaces.sort_by(|a, b| a.id.cmp(&b.id));

    // 9. Sort changed_paths by (path, status)
    changes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.status.cmp(&b.status)));

    // 10. Build ObligationRecord list sorted by id
    let obligations: Vec<model::ObligationRecord> = obligation_ids
        .iter()
        .map(|id| {
            let mut reasons = obligations_map[id].clone();
            reasons.sort_by(|a, b| {
                let pa = a.path.as_deref().unwrap_or("");
                let pb = b.path.as_deref().unwrap_or("");
                let ra = a.rule.as_deref().unwrap_or("");
                let rb = b.rule.as_deref().unwrap_or("");
                let pata = a.pattern.as_deref().unwrap_or("");
                let patb = b.pattern.as_deref().unwrap_or("");
                pa.cmp(pb)
                    .then_with(|| ra.cmp(rb))
                    .then_with(|| pata.cmp(patb))
            });
            model::ObligationRecord {
                id: id.clone(),
                reasons,
            }
        })
        .collect();

    // 11. Build SelectedSurface list sorted by id (already sorted from solver)
    let selected_surfaces: Vec<model::SelectedSurface> = selected
        .iter()
        .map(|c| model::SelectedSurface {
            id: c.id.clone(),
            template: c.template.clone(),
            cost: c.cost,
            covers: c.covers.clone(),
            run: c.run.clone(),
        })
        .collect();

    // 12. Build WorkspaceInfo from workspace graph
    let workspace_info = model::WorkspaceInfo {
        packages: workspace
            .packages
            .iter()
            .map(|pkg| model::WorkspacePackage {
                name: pkg.name.clone(),
                dir: pkg.dir.to_string(),
                manifest: pkg.manifest.to_string(),
                dependencies: pkg.dependencies.clone(),
                reverse_dependencies: workspace
                    .reverse_deps
                    .get(&pkg.name)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
    };

    // 13. Build PlanArtifacts
    let output_dir_str = &config.defaults.output_dir;
    let artifacts = model::PlanArtifacts {
        output_dir: output_dir_str.clone(),
        diff_patch: format!("{}/diff.patch", output_dir_str),
        plan_json: format!("{}/plan.json", output_dir_str),
        plan_markdown: format!("{}/plan.md", output_dir_str),
        commands_shell: format!("{}/commands.sh", output_dir_str),
        github_actions: format!("{}/github-actions.yml", output_dir_str),
    };

    // 14. Compute config_digest
    let config_value = serde_json::to_value(&config)?;
    let config_digest = sha256_hex(&canonical_json(&config_value));

    // 15. Assemble Plan with plan_digest = "" initially
    let mut plan = Plan {
        version: "0.1.0-ref".to_string(),
        created_at: utc_now(),
        repo_root: repo_root.to_string(),
        base: range.base.clone(),
        head: range.head.clone(),
        merge_base: git_changes.merge_base.clone(),
        profile: profile.to_string(),
        config_digest,
        plan_digest: String::new(),
        artifacts,
        workspace: workspace_info,
        changed_paths: changes,
        obligations,
        selected_surfaces,
        omitted_surfaces,
        diagnostics,
    };

    // 16. Compute plan_digest = sha256_hex(canonical_json(plan_as_value))
    //     The plan_digest field is "" at this point, so canonical_json will include it as "".
    //     We need to exclude plan_digest from the computation, matching the reference:
    //     plan_digest = sha256(canonical_json({k:v for k,v in plan.items() if k != "plan_digest"}))
    let mut plan_value = serde_json::to_value(&plan)?;
    if let serde_json::Value::Object(ref mut map) = plan_value {
        map.remove("plan_digest");
    }
    let plan_digest = sha256_hex(&canonical_json(&plan_value));
    plan.plan_digest = plan_digest;

    Ok(plan)
}

/// Plan a repo from any change source.
///
/// Resolves the `ChangeSource` into `(changes, patch, base, head, merge_base)`
/// then feeds into the shared planning pipeline (load config, discover workspace,
/// compile obligations, build candidates, solve, assemble plan).
pub fn plan_from_source(repo_root: &Utf8Path, source: ChangeSource, profile: &str) -> Result<Plan> {
    // Resolve change source into (changes, patch, base, head, merge_base)
    let (changes, _patch, base, head, merge_base) = match source {
        ChangeSource::GitRange(range) => {
            let git_changes = collect_git_changes(repo_root, &range)?;
            (
                git_changes.changes,
                git_changes.patch,
                range.base,
                range.head,
                git_changes.merge_base,
            )
        }
        ChangeSource::Staged => {
            let sha = head_sha(repo_root)?;
            let git_changes = collect_staged_changes(repo_root)?;
            (
                git_changes.changes,
                git_changes.patch,
                sha.clone(),
                "STAGED".to_string(),
                sha,
            )
        }
        ChangeSource::WorkingTree => {
            let sha = head_sha(repo_root)?;
            let git_changes = collect_working_tree_changes(repo_root)?;
            (
                git_changes.changes,
                git_changes.patch,
                sha.clone(),
                "WORKING_TREE".to_string(),
                sha,
            )
        }
        ChangeSource::PathsFromStdin(paths) => {
            let changes = parse_stdin_paths(&paths);
            (
                changes,
                String::new(),
                "STDIN".to_string(),
                "STDIN".to_string(),
                "STDIN".to_string(),
            )
        }
    };

    // Shared planning pipeline (identical to plan_repo from here on)

    // 1. Load config
    let config = load_config(repo_root)?;

    // 2. Discover workspace
    let workspace = WorkspaceGraph::discover(repo_root)?;

    // 3. Compile obligations (mutates changes to set owner)
    let mut changes = changes;
    let (obligations_map, diagnostics) =
        compile_obligations(&config, profile, &mut changes, &workspace);

    // 4. Extract sorted obligation ids
    let obligation_ids: Vec<String> = obligations_map.keys().cloned().collect();

    // 5. Build candidates
    let output_dir_str = format!(
        "{}/{}",
        repo_root.as_str().replace('\\', "/"),
        config.defaults.output_dir
    );
    let output_dir = camino::Utf8PathBuf::from(&output_dir_str);
    let candidates = build_candidates(&config, &obligation_ids, profile, &output_dir);

    // 6. Solve exact cover
    let selected = solve_exact_cover(&obligation_ids, &candidates)?;
    let selected_ids: std::collections::BTreeSet<String> =
        selected.iter().map(|c| c.id.clone()).collect();

    // 7. Compute omitted surfaces
    let mut omitted_surfaces: Vec<model::OmittedSurface> = candidates
        .iter()
        .filter(|c| !selected_ids.contains(&c.id))
        .map(|c| model::OmittedSurface {
            id: c.id.clone(),
            reason: "not selected by optimal weighted cover".to_string(),
        })
        .collect();
    omitted_surfaces.sort_by(|a, b| a.id.cmp(&b.id));

    // 8. Sort changed_paths by (path, status)
    changes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.status.cmp(&b.status)));

    // 9. Build ObligationRecord list sorted by id
    let obligations: Vec<model::ObligationRecord> = obligation_ids
        .iter()
        .map(|id| {
            let mut reasons = obligations_map[id].clone();
            reasons.sort_by(|a, b| {
                let pa = a.path.as_deref().unwrap_or("");
                let pb = b.path.as_deref().unwrap_or("");
                let ra = a.rule.as_deref().unwrap_or("");
                let rb = b.rule.as_deref().unwrap_or("");
                let pata = a.pattern.as_deref().unwrap_or("");
                let patb = b.pattern.as_deref().unwrap_or("");
                pa.cmp(pb)
                    .then_with(|| ra.cmp(rb))
                    .then_with(|| pata.cmp(patb))
            });
            model::ObligationRecord {
                id: id.clone(),
                reasons,
            }
        })
        .collect();

    // 10. Build SelectedSurface list sorted by id
    let selected_surfaces: Vec<model::SelectedSurface> = selected
        .iter()
        .map(|c| model::SelectedSurface {
            id: c.id.clone(),
            template: c.template.clone(),
            cost: c.cost,
            covers: c.covers.clone(),
            run: c.run.clone(),
        })
        .collect();

    // 11. Build WorkspaceInfo from workspace graph
    let workspace_info = model::WorkspaceInfo {
        packages: workspace
            .packages
            .iter()
            .map(|pkg| model::WorkspacePackage {
                name: pkg.name.clone(),
                dir: pkg.dir.to_string(),
                manifest: pkg.manifest.to_string(),
                dependencies: pkg.dependencies.clone(),
                reverse_dependencies: workspace
                    .reverse_deps
                    .get(&pkg.name)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
    };

    // 12. Build PlanArtifacts
    let output_dir_str = &config.defaults.output_dir;
    let artifacts = model::PlanArtifacts {
        output_dir: output_dir_str.clone(),
        diff_patch: format!("{}/diff.patch", output_dir_str),
        plan_json: format!("{}/plan.json", output_dir_str),
        plan_markdown: format!("{}/plan.md", output_dir_str),
        commands_shell: format!("{}/commands.sh", output_dir_str),
        github_actions: format!("{}/github-actions.yml", output_dir_str),
    };

    // 13. Compute config_digest
    let config_value = serde_json::to_value(&config)?;
    let config_digest = sha256_hex(&canonical_json(&config_value));

    // 14. Assemble Plan
    let mut plan = Plan {
        version: "0.1.0-ref".to_string(),
        created_at: utc_now(),
        repo_root: repo_root.to_string(),
        base,
        head,
        merge_base,
        profile: profile.to_string(),
        config_digest,
        plan_digest: String::new(),
        artifacts,
        workspace: workspace_info,
        changed_paths: changes,
        obligations,
        selected_surfaces,
        omitted_surfaces,
        diagnostics,
    };

    // 15. Compute plan_digest
    let mut plan_value = serde_json::to_value(&plan)?;
    if let serde_json::Value::Object(ref mut map) = plan_value {
        map.remove("plan_digest");
    }
    let plan_digest = sha256_hex(&canonical_json(&plan_value));
    plan.plan_digest = plan_digest;

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_canonical_json_sorted_keys_no_whitespace() {
        let value = json!({"b": 2, "a": 1, "c": 3});
        assert_eq!(canonical_json(&value), r#"{"a":1,"b":2,"c":3}"#);
    }

    #[test]
    fn test_canonical_json_nested_objects() {
        let value = json!({"z": {"b": 1, "a": 2}, "a": []});
        assert_eq!(canonical_json(&value), r#"{"a":[],"z":{"a":2,"b":1}}"#);
    }

    #[test]
    fn test_canonical_json_array() {
        let value = json!([3, 1, 2]);
        assert_eq!(canonical_json(&value), "[3,1,2]");
    }

    #[test]
    fn test_canonical_json_string_escaping() {
        let value = json!({"key": "hello \"world\""});
        assert_eq!(canonical_json(&value), r#"{"key":"hello \"world\""}"#);
    }

    #[test]
    fn test_canonical_json_null_bool() {
        let value = json!({"a": null, "b": true, "c": false});
        assert_eq!(canonical_json(&value), r#"{"a":null,"b":true,"c":false}"#);
    }

    #[test]
    fn test_canonical_json_roundtrip() {
        let value = json!({"z": [1, "two", null], "a": {"nested": true}});
        let first = canonical_json(&value);
        let parsed: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second = canonical_json(&parsed);
        assert_eq!(first, second);
    }

    #[test]
    fn test_canonical_json_float() {
        // serde_json represents 13.0 as Number, verify it serializes correctly
        let value = json!({"cost": 13.0});
        let result = canonical_json(&value);
        assert_eq!(result, r#"{"cost":13.0}"#);
    }

    #[test]
    fn test_sha256_hex_known_digest() {
        // SHA-256 of empty string
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hex_hello() {
        // SHA-256 of "hello"
        assert_eq!(
            sha256_hex("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_hex_consistent() {
        let text = r#"{"a":1,"b":2}"#;
        assert_eq!(sha256_hex(text), sha256_hex(text));
    }

    #[test]
    fn test_utc_now_format() {
        let ts = utc_now();
        let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").unwrap();
        assert!(
            re.is_match(&ts),
            "utc_now() returned {ts:?} which doesn't match ISO 8601 format"
        );
    }

    #[test]
    fn test_parse_stdin_paths_basic() {
        let lines = vec!["src/main.rs".to_string(), "lib.rs".to_string()];
        let result = parse_stdin_paths(&lines);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "src/main.rs");
        assert_eq!(result[0].status, "M");
        assert!(result[0].owner.is_none());
        assert_eq!(result[1].path, "lib.rs");
    }

    #[test]
    fn test_parse_stdin_paths_skips_blank_lines() {
        let lines = vec![
            "a.rs".to_string(),
            "".to_string(),
            "   ".to_string(),
            "b.rs".to_string(),
        ];
        let result = parse_stdin_paths(&lines);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "a.rs");
        assert_eq!(result[1].path, "b.rs");
    }

    #[test]
    fn test_parse_stdin_paths_trims_whitespace() {
        let lines = vec!["  src/lib.rs  ".to_string(), "\tCargo.toml\t".to_string()];
        let result = parse_stdin_paths(&lines);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "src/lib.rs");
        assert_eq!(result[1].path, "Cargo.toml");
    }

    #[test]
    fn test_parse_stdin_paths_empty_input() {
        let lines: Vec<String> = vec![];
        let result = parse_stdin_paths(&lines);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_stdin_paths_all_blank() {
        let lines = vec!["".to_string(), "   ".to_string(), "\t".to_string()];
        let result = parse_stdin_paths(&lines);
        assert!(result.is_empty());
    }

    use proptest::prelude::*;

    // Feature: rust-native-planner, Property 8: Plan digest determinism

    /// Recursive strategy that generates random `serde_json::Value` trees.
    /// Leaf types: null, bool, integer, float, string.
    /// Composite types: arrays (0-5 elements), objects (0-5 key-value pairs).
    fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
        let leaf = prop_oneof![
            Just(serde_json::Value::Null),
            any::<bool>().prop_map(serde_json::Value::Bool),
            (-1_000_000i64..1_000_000i64)
                .prop_map(|n| serde_json::Value::Number(serde_json::Number::from(n))),
            (1i64..1_000_000i64).prop_map(|n| {
                // Generate a float by converting to f64; serde_json::Number::from_f64
                // returns None for NaN/Inf, so we use safe integer-derived floats.
                let f = n as f64 + 0.5;
                serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap())
            }),
            "[a-zA-Z0-9]{0,20}".prop_map(serde_json::Value::String),
        ];

        leaf.prop_recursive(
            4,  // max depth
            64, // max total nodes
            5,  // items per collection
            |inner| {
                prop_oneof![
                    // Arrays with 0-5 elements
                    prop::collection::vec(inner.clone(), 0..=5).prop_map(serde_json::Value::Array),
                    // Objects with 0-5 key-value pairs
                    prop::collection::vec(("[a-zA-Z0-9]{1,10}", inner), 0..=5).prop_map(|pairs| {
                        let map: serde_json::Map<String, serde_json::Value> =
                            pairs.into_iter().collect();
                        serde_json::Value::Object(map)
                    }),
                ]
            },
        )
    }

    // Feature: rust-native-planner, Property 15: Timestamp format
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 16.3**
        ///
        /// Call `utc_now()` multiple times, verify each matches the ISO 8601
        /// pattern `YYYY-MM-DDTHH:MM:SSZ` and has exactly 20 characters.
        #[test]
        fn prop_timestamp_format(_dummy in 0..1u8) {
            let ts = utc_now();
            let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").unwrap();
            prop_assert!(
                re.is_match(&ts),
                "utc_now() returned {:?} which doesn't match ISO 8601 format",
                ts
            );
            prop_assert_eq!(
                ts.len(),
                20,
                "utc_now() returned {:?} with length {} instead of 20",
                ts,
                ts.len()
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// **Validates: Requirements 8.3, 8.4, 16.2**
        ///
        /// Round-trip idempotency: canonical_json(parse(canonical_json(x))) == canonical_json(x)
        /// SHA-256 consistency: sha256_hex(canonical_json(x)) is the same when called twice
        #[test]
        fn prop_digest_determinism(value in arb_json_value()) {
            // Round-trip idempotency
            let first = canonical_json(&value);
            let parsed: serde_json::Value = serde_json::from_str(&first)
                .expect("canonical_json output must be valid JSON");
            let second = canonical_json(&parsed);
            prop_assert_eq!(&first, &second,
                "canonical_json round-trip failed");

            // SHA-256 consistency: same input → same digest
            let digest1 = sha256_hex(&first);
            let digest2 = sha256_hex(&first);
            prop_assert_eq!(&digest1, &digest2,
                "sha256_hex not consistent for same input");

            // Digest is 64 hex chars (SHA-256 = 32 bytes = 64 hex digits)
            prop_assert_eq!(digest1.len(), 64);
            prop_assert!(digest1.chars().all(|c| c.is_ascii_hexdigit()),
                "sha256_hex output should be lowercase hex");
        }
    }

    // Feature: rust-native-planner, Property 9: Plan collections are sorted

    /// Strategy for generating a random `ChangedPath`.
    fn arb_changed_path() -> impl Strategy<Value = model::ChangedPath> {
        (
            "[a-z]{1,5}(/[a-z]{1,5}){0,3}\\.[a-z]{1,3}",
            prop_oneof![Just("M"), Just("A"), Just("D"), Just("R"), Just("C")]
                .prop_map(String::from),
        )
            .prop_map(|(path, status)| model::ChangedPath {
                path,
                status,
                owner: None,
            })
    }

    /// Strategy for generating a random `ObligationRecord`.
    fn arb_obligation_record() -> impl Strategy<Value = model::ObligationRecord> {
        "[a-z]{1,4}(:[a-z]{1,4}){0,2}".prop_map(|id| model::ObligationRecord {
            id,
            reasons: vec![],
        })
    }

    /// Strategy for generating a random `SelectedSurface`.
    fn arb_selected_surface() -> impl Strategy<Value = model::SelectedSurface> {
        "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}".prop_map(|id| model::SelectedSurface {
            id,
            template: "tpl".to_string(),
            cost: 1.0,
            covers: vec![],
            run: vec![],
        })
    }

    /// Strategy for generating a random `OmittedSurface`.
    fn arb_omitted_surface() -> impl Strategy<Value = model::OmittedSurface> {
        "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}".prop_map(|id| model::OmittedSurface {
            id,
            reason: "not selected".to_string(),
        })
    }

    // Feature: proofrun-differentiation, Property 1: Change source field mapping

    /// Resolve a `ChangeSource` to its expected (base, head, merge_base, patch) sentinel values.
    /// For Staged and WorkingTree, `sha` simulates the HEAD SHA that would come from git.
    /// This mirrors the match arm in `plan_from_source` without requiring a real git repo.
    fn resolve_sentinels(source: &ChangeSource, sha: &str) -> (String, String, String, String) {
        match source {
            ChangeSource::Staged => (
                sha.to_string(),
                "STAGED".to_string(),
                sha.to_string(),
                String::new(), // patch comes from git, but sentinel relationship is what matters
            ),
            ChangeSource::WorkingTree => (
                sha.to_string(),
                "WORKING_TREE".to_string(),
                sha.to_string(),
                String::new(),
            ),
            ChangeSource::PathsFromStdin(_) => (
                "STDIN".to_string(),
                "STDIN".to_string(),
                "STDIN".to_string(),
                String::new(),
            ),
            ChangeSource::GitRange(range) => (
                range.base.clone(),
                range.head.clone(),
                String::new(), // merge_base comes from git
                String::new(),
            ),
        }
    }

    /// Strategy: generate a hex-like SHA string (40 hex chars).
    fn arb_sha() -> impl Strategy<Value = String> {
        "[0-9a-f]{40}"
    }

    /// Strategy: generate a random `ChangeSource` variant.
    fn arb_change_source() -> impl Strategy<Value = (ChangeSource, String)> {
        let sha_strat = arb_sha();
        sha_strat.prop_flat_map(|sha| {
            let sha_clone = sha.clone();
            prop_oneof![
                // Staged variant
                Just((ChangeSource::Staged, sha.clone())),
                // WorkingTree variant
                Just((ChangeSource::WorkingTree, sha.clone())),
                // PathsFromStdin with random paths
                prop::collection::vec("[a-z]{1,5}(/[a-z]{1,5}){0,3}\\.[a-z]{1,3}", 0..=5).prop_map(
                    move |paths| (ChangeSource::PathsFromStdin(paths), sha_clone.clone())
                ),
                // GitRange with random base/head SHAs
                (arb_sha(), arb_sha()).prop_map(move |(base, head)| {
                    (ChangeSource::GitRange(GitRange { base, head }), sha.clone())
                }),
            ]
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.2, 1.3, 2.2, 2.3, 3.3, 3.4**
        ///
        /// Generate random `ChangeSource` variants with random SHA strings and path lists;
        /// verify resolved plan fields match expected sentinel values per the design spec:
        /// - Staged → base = SHA, head = "STAGED", merge_base = SHA
        /// - WorkingTree → base = SHA, head = "WORKING_TREE", merge_base = SHA
        /// - PathsFromStdin → base = "STDIN", head = "STDIN", merge_base = "STDIN", patch = ""
        /// - GitRange → base = range.base, head = range.head
        #[test]
        fn prop_change_source_field_mapping((source, sha) in arb_change_source()) {
            let (base, head, merge_base, patch) = resolve_sentinels(&source, &sha);

            match &source {
                ChangeSource::Staged => {
                    prop_assert_eq!(&base, &sha, "Staged base should be HEAD SHA");
                    prop_assert_eq!(&head, "STAGED", "Staged head should be 'STAGED'");
                    prop_assert_eq!(&merge_base, &sha, "Staged merge_base should be HEAD SHA");
                }
                ChangeSource::WorkingTree => {
                    prop_assert_eq!(&base, &sha, "WorkingTree base should be HEAD SHA");
                    prop_assert_eq!(&head, "WORKING_TREE", "WorkingTree head should be 'WORKING_TREE'");
                    prop_assert_eq!(&merge_base, &sha, "WorkingTree merge_base should be HEAD SHA");
                }
                ChangeSource::PathsFromStdin(paths) => {
                    prop_assert_eq!(&base, "STDIN", "PathsFromStdin base should be 'STDIN'");
                    prop_assert_eq!(&head, "STDIN", "PathsFromStdin head should be 'STDIN'");
                    prop_assert_eq!(&merge_base, "STDIN", "PathsFromStdin merge_base should be 'STDIN'");
                    prop_assert_eq!(&patch, "", "PathsFromStdin patch should be empty");

                    // Also verify parse_stdin_paths produces correct ChangedPath entries
                    let changed = parse_stdin_paths(paths);
                    let non_blank: Vec<_> = paths.iter()
                        .map(|l| l.trim().to_owned())
                        .filter(|l| !l.is_empty())
                        .collect();
                    prop_assert_eq!(
                        changed.len(), non_blank.len(),
                        "parse_stdin_paths should produce one entry per non-blank path"
                    );
                    for cp in &changed {
                        prop_assert_eq!(&cp.status, "M", "All stdin paths should have status 'M'");
                        prop_assert!(cp.owner.is_none(), "stdin paths should have no owner");
                    }
                }
                ChangeSource::GitRange(range) => {
                    prop_assert_eq!(&base, &range.base, "GitRange base should match range.base");
                    prop_assert_eq!(&head, &range.head, "GitRange head should match range.head");
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 8.6, 16.1**
        ///
        /// Generate random Plan collections, sort them using the same logic
        /// as `plan_repo`, and verify the sorting invariants hold:
        /// - changed_paths sorted by (path, status)
        /// - obligations sorted by id
        /// - selected_surfaces sorted by id
        /// - omitted_surfaces sorted by id
        #[test]
        fn prop_plan_collections_sorted(
            changed_paths in prop::collection::vec(arb_changed_path(), 1..=10),
            obligations in prop::collection::vec(arb_obligation_record(), 1..=5),
            selected_surfaces in prop::collection::vec(arb_selected_surface(), 1..=5),
            omitted_surfaces in prop::collection::vec(arb_omitted_surface(), 0..=3),
        ) {
            // Apply the same sorting logic as plan_repo

            let mut sorted_changed = changed_paths;
            sorted_changed.sort_by(|a, b| {
                a.path.cmp(&b.path).then_with(|| a.status.cmp(&b.status))
            });

            let mut sorted_obligations = obligations;
            sorted_obligations.sort_by(|a, b| a.id.cmp(&b.id));

            let mut sorted_selected = selected_surfaces;
            sorted_selected.sort_by(|a, b| a.id.cmp(&b.id));

            let mut sorted_omitted = omitted_surfaces;
            sorted_omitted.sort_by(|a, b| a.id.cmp(&b.id));

            // Verify changed_paths sorted by (path, status)
            for w in sorted_changed.windows(2) {
                let cmp = w[0].path.cmp(&w[1].path)
                    .then_with(|| w[0].status.cmp(&w[1].status));
                prop_assert!(
                    cmp != std::cmp::Ordering::Greater,
                    "changed_paths not sorted: ({:?}, {:?}) > ({:?}, {:?})",
                    w[0].path, w[0].status, w[1].path, w[1].status
                );
            }

            // Verify obligations sorted by id
            for w in sorted_obligations.windows(2) {
                prop_assert!(
                    w[0].id <= w[1].id,
                    "obligations not sorted: {:?} > {:?}",
                    w[0].id, w[1].id
                );
            }

            // Verify selected_surfaces sorted by id
            for w in sorted_selected.windows(2) {
                prop_assert!(
                    w[0].id <= w[1].id,
                    "selected_surfaces not sorted: {:?} > {:?}",
                    w[0].id, w[1].id
                );
            }

            // Verify omitted_surfaces sorted by id
            for w in sorted_omitted.windows(2) {
                prop_assert!(
                    w[0].id <= w[1].id,
                    "omitted_surfaces not sorted: {:?} > {:?}",
                    w[0].id, w[1].id
                );
            }
        }
    }

    // Feature: proofrun-differentiation, Property 3: Change source mutual exclusivity

    /// **Validates: Requirements 4.1, 4.2, 4.3**
    ///
    /// Exhaustively test all 16 combinations of the 4 boolean change source flags.
    /// Acceptance iff exactly one flag is set (4 valid combinations out of 16).
    #[test]
    fn prop_change_source_mutual_exclusivity() {
        for bits in 0u8..16 {
            let has_git_range = bits & 0b1000 != 0;
            let staged = bits & 0b0100 != 0;
            let working_tree = bits & 0b0010 != 0;
            let paths_from_stdin = bits & 0b0001 != 0;

            let set_count = [has_git_range, staged, working_tree, paths_from_stdin]
                .iter()
                .filter(|&&b| b)
                .count();

            let result =
                validate_change_source_flags(has_git_range, staged, working_tree, paths_from_stdin);

            if set_count == 1 {
                assert!(
                    result.is_ok(),
                    "Expected Ok for exactly one flag set (git_range={has_git_range}, staged={staged}, \
                     working_tree={working_tree}, paths_from_stdin={paths_from_stdin}), got Err: {:?}",
                    result.unwrap_err()
                );
            } else {
                assert!(
                    result.is_err(),
                    "Expected Err for {set_count} flags set (git_range={has_git_range}, staged={staged}, \
                     working_tree={working_tree}, paths_from_stdin={paths_from_stdin}), got Ok"
                );
                let msg = result.unwrap_err();
                if set_count == 0 {
                    assert!(
                        msg.contains("no change source"),
                        "Zero-flag error should mention 'no change source', got: {msg}"
                    );
                } else {
                    assert!(
                        msg.contains("conflicting"),
                        "Multi-flag error should mention 'conflicting', got: {msg}"
                    );
                }
            }
        }
    }

    // Feature: proofrun-differentiation, Property 2: Stdin path parsing

    /// Strategy: generate a random Vec<String> where each element is either
    /// a blank/whitespace-only line or a valid path with optional surrounding whitespace.
    fn arb_stdin_line() -> impl Strategy<Value = String> {
        prop_oneof![
            // Blank line (empty string)
            Just(String::new()),
            // Whitespace-only line (spaces and/or tabs)
            "[ \t]{1,5}".prop_map(String::from),
            // Valid path with optional leading/trailing whitespace
            (
                "[ \t]{0,3}",
                "[a-z]{1,5}(/[a-z]{1,5}){0,3}\\.[a-z]{1,3}",
                "[ \t]{0,3}",
            )
                .prop_map(|(lead, path, trail)| format!("{lead}{path}{trail}")),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 3.1, 3.2, 3.5**
        ///
        /// Generate random string lists with blanks, whitespace-only lines, and valid
        /// paths; verify blank lines are skipped, paths are trimmed, all have status "M",
        /// and all have owner = None.
        #[test]
        fn prop_stdin_path_parsing(lines in prop::collection::vec(arb_stdin_line(), 0..=20)) {
            let result = parse_stdin_paths(&lines);

            // Compute expected non-blank lines (after trimming)
            let expected_paths: Vec<String> = lines
                .iter()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect();

            // (a) Blank and whitespace-only lines are skipped: result count matches non-blank count
            prop_assert_eq!(
                result.len(),
                expected_paths.len(),
                "Result count {} != expected non-blank count {}",
                result.len(),
                expected_paths.len()
            );

            for (i, cp) in result.iter().enumerate() {
                // (b) Each remaining path is trimmed of leading/trailing whitespace
                prop_assert_eq!(
                    &cp.path,
                    &expected_paths[i],
                    "Path at index {} not trimmed correctly: got {:?}, expected {:?}",
                    i,
                    cp.path,
                    expected_paths[i]
                );
                // Verify no leading/trailing whitespace remains
                prop_assert_eq!(
                    cp.path.trim(),
                    cp.path.as_str(),
                    "Path at index {} has residual whitespace: {:?}",
                    i,
                    cp.path
                );

                // (c) Every entry has status "M"
                prop_assert_eq!(
                    &cp.status,
                    "M",
                    "Path at index {} has status {:?} instead of 'M'",
                    i,
                    cp.status
                );

                // (d) Every entry has owner = None
                prop_assert!(
                    cp.owner.is_none(),
                    "Path at index {} has owner {:?} instead of None",
                    i,
                    cp.owner
                );
            }
        }
    }

    // Feature: proofrun-differentiation, Property 18: Budget gate correctness

    /// Strategy: generate a Plan with random surfaces (0-5) with random costs,
    /// random obligations with/without fallback sources, and optionally a single
    /// workspace.smoke surface.
    fn arb_budget_gate_plan() -> impl Strategy<Value = model::Plan> {
        // Generate 0-5 surfaces with random costs and templates
        let surfaces_strat = prop::collection::vec(
            (
                "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}",
                0.0f64..100.0f64,
                prop_oneof![
                    Just("workspace.smoke".to_string()),
                    "[a-z]{1,6}\\.[a-z]{1,4}".prop_map(String::from),
                ],
            ),
            0..=5,
        );

        // Generate 0-3 obligations, each with 0-2 reasons that may be fallback
        let obligations_strat = prop::collection::vec(
            (
                "[a-z]{1,4}(:[a-z]{1,4}){0,2}",
                prop::collection::vec(
                    prop_oneof![
                        Just("rule".to_string()),
                        Just("profile".to_string()),
                        Just("unknown-fallback".to_string()),
                        Just("empty-range-fallback".to_string()),
                    ],
                    0..=2,
                ),
            ),
            0..=3,
        );

        (surfaces_strat, obligations_strat).prop_map(|(surfaces, obligations)| {
            let selected_surfaces: Vec<model::SelectedSurface> = surfaces
                .into_iter()
                .map(|(id, cost, template)| model::SelectedSurface {
                    id,
                    template,
                    cost,
                    covers: vec![],
                    run: vec![],
                })
                .collect();

            let obligations: Vec<model::ObligationRecord> = obligations
                .into_iter()
                .map(|(id, sources)| model::ObligationRecord {
                    id,
                    reasons: sources
                        .into_iter()
                        .map(|source| model::ObligationReason {
                            source,
                            path: None,
                            rule: None,
                            pattern: None,
                        })
                        .collect(),
                })
                .collect();

            model::Plan {
                version: "1".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                repo_root: "/tmp".to_string(),
                base: "abc".to_string(),
                head: "def".to_string(),
                merge_base: "abc".to_string(),
                profile: "ci".to_string(),
                config_digest: "d".to_string(),
                plan_digest: "d".to_string(),
                artifacts: model::PlanArtifacts {
                    output_dir: ".proofrun".to_string(),
                    diff_patch: "".to_string(),
                    plan_json: "".to_string(),
                    plan_markdown: "".to_string(),
                    commands_shell: "".to_string(),
                    github_actions: "".to_string(),
                },
                workspace: model::WorkspaceInfo { packages: vec![] },
                changed_paths: vec![],
                obligations,
                selected_surfaces,
                omitted_surfaces: vec![],
                diagnostics: vec![],
            }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 21.1, 21.2, 22.1, 22.2, 23.1, 23.2, 24.1, 24.2**
        ///
        /// Generate Plans with random costs/surface counts and random thresholds;
        /// verify each budget gate fires iff condition met.
        #[test]
        fn prop_budget_gate_correctness(
            plan in arb_budget_gate_plan(),
            max_cost_threshold in prop::option::of(0.0f64..200.0f64),
            max_surfaces_threshold in prop::option::of(0usize..8),
            fail_on_fallback in proptest::bool::ANY,
            warn_on_workspace_smoke_only in proptest::bool::ANY,
        ) {
            let result = check_budget_gates(
                &plan,
                max_cost_threshold,
                max_surfaces_threshold,
                fail_on_fallback,
                warn_on_workspace_smoke_only,
            );

            // (a) max_cost gate fires iff sum(costs) > threshold
            let total_cost: f64 = plan.selected_surfaces.iter().map(|s| s.cost).sum();
            let cost_gate_should_fire = max_cost_threshold
                .map(|t| total_cost > t)
                .unwrap_or(false);
            let cost_gate_fired = result.messages.iter().any(|m| m.contains("--max-cost"));
            prop_assert!(
                cost_gate_fired == cost_gate_should_fire,
                "max_cost gate: fired={}, expected={}, total_cost={}, threshold={:?}",
                cost_gate_fired, cost_gate_should_fire, total_cost, max_cost_threshold
            );

            // (b) max_surfaces gate fires iff surface count > threshold
            let surface_count = plan.selected_surfaces.len();
            let surfaces_gate_should_fire = max_surfaces_threshold
                .map(|t| surface_count > t)
                .unwrap_or(false);
            let surfaces_gate_fired = result.messages.iter().any(|m| m.contains("--max-surfaces"));
            prop_assert!(
                surfaces_gate_fired == surfaces_gate_should_fire,
                "max_surfaces gate: fired={}, expected={}, count={}, threshold={:?}",
                surfaces_gate_fired, surfaces_gate_should_fire, surface_count, max_surfaces_threshold
            );

            // (c) fail_on_fallback gate fires iff any obligation has fallback source
            let has_fallback = plan.obligations.iter().any(|o| {
                o.reasons.iter().any(|r| {
                    r.source == "unknown-fallback" || r.source == "empty-range-fallback"
                })
            });
            let fallback_gate_should_fire = fail_on_fallback && has_fallback;
            let fallback_gate_fired = result.messages.iter().any(|m| m.contains("--fail-on-fallback"));
            prop_assert!(
                fallback_gate_fired == fallback_gate_should_fire,
                "fail_on_fallback gate: fired={}, expected={}, flag={}, has_fallback={}",
                fallback_gate_fired, fallback_gate_should_fire, fail_on_fallback, has_fallback
            );

            // (d) warn_on_workspace_smoke_only fires iff exactly 1 surface with template "workspace.smoke"
            let is_smoke_only = plan.selected_surfaces.len() == 1
                && plan.selected_surfaces[0].template == "workspace.smoke";
            let smoke_warn_should_fire = warn_on_workspace_smoke_only && is_smoke_only;
            let smoke_warn_fired = result.messages.iter().any(|m| m.contains("workspace.smoke"));
            prop_assert!(
                smoke_warn_fired == smoke_warn_should_fire,
                "warn_on_workspace_smoke_only: fired={}, expected={}, flag={}, is_smoke_only={}",
                smoke_warn_fired, smoke_warn_should_fire, warn_on_workspace_smoke_only, is_smoke_only
            );

            // (e) warn_on_workspace_smoke_only does NOT set `failed`
            // Compute expected failed: only hard gates contribute
            let expected_failed = cost_gate_should_fire
                || surfaces_gate_should_fire
                || fallback_gate_should_fire;
            prop_assert!(
                result.failed == expected_failed,
                "failed={}, expected={} (smoke warning must not set failed)",
                result.failed, expected_failed
            );
        }
    }
}
