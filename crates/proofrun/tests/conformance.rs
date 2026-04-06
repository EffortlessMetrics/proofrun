// Feature: rust-native-planner, Property 14: Fixture parity with reference implementation
//
// **Validates: Requirements 15.1, 15.2, 15.3, 15.4**
//
// For all fixture scenarios in `fixtures/demo-workspace`, the Rust planner
// produces plan.json with identical obligations, selected_surfaces,
// omitted_surfaces, changed_paths, and workspace fields, and identical
// plan.md, commands.sh, github-actions.yml content, and identical dry-run
// receipt.json structure compared to the recorded reference output.

use proofrun::{
    emit_commands_shell, emit_github_actions, emit_plan_markdown, execute_plan, plan_repo,
    ExecutionMode, GitRange,
};

/// Resolve the workspace root (two levels up from CARGO_MANIFEST_DIR which is crates/proofrun).
fn workspace_root() -> camino::Utf8PathBuf {
    let manifest_dir = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("crates/ parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

const REF_REPO_ROOT: &str = "/mnt/data/proofrun/fixtures/demo-workspace/repo";

fn fixture_root() -> camino::Utf8PathBuf {
    workspace_root().join("fixtures/demo-workspace")
}

fn fixture_repo_root() -> camino::Utf8PathBuf {
    fixture_root().join("repo")
}

/// Replace the reference repo root with the actual repo root in a string.
/// Normalizes to forward slashes for cross-platform consistency.
fn normalize_ref_path(text: &str, actual_repo_root: &str) -> String {
    let normalized_root = actual_repo_root.replace('\\', "/");
    text.replace(REF_REPO_ROOT, &normalized_root)
}

/// Normalize all path separators to forward slashes in a JSON string.
fn normalize_slashes(text: &str) -> String {
    // In JSON, backslashes are escaped as \\, so we need to handle both
    // the raw string and JSON-escaped versions
    text.replace('\\', "/")
}

fn normalize_artifact_text(text: &str) -> String {
    normalize_slashes(text).replace("\r\n", "\n")
}

fn fixture_scenario_path(scenario: &str) -> camino::Utf8PathBuf {
    fixture_root().join("sample").join(scenario)
}

/// Compare two serde_json::Value fields, normalizing any string values that
/// contain the reference repo root path.
fn assert_json_eq_normalized(
    label: &str,
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    actual_repo_root: &str,
) {
    let actual_str = serde_json::to_string_pretty(actual).unwrap();
    let expected_str = serde_json::to_string_pretty(expected).unwrap();
    let expected_normalized = normalize_ref_path(&expected_str, actual_repo_root);
    // Normalize path separators on both sides for cross-platform compatibility
    let actual_normalized = normalize_slashes(&actual_str);
    let expected_final = normalize_slashes(&expected_normalized);
    assert_eq!(
        actual_normalized, expected_final,
        "\n{label} mismatch.\nActual:\n{actual_normalized}\n\nExpected (normalized):\n{expected_final}"
    );
}

// Shared scenario commit pairs.
const SCENARIO_CORE_CHANGE: (&str, &str) = (
    "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
    "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
);
const SCENARIO_DOCS_CHANGE: (&str, &str) = (
    "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd",
);
const SCENARIO_EMPTY_DIFF: (&str, &str) = (
    "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
    "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
);
const SCENARIO_DELETION_ONLY: (&str, &str) = (
    "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    "75883c2489a7eb2807c62e886076a459df88b45b",
);
const SCENARIO_RENAME: (&str, &str) = (
    "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    "91ebd2444afe6b8a600f6dc984171fa58bf7d4ac",
);

/// Run plan_repo for a given scenario and return the plan.
fn run_scenario_from_root(base: &str, head: &str, repo_root: &camino::Utf8Path) -> proofrun::Plan {
    let range = GitRange {
        base: base.to_string(),
        head: head.to_string(),
    };
    plan_repo(repo_root, range, "ci").expect("plan_repo should succeed")
}

fn run_scenario(base: &str, head: &str) -> proofrun::Plan {
    run_scenario_from_root(base, head, &fixture_repo_root())
}

/// Load the recorded plan.json for a scenario.
fn load_reference_plan(scenario: &str) -> serde_json::Value {
    let path = fixture_scenario_path(scenario).join("plan.json");
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    serde_json::from_str(&content).expect("plan.json should be valid JSON")
}

/// Load a recorded text artifact for a scenario.
fn load_reference_text(scenario: &str, filename: &str) -> String {
    let path = fixture_scenario_path(scenario).join(filename);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// Load the recorded receipt.json for a scenario.
fn load_reference_receipt(scenario: &str) -> serde_json::Value {
    let path = fixture_scenario_path(scenario).join("receipt.json");
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    serde_json::from_str(&content).expect("receipt.json should be valid JSON")
}

/// Assert that the plan's structural fields match the reference plan.json.
fn assert_plan_fields_match(plan: &proofrun::Plan, reference: &serde_json::Value, scenario: &str) {
    let actual_value = serde_json::to_value(plan).expect("plan should serialize to JSON");
    let actual_repo_root = &plan.repo_root;

    // Fields that should be identical (after path normalization for repo_root differences)
    let fields = [
        "obligations",
        "selected_surfaces",
        "omitted_surfaces",
        "changed_paths",
        "workspace",
    ];

    for field in &fields {
        let actual_field = &actual_value[field];
        let expected_field = &reference[field];
        assert!(
            !actual_field.is_null() || !expected_field.is_null(),
            "{scenario}: field '{field}' missing from one side"
        );
        assert_json_eq_normalized(
            &format!("{scenario}/{field}"),
            actual_field,
            expected_field,
            actual_repo_root,
        );
    }
}

/// Assert that emitted text artifacts match the reference (after path normalization).
/// For plan.md, the plan_digest line will differ (because repo_root affects the digest),
/// so we compare all lines except the plan_digest line.
fn assert_emitted_text_matches(plan: &proofrun::Plan, scenario: &str) {
    let actual_repo_root = &plan.repo_root;

    // plan.md — compare line-by-line, skipping the plan digest line
    let actual_md = emit_plan_markdown(plan);
    let expected_md = load_reference_text(scenario, "plan.md");
    let expected_md_normalized =
        normalize_ref_path(&normalize_artifact_text(&expected_md), actual_repo_root);
    let actual_md_normalized = normalize_artifact_text(&actual_md);

    let actual_lines: Vec<String> = actual_md_normalized
        .lines()
        .map(|l| l.to_owned())
        .collect();
    let expected_lines: Vec<String> = expected_md_normalized
        .lines()
        .map(|l| l.to_owned())
        .collect();
    assert_eq!(
        actual_lines.len(),
        expected_lines.len(),
        "{scenario}/plan.md line count mismatch: actual={}, expected={}",
        actual_lines.len(),
        expected_lines.len()
    );
    for (i, (a, e)) in actual_lines.iter().zip(expected_lines.iter()).enumerate() {
        // Skip the plan digest line (it will differ due to repo_root in the plan)
        if a.starts_with("- plan digest:") && e.starts_with("- plan digest:") {
            continue;
        }
        assert_eq!(a, e, "{scenario}/plan.md line {i} mismatch");
    }

    // commands.sh — should match exactly after path normalization
    let actual_sh = emit_commands_shell(plan);
    let expected_sh = load_reference_text(scenario, "commands.sh");
    let expected_sh_normalized =
        normalize_ref_path(&normalize_artifact_text(&expected_sh), actual_repo_root);
    let actual_sh_normalized = normalize_artifact_text(&actual_sh);
    assert_eq!(
        actual_sh_normalized,
        expected_sh_normalized,
        "{scenario}/commands.sh mismatch"
    );

    // github-actions.yml — should match exactly after path normalization
    let actual_gha = emit_github_actions(plan);
    let expected_gha = load_reference_text(scenario, "github-actions.yml");
    let expected_gha_normalized =
        normalize_ref_path(&normalize_artifact_text(&expected_gha), actual_repo_root);
    let actual_gha_normalized = normalize_artifact_text(&actual_gha);
    assert_eq!(
        actual_gha_normalized,
        expected_gha_normalized,
        "{scenario}/github-actions.yml mismatch"
    );
}

/// Assert that dry-run receipt structure matches the reference.
fn assert_receipt_matches(repo_root: &camino::Utf8Path, plan: &proofrun::Plan, scenario: &str) {
    let receipt = execute_plan(repo_root, plan, ExecutionMode::DryRun)
        .expect("dry-run execution should succeed");

    let reference = load_reference_receipt(scenario);

    // Compare status
    assert_eq!(
        receipt.status,
        reference["status"].as_str().unwrap(),
        "{scenario}: receipt status mismatch"
    );

    // Compare step count
    let ref_steps = reference["steps"].as_array().unwrap();
    assert_eq!(
        receipt.steps.len(),
        ref_steps.len(),
        "{scenario}: receipt step count mismatch"
    );

    // Compare step ids and exit codes
    for (i, (actual_step, ref_step)) in receipt.steps.iter().zip(ref_steps.iter()).enumerate() {
        assert_eq!(
            actual_step.id,
            ref_step["id"].as_str().unwrap(),
            "{scenario}: step {i} id mismatch"
        );
        assert_eq!(
            actual_step.exit_code as i64,
            ref_step["exit_code"].as_i64().unwrap(),
            "{scenario}: step {i} exit_code mismatch"
        );
    }
}

#[test]
fn test_core_change_plan_fields() {
    assert_plan_fields_match(&run_scenario(SCENARIO_CORE_CHANGE.0, SCENARIO_CORE_CHANGE.1), &load_reference_plan("core-change"), "core-change");
}

#[test]
fn test_core_change_emitted_text() {
    assert_emitted_text_matches(&run_scenario(SCENARIO_CORE_CHANGE.0, SCENARIO_CORE_CHANGE.1), "core-change");
}

#[test]
fn test_core_change_receipt() {
    assert_receipt_matches(&fixture_repo_root(), &run_scenario(SCENARIO_CORE_CHANGE.0, SCENARIO_CORE_CHANGE.1), "core-change");
}

#[test]
fn test_docs_change_plan_fields() {
    assert_plan_fields_match(&run_scenario(SCENARIO_DOCS_CHANGE.0, SCENARIO_DOCS_CHANGE.1), &load_reference_plan("docs-change"), "docs-change");
}

#[test]
fn test_docs_change_emitted_text() {
    assert_emitted_text_matches(&run_scenario(SCENARIO_DOCS_CHANGE.0, SCENARIO_DOCS_CHANGE.1), "docs-change");
}

#[test]
fn test_docs_change_receipt() {
    assert_receipt_matches(&fixture_repo_root(), &run_scenario(SCENARIO_DOCS_CHANGE.0, SCENARIO_DOCS_CHANGE.1), "docs-change");
}

#[test]
fn test_empty_diff_plan_fields() {
    assert_plan_fields_match(&run_scenario(SCENARIO_EMPTY_DIFF.0, SCENARIO_EMPTY_DIFF.1), &load_reference_plan("empty-diff"), "empty-diff");
}

#[test]
fn test_empty_diff_emitted_text() {
    assert_emitted_text_matches(&run_scenario(SCENARIO_EMPTY_DIFF.0, SCENARIO_EMPTY_DIFF.1), "empty-diff");
}

#[test]
fn test_empty_diff_receipt() {
    assert_receipt_matches(&fixture_repo_root(), &run_scenario(SCENARIO_EMPTY_DIFF.0, SCENARIO_EMPTY_DIFF.1), "empty-diff");
}

#[test]
fn test_deletion_only_plan_fields() {
    assert_plan_fields_match(&run_scenario(SCENARIO_DELETION_ONLY.0, SCENARIO_DELETION_ONLY.1), &load_reference_plan("deletion-only"), "deletion-only");
}

#[test]
fn test_deletion_only_emitted_text() {
    assert_emitted_text_matches(&run_scenario(SCENARIO_DELETION_ONLY.0, SCENARIO_DELETION_ONLY.1), "deletion-only");
}

#[test]
fn test_deletion_only_receipt() {
    assert_receipt_matches(&fixture_repo_root(), &run_scenario(SCENARIO_DELETION_ONLY.0, SCENARIO_DELETION_ONLY.1), "deletion-only");
}

#[test]
fn test_rename_plan_fields() {
    assert_plan_fields_match(&run_scenario(SCENARIO_RENAME.0, SCENARIO_RENAME.1), &load_reference_plan("rename"), "rename");
}

#[test]
fn test_rename_emitted_text() {
    assert_emitted_text_matches(&run_scenario(SCENARIO_RENAME.0, SCENARIO_RENAME.1), "rename");
}

#[test]
fn test_rename_receipt() {
    assert_receipt_matches(&fixture_repo_root(), &run_scenario(SCENARIO_RENAME.0, SCENARIO_RENAME.1), "rename");
}

#[test]
fn test_workspace_discovery_from_nested_package_path_matches_root_output() {
    let repo_root = fixture_repo_root();
    let nested_repo_root = repo_root.join("crates/core");

    let nested_plan = run_scenario_from_root(SCENARIO_DOCS_CHANGE.0, SCENARIO_DOCS_CHANGE.1, &nested_repo_root);
    let root_plan = run_scenario(SCENARIO_DOCS_CHANGE.0, SCENARIO_DOCS_CHANGE.1);

    let root_plan_value = serde_json::to_value(root_plan).unwrap_or_else(|e| panic!("serialize root plan failed: {e}"));
    assert_plan_fields_match(&nested_plan, &root_plan_value, "docs-change (nested path)");
    assert!(
        nested_plan.repo_root.ends_with("crates/core"),
        "nested scenario should resolve repo root to the nested package path",
    );
}
