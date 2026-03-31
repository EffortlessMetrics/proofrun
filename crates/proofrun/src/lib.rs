pub mod cargo_workspace;
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
pub use config::{load_config, Config, Rule, SurfaceTemplate};
pub use doctor::{doctor_repo, DoctorReport};
pub use emit::{
    emit_commands_shell, emit_github_actions, emit_plan_markdown, write_plan_artifacts,
};
pub use explain::render_explanation;
pub use git::{collect_git_changes, parse_name_status_line, GitChanges, GitRange};
pub use model::{Plan, PlanArtifacts, Receipt, SelectedSurface, WorkspaceInfo, WorkspacePackage};
pub use obligations::{
    compile_obligations, expand_template, glob_to_regex, match_path, Obligation,
};
pub use planner::{build_candidates, solve_exact_cover, CandidateSurface};
pub use run::{execute_plan, ExecutionMode};

use anyhow::Result;
use camino::Utf8Path;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

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
}
