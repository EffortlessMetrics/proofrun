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

/// The reference path prefix used in recorded fixture artifacts.
const REF_REPO_ROOT: &str = "/mnt/data/proofrun/fixtures/demo-workspace/repo";

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

/// Run plan_repo for a given scenario and return the plan.
fn run_scenario(base: &str, head: &str) -> proofrun::Plan {
    let ws = workspace_root();
    let repo_root = ws.join("fixtures/demo-workspace/repo");
    let range = GitRange {
        base: base.to_string(),
        head: head.to_string(),
    };
    plan_repo(&repo_root, range, "ci").expect("plan_repo should succeed")
}

/// Load the recorded plan.json for a scenario.
fn load_reference_plan(scenario: &str) -> serde_json::Value {
    let ws = workspace_root();
    let path = ws.join(format!(
        "fixtures/demo-workspace/sample/{scenario}/plan.json"
    ));
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    serde_json::from_str(&content).expect("plan.json should be valid JSON")
}

/// Load a recorded text artifact for a scenario.
fn load_reference_text(scenario: &str, filename: &str) -> String {
    let ws = workspace_root();
    let path = ws.join(format!(
        "fixtures/demo-workspace/sample/{scenario}/{filename}"
    ));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// Load the recorded receipt.json for a scenario.
fn load_reference_receipt(scenario: &str) -> serde_json::Value {
    let ws = workspace_root();
    let path = ws.join(format!(
        "fixtures/demo-workspace/sample/{scenario}/receipt.json"
    ));
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
    let expected_md_normalized = normalize_ref_path(&expected_md, actual_repo_root);

    let actual_lines: Vec<String> = normalize_slashes(&actual_md)
        .lines()
        .map(|l| l.to_owned())
        .collect();
    let expected_lines: Vec<String> = normalize_slashes(&expected_md_normalized)
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
    let expected_sh_normalized = normalize_ref_path(&expected_sh, actual_repo_root);
    assert_eq!(
        normalize_slashes(&actual_sh),
        normalize_slashes(&expected_sh_normalized),
        "{scenario}/commands.sh mismatch"
    );

    // github-actions.yml — should match exactly after path normalization
    let actual_gha = emit_github_actions(plan);
    let expected_gha = load_reference_text(scenario, "github-actions.yml");
    let expected_gha_normalized = normalize_ref_path(&expected_gha, actual_repo_root);
    assert_eq!(
        normalize_slashes(&actual_gha),
        normalize_slashes(&expected_gha_normalized),
        "{scenario}/github-actions.yml mismatch"
    );
}

/// Assert that dry-run receipt structure matches the reference.
fn assert_receipt_matches(plan: &proofrun::Plan, scenario: &str) {
    let ws = workspace_root();
    let repo_root = ws.join("fixtures/demo-workspace/repo");

    let receipt = execute_plan(&repo_root, plan, ExecutionMode::DryRun)
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

// ─── Test cases ───

#[test]
fn test_core_change_plan_fields() {
    let plan = run_scenario(
        "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    );
    let reference = load_reference_plan("core-change");
    assert_plan_fields_match(&plan, &reference, "core-change");
}

#[test]
fn test_core_change_emitted_text() {
    let plan = run_scenario(
        "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    );
    assert_emitted_text_matches(&plan, "core-change");
}

#[test]
fn test_core_change_receipt() {
    let plan = run_scenario(
        "8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717",
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
    );
    assert_receipt_matches(&plan, "core-change");
}

#[test]
fn test_docs_change_plan_fields() {
    let plan = run_scenario(
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
        "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd",
    );
    let reference = load_reference_plan("docs-change");
    assert_plan_fields_match(&plan, &reference, "docs-change");
}

#[test]
fn test_docs_change_emitted_text() {
    let plan = run_scenario(
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
        "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd",
    );
    assert_emitted_text_matches(&plan, "docs-change");
}

#[test]
fn test_docs_change_receipt() {
    let plan = run_scenario(
        "ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb",
        "1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd",
    );
    assert_receipt_matches(&plan, "docs-change");
}
