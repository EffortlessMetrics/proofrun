use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8Path;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cargo_workspace::WorkspaceGraph;
use crate::config::{load_config, Config};
use crate::obligations::expand_template;
use crate::planner::fnmatch_match;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorFinding {
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub repo_root: String,
    pub config_path: String,
    pub cargo_manifest_path: String,
    pub package_count: usize,
    pub packages: Vec<String>,
    pub issues: Vec<String>,
    pub findings: Vec<DoctorFinding>,
}

/// Check repo readiness, matching the reference `doctor()` function.
///
/// 1. Set repo_root, config_path, cargo_manifest_path
/// 2. Check if Cargo.toml exists — if not, add issue
/// 3. Check if proofrun.toml exists — if not, add issue with specific message
/// 4. Load config (using default fallback)
/// 5. Try to discover workspace — if successful, get package names; if failed, set packages to empty
/// 6. Check: no packages, no profiles, no surfaces, no rules
/// 7. Return DoctorReport
pub fn doctor_repo(repo_root: &Utf8Path) -> DoctorReport {
    let config_path = repo_root.join("proofrun.toml");
    let cargo_path = repo_root.join("Cargo.toml");

    // Load config (falls back to built-in default if proofrun.toml missing)
    let config = load_config(repo_root).unwrap_or_else(|_| {
        // If config loading fails entirely, parse the default
        toml::from_str(crate::config::DEFAULT_CONFIG_TOML)
            .expect("built-in default config must parse")
    });

    // Try to discover workspace packages
    let workspace_graph = WorkspaceGraph::discover(repo_root).ok();
    let packages: Vec<String> = workspace_graph
        .as_ref()
        .map(|g| g.packages.iter().map(|p| p.name.clone()).collect())
        .unwrap_or_default();
    let package_dirs: Vec<String> = workspace_graph
        .as_ref()
        .map(|g| g.packages.iter().map(|p| p.dir.to_string()).collect())
        .unwrap_or_default();

    let mut issues = Vec::new();
    let mut findings = Vec::new();

    if !cargo_path.exists() {
        let msg = "missing Cargo.toml".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "missing-cargo-toml".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }
    if !config_path.exists() {
        let msg = "proofrun.toml missing; using built-in default config".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "missing-proofrun-toml".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }
    if packages.is_empty() {
        let msg = "no Cargo packages discovered".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "no-packages".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }
    if config.profiles.is_empty() {
        let msg = "no profiles configured".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "no-profiles".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }
    if config.surfaces.is_empty() {
        let msg = "no surfaces configured".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "no-surfaces".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }
    if config.rules.is_empty() {
        let msg = "no rules configured".to_owned();
        findings.push(DoctorFinding {
            severity: "warning".to_owned(),
            code: "no-rules".to_owned(),
            message: msg.clone(),
        });
        issues.push(msg);
    }

    // Check for duplicate surface IDs
    findings.extend(check_duplicate_surfaces(&config));

    // Check for uncovered obligations
    findings.extend(check_uncovered_obligations(&config, &packages));

    // Check for unreachable rules
    findings.extend(check_unreachable_rules(&config, &package_dirs));

    // Check for unbound placeholders in surface templates
    findings.extend(check_unbound_placeholders(&config));

    // Check for missing tools in PATH
    findings.extend(check_missing_tools());

    DoctorReport {
        repo_root: repo_root.to_string(),
        config_path: config_path.to_string(),
        cargo_manifest_path: cargo_path.to_string(),
        package_count: packages.len(),
        packages,
        issues,
        findings,
    }
}

/// Check a Config for duplicate surface IDs.
///
/// Returns a `DoctorFinding` with severity `"error"` and code
/// `"duplicate-surface-id"` for each id that appears more than once.
pub(crate) fn check_duplicate_surfaces(config: &Config) -> Vec<DoctorFinding> {
    let mut id_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for surface in &config.surfaces {
        *id_counts.entry(surface.id.clone()).or_insert(0) += 1;
    }
    let mut findings = Vec::new();
    for (id, count) in &id_counts {
        if *count > 1 {
            findings.push(DoctorFinding {
                severity: "error".to_owned(),
                code: "duplicate-surface-id".to_owned(),
                message: format!("duplicate surface id '{}' ({} occurrences)", id, count),
            });
        }
    }
    findings
}

/// Check for obligations that no surface template can cover (R13).
///
/// For each rule emit pattern, expand with all workspace package names as `{owner}`.
/// For each expanded obligation, check if any surface template's expanded cover
/// patterns match it (using fnmatch-style matching). If no surface covers the
/// obligation, emit a warning finding with code `"uncovered-obligation"`.
pub(crate) fn check_uncovered_obligations(
    config: &Config,
    package_names: &[String],
) -> Vec<DoctorFinding> {
    let mut findings = Vec::new();

    // Collect all possible expanded obligation patterns from rules
    let mut all_obligations: BTreeSet<String> = BTreeSet::new();
    for rule in &config.rules {
        for emit_template in &rule.emit {
            if emit_template.contains("{owner}") {
                // Expand with each package name
                for pkg in package_names {
                    let mut values = BTreeMap::new();
                    values.insert("owner".to_string(), pkg.clone());
                    if let Ok(expanded) = expand_template(emit_template, &values) {
                        all_obligations.insert(expanded);
                    }
                }
            } else {
                // No {owner} placeholder — the emit pattern is literal
                all_obligations.insert(emit_template.clone());
            }
        }
    }

    // Collect all possible expanded cover patterns from surface templates
    let mut all_cover_patterns: Vec<String> = Vec::new();
    for surface in &config.surfaces {
        for cover_template in &surface.covers {
            if cover_template.contains("{pkg}") {
                // Expand with each package name
                for pkg in package_names {
                    let mut values = BTreeMap::new();
                    values.insert("pkg".to_string(), pkg.clone());
                    if let Ok(expanded) = expand_template(cover_template, &values) {
                        all_cover_patterns.push(expanded);
                    }
                }
            } else {
                // No {pkg} placeholder — may contain wildcards like `pkg:*:tests`
                all_cover_patterns.push(cover_template.clone());
            }
        }
    }

    // Check each obligation against all cover patterns
    for obligation in &all_obligations {
        let covered = all_cover_patterns
            .iter()
            .any(|pat| fnmatch_match(pat, obligation));
        if !covered {
            findings.push(DoctorFinding {
                severity: "warning".to_owned(),
                code: "uncovered-obligation".to_owned(),
                message: format!(
                    "obligation '{}' is not covered by any surface template",
                    obligation
                ),
            });
        }
    }

    findings
}

/// Check for rules whose path patterns can never match any workspace package (R14).
///
/// For each rule path pattern that uses a `crates/*/` prefix, check if any
/// workspace package directory starts with `crates/`. If no package directory
/// matches, emit a warning finding with code `"unreachable-rule"`.
pub(crate) fn check_unreachable_rules(
    config: &Config,
    package_dirs: &[String],
) -> Vec<DoctorFinding> {
    let mut findings = Vec::new();

    for (idx, rule) in config.rules.iter().enumerate() {
        let rule_index = idx + 1;
        for pattern in &rule.when.paths {
            // Check if the pattern uses a `crates/*/` prefix
            if !pattern.starts_with("crates/*/") {
                continue;
            }

            // Check if any package directory matches `crates/*`
            // i.e., any package dir starts with "crates/"
            let has_match = package_dirs.iter().any(|dir| dir.starts_with("crates/"));

            if !has_match {
                findings.push(DoctorFinding {
                    severity: "warning".to_owned(),
                    code: "unreachable-rule".to_owned(),
                    message: format!(
                        "rule {} pattern '{}' uses crates/*/ prefix but no workspace package directory matches",
                        rule_index, pattern
                    ),
                });
            }
        }
    }

    findings
}

/// Check for unbound placeholders in surface templates (R15).
///
/// Scans each surface template's `id`, `covers`, and `run` strings for
/// `{placeholder}` patterns. Known placeholders are `pkg`, `profile`, and
/// `artifacts.diff_patch`. Any other placeholder produces a finding with
/// severity `"error"` and code `"unbound-placeholder"`.
pub(crate) fn check_unbound_placeholders(config: &Config) -> Vec<DoctorFinding> {
    let known: BTreeSet<&str> = ["pkg", "profile", "artifacts.diff_patch"]
        .iter()
        .copied()
        .collect();
    let re = Regex::new(r"\{([^}]+)\}").expect("placeholder regex must compile");

    let mut findings = Vec::new();

    for surface in &config.surfaces {
        // Collect all strings to scan: id, covers, run
        let mut strings_to_scan: Vec<&str> = vec![&surface.id];
        for s in &surface.covers {
            strings_to_scan.push(s);
        }
        for s in &surface.run {
            strings_to_scan.push(s);
        }

        for s in &strings_to_scan {
            for cap in re.captures_iter(s) {
                let placeholder = &cap[1];
                if !known.contains(placeholder) {
                    findings.push(DoctorFinding {
                        severity: "error".to_owned(),
                        code: "unbound-placeholder".to_owned(),
                        message: format!(
                            "surface '{}' contains unbound placeholder '{{{}}}' in string \"{}\"",
                            surface.id, placeholder, s
                        ),
                    });
                }
            }
        }
    }

    findings
}

/// Check whether a tool is available on the system PATH by attempting
/// to run it with `--version`. Returns `true` if the command spawns
/// successfully (regardless of exit code), `false` if it cannot be found.
fn is_tool_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Check PATH for required tools (R16).
///
/// Checks for `git`, `cargo`, `cargo-nextest`, and `cargo-mutants`.
/// Missing `git`/`cargo` → severity `"error"`.
/// Missing `cargo-nextest`/`cargo-mutants` → severity `"warning"`.
pub(crate) fn check_missing_tools() -> Vec<DoctorFinding> {
    let tools: &[(&str, &str)] = &[
        ("git", "error"),
        ("cargo", "error"),
        ("cargo-nextest", "warning"),
        ("cargo-mutants", "warning"),
    ];

    let mut findings = Vec::new();
    for &(tool, severity) in tools {
        if !is_tool_available(tool) {
            findings.push(DoctorFinding {
                severity: severity.to_owned(),
                code: "missing-tool".to_owned(),
                message: format!("'{}' not found in PATH", tool),
            });
        }
    }
    findings
}

/// Check whether a `DoctorReport` should cause a strict-mode failure.
///
/// Returns `true` if any finding has severity `"error"`, `false` otherwise.
pub fn should_fail_strict(report: &DoctorReport) -> bool {
    report.findings.iter().any(|f| f.severity == "error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Defaults, SurfaceTemplate, UnknownConfig};
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    /// Build a minimal Config with the given surface templates.
    fn make_config(surfaces: Vec<SurfaceTemplate>) -> Config {
        Config {
            version: 1,
            defaults: Defaults::default(),
            profiles: BTreeMap::new(),
            surfaces,
            rules: Vec::new(),
            unknown: UnknownConfig::default(),
        }
    }

    /// Strategy for generating a SurfaceTemplate with a given id.
    fn arb_surface_template(id: String) -> SurfaceTemplate {
        SurfaceTemplate {
            id,
            covers: vec!["pkg:test:tests".to_owned()],
            cost: 1.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }
    }

    /// Strategy: generate 1–6 surface ids where some may be duplicates.
    /// We pick from a small pool of ids to increase the chance of collisions.
    fn arb_surface_ids() -> impl Strategy<Value = Vec<String>> {
        let id_pool = prop::collection::vec("[a-z]{1,4}\\.[a-z]{1,4}", 1..=3);
        id_pool.prop_flat_map(|pool| {
            let pool_clone = pool.clone();
            prop::collection::vec(
                (0..pool_clone.len()).prop_map(move |i| pool[i].clone()),
                1..=6,
            )
        })
    }

    /// Check whether a list of ids contains any duplicates.
    fn has_duplicates(ids: &[String]) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                return true;
            }
        }
        false
    }

    // Feature: proofrun-differentiation, Property 11: Doctor duplicate surface detection
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 12.1, 12.2**
        ///
        /// For any Config where two or more surface templates share the same id,
        /// check_duplicate_surfaces returns at least one DoctorFinding with
        /// severity "error" and code "duplicate-surface-id".
        /// For any Config with all unique surface ids, no such finding is present.
        #[test]
        fn prop_duplicate_surface_detection(ids in arb_surface_ids()) {
            let surfaces: Vec<SurfaceTemplate> = ids.iter()
                .map(|id| arb_surface_template(id.clone()))
                .collect();
            let config = make_config(surfaces);
            let findings = check_duplicate_surfaces(&config);

            let dup_findings: Vec<&DoctorFinding> = findings.iter()
                .filter(|f| f.code == "duplicate-surface-id")
                .collect();

            if has_duplicates(&ids) {
                // Duplicates exist → at least one finding must be present
                prop_assert!(
                    !dup_findings.is_empty(),
                    "expected duplicate-surface-id finding when duplicates exist, ids: {:?}",
                    ids
                );
                // All such findings must have severity "error"
                for f in &dup_findings {
                    prop_assert_eq!(
                        &f.severity, "error",
                        "duplicate-surface-id finding must have severity error"
                    );
                }
            } else {
                // No duplicates → no such finding
                prop_assert!(
                    dup_findings.is_empty(),
                    "expected no duplicate-surface-id finding when all ids unique, ids: {:?}",
                    ids
                );
            }
        }
    }

    // --- Uncovered obligation tests (R13) ---

    use crate::config::Rule;
    use crate::config::RuleWhen;

    /// Build a Config with given surfaces and rules.
    fn make_config_with_rules(surfaces: Vec<SurfaceTemplate>, rules: Vec<Rule>) -> Config {
        Config {
            version: 1,
            defaults: Defaults::default(),
            profiles: BTreeMap::new(),
            surfaces,
            rules,
            unknown: UnknownConfig::default(),
        }
    }

    #[test]
    fn test_uncovered_obligation_all_covered() {
        // Rule emits pkg:{owner}:tests, surface covers pkg:{pkg}:tests
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 3.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }];
        let config = make_config_with_rules(surfaces, rules);
        let packages = vec!["core".to_owned(), "app".to_owned()];

        let findings = check_uncovered_obligations(&config, &packages);
        let uncovered: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "uncovered-obligation")
            .collect();
        assert!(uncovered.is_empty(), "all obligations should be covered");
    }

    #[test]
    fn test_uncovered_obligation_wildcard_cover() {
        // Rule emits pkg:{owner}:tests, surface covers pkg:*:tests (wildcard)
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let surfaces = vec![SurfaceTemplate {
            id: "workspace.all-tests".to_owned(),
            covers: vec!["pkg:*:tests".to_owned()],
            cost: 10.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }];
        let config = make_config_with_rules(surfaces, rules);
        let packages = vec!["core".to_owned(), "app".to_owned()];

        let findings = check_uncovered_obligations(&config, &packages);
        let uncovered: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "uncovered-obligation")
            .collect();
        assert!(
            uncovered.is_empty(),
            "wildcard cover should match all pkg obligations"
        );
    }

    #[test]
    fn test_uncovered_obligation_missing_surface() {
        // Rule emits pkg:{owner}:mutation-diff but no surface covers it
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:mutation-diff".to_owned()],
        }];
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 3.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }];
        let config = make_config_with_rules(surfaces, rules);
        let packages = vec!["core".to_owned()];

        let findings = check_uncovered_obligations(&config, &packages);
        let uncovered: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "uncovered-obligation")
            .collect();
        assert_eq!(uncovered.len(), 1);
        assert_eq!(uncovered[0].severity, "warning");
        assert!(uncovered[0].message.contains("pkg:core:mutation-diff"));
    }

    #[test]
    fn test_uncovered_obligation_literal_emit() {
        // Rule emits workspace:docs (no {owner}), surface covers workspace:docs
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["docs/**".to_owned()],
            },
            emit: vec!["workspace:docs".to_owned()],
        }];
        let surfaces = vec![SurfaceTemplate {
            id: "workspace.docs".to_owned(),
            covers: vec!["workspace:docs".to_owned()],
            cost: 4.0,
            run: vec!["cargo".to_owned(), "doc".to_owned()],
        }];
        let config = make_config_with_rules(surfaces, rules);
        let packages = vec!["core".to_owned()];

        let findings = check_uncovered_obligations(&config, &packages);
        let uncovered: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "uncovered-obligation")
            .collect();
        assert!(uncovered.is_empty(), "literal obligation should be covered");
    }

    #[test]
    fn test_uncovered_obligation_no_packages() {
        // With no packages, {owner} templates produce no obligations
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let surfaces = vec![];
        let config = make_config_with_rules(surfaces, rules);
        let packages: Vec<String> = vec![];

        let findings = check_uncovered_obligations(&config, &packages);
        let uncovered: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "uncovered-obligation")
            .collect();
        assert!(
            uncovered.is_empty(),
            "no packages means no expanded obligations"
        );
    }

    // --- Unreachable rule tests (R14) ---

    #[test]
    fn test_unreachable_rule_with_matching_packages() {
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let config = make_config_with_rules(vec![], rules);
        let package_dirs = vec!["crates/core".to_owned(), "crates/app".to_owned()];

        let findings = check_unreachable_rules(&config, &package_dirs);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unreachable-rule")
            .collect();
        assert!(
            unreachable.is_empty(),
            "rule should be reachable when packages exist under crates/"
        );
    }

    #[test]
    fn test_unreachable_rule_no_matching_packages() {
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let config = make_config_with_rules(vec![], rules);
        let package_dirs = vec!["src".to_owned(), "lib".to_owned()];

        let findings = check_unreachable_rules(&config, &package_dirs);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unreachable-rule")
            .collect();
        assert_eq!(unreachable.len(), 1);
        assert_eq!(unreachable[0].severity, "warning");
        assert!(unreachable[0].message.contains("crates/*/src/**/*.rs"));
    }

    #[test]
    fn test_unreachable_rule_non_crates_pattern_ignored() {
        // Patterns that don't start with crates/*/ are not checked
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["docs/**".to_owned(), "**/*.md".to_owned()],
            },
            emit: vec!["workspace:docs".to_owned()],
        }];
        let config = make_config_with_rules(vec![], rules);
        let package_dirs = vec!["src".to_owned()];

        let findings = check_unreachable_rules(&config, &package_dirs);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unreachable-rule")
            .collect();
        assert!(
            unreachable.is_empty(),
            "non-crates patterns should not be flagged"
        );
    }

    #[test]
    fn test_unreachable_rule_empty_package_dirs() {
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_owned()],
            },
            emit: vec!["pkg:{owner}:tests".to_owned()],
        }];
        let config = make_config_with_rules(vec![], rules);
        let package_dirs: Vec<String> = vec![];

        let findings = check_unreachable_rules(&config, &package_dirs);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unreachable-rule")
            .collect();
        assert_eq!(unreachable.len(), 1);
        assert_eq!(unreachable[0].severity, "warning");
    }

    // --- Unbound placeholder tests (R15) ---

    #[test]
    fn test_unbound_placeholder_all_known() {
        // Surface uses only known placeholders — no findings expected
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 3.0,
            run: vec![
                "cargo".to_owned(),
                "nextest".to_owned(),
                "run".to_owned(),
                "--profile".to_owned(),
                "{profile}".to_owned(),
                "-E".to_owned(),
                "package({pkg})".to_owned(),
            ],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert!(unbound.is_empty(), "all placeholders are known");
    }

    #[test]
    fn test_unbound_placeholder_artifacts_diff_patch() {
        // {artifacts.diff_patch} is a known placeholder
        let surfaces = vec![SurfaceTemplate {
            id: "mutation.diff".to_owned(),
            covers: vec!["pkg:{pkg}:mutation-diff".to_owned()],
            cost: 13.0,
            run: vec![
                "cargo".to_owned(),
                "mutants".to_owned(),
                "--in-diff".to_owned(),
                "{artifacts.diff_patch}".to_owned(),
                "--package".to_owned(),
                "{pkg}".to_owned(),
            ],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert!(
            unbound.is_empty(),
            "artifacts.diff_patch is a known placeholder"
        );
    }

    #[test]
    fn test_unbound_placeholder_unknown_in_run() {
        // Surface uses {unknown_thing} in run — should produce a finding
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 3.0,
            run: vec![
                "cargo".to_owned(),
                "test".to_owned(),
                "--flag".to_owned(),
                "{unknown_thing}".to_owned(),
            ],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert_eq!(unbound.len(), 1);
        assert_eq!(unbound[0].severity, "error");
        assert!(unbound[0].message.contains("unknown_thing"));
        assert!(unbound[0].message.contains("tests.pkg"));
    }

    #[test]
    fn test_unbound_placeholder_unknown_in_covers() {
        // Surface uses {foo} in covers — should produce a finding
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_owned(),
            covers: vec!["pkg:{foo}:tests".to_owned()],
            cost: 3.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert_eq!(unbound.len(), 1);
        assert_eq!(unbound[0].severity, "error");
        assert!(unbound[0].message.contains("foo"));
    }

    #[test]
    fn test_unbound_placeholder_unknown_in_id() {
        // Surface uses {bar} in id — should produce a finding
        let surfaces = vec![SurfaceTemplate {
            id: "tests.{bar}".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 3.0,
            run: vec!["cargo".to_owned(), "test".to_owned()],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert_eq!(unbound.len(), 1);
        assert_eq!(unbound[0].severity, "error");
        assert!(unbound[0].message.contains("bar"));
    }

    #[test]
    fn test_unbound_placeholder_no_placeholders() {
        // Surface with no placeholders at all — no findings
        let surfaces = vec![SurfaceTemplate {
            id: "workspace.smoke".to_owned(),
            covers: vec!["workspace:smoke".to_owned()],
            cost: 2.0,
            run: vec![
                "cargo".to_owned(),
                "test".to_owned(),
                "--workspace".to_owned(),
            ],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert!(unbound.is_empty(), "no placeholders means no findings");
    }

    #[test]
    fn test_unbound_placeholder_multiple_unknown() {
        // Surface with multiple unknown placeholders across different fields
        let surfaces = vec![SurfaceTemplate {
            id: "tests.{bad_id}".to_owned(),
            covers: vec!["pkg:{bad_cover}:tests".to_owned()],
            cost: 3.0,
            run: vec!["cargo".to_owned(), "{bad_run}".to_owned()],
        }];
        let config = make_config(surfaces);
        let findings = check_unbound_placeholders(&config);
        let unbound: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unbound-placeholder")
            .collect();
        assert_eq!(unbound.len(), 3);
        for f in &unbound {
            assert_eq!(f.severity, "error");
        }
    }

    // --- Property test: unbound placeholder detection (Property 12) ---

    /// Known placeholders that should NOT trigger an unbound-placeholder finding.
    const KNOWN_PLACEHOLDERS: &[&str] = &["pkg", "profile", "artifacts.diff_patch"];

    /// Strategy: generate a random unknown placeholder name (lowercase alpha, 3–10 chars)
    /// that is guaranteed not to collide with any known placeholder.
    fn arb_unknown_placeholder() -> impl Strategy<Value = String> {
        "[a-z]{3,10}".prop_filter("must not be a known placeholder", |s| {
            !KNOWN_PLACEHOLDERS.contains(&s.as_str())
        })
    }

    /// Build a SurfaceTemplate whose run args contain the given placeholder strings
    /// wrapped in `{…}`. The id and covers use only known placeholders.
    fn make_surface_with_run_placeholders(placeholders: &[String]) -> SurfaceTemplate {
        let mut run: Vec<String> = vec!["cargo".to_owned(), "test".to_owned()];
        for p in placeholders {
            run.push(format!("{{{}}}", p));
        }
        SurfaceTemplate {
            id: "test.surface".to_owned(),
            covers: vec!["pkg:{pkg}:tests".to_owned()],
            cost: 1.0,
            run,
        }
    }

    // Feature: proofrun-differentiation, Property 12: Doctor unbound placeholder detection
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 15.1, 15.2**
        ///
        /// For any surface template containing a placeholder not in the known set
        /// (pkg, profile, artifacts.diff_patch), check_unbound_placeholders returns
        /// at least one DoctorFinding with severity "error" and code
        /// "unbound-placeholder". For any surface template using only known
        /// placeholders, no such finding is present.
        #[test]
        fn prop_unbound_placeholder_detection(
            inject_unknown in proptest::bool::ANY,
            unknown_name in arb_unknown_placeholder(),
            use_known in proptest::collection::vec(
                proptest::sample::select(KNOWN_PLACEHOLDERS),
                0..=3
            ),
        ) {
            // Build run args from the selected known placeholders
            let mut placeholders: Vec<String> = use_known
                .iter()
                .map(|s| s.to_string())
                .collect();

            // Optionally inject an unknown placeholder
            if inject_unknown {
                placeholders.push(unknown_name.clone());
            }

            let surface = make_surface_with_run_placeholders(&placeholders);
            let config = make_config(vec![surface]);
            let findings = check_unbound_placeholders(&config);

            let unbound_findings: Vec<&DoctorFinding> = findings
                .iter()
                .filter(|f| f.code == "unbound-placeholder")
                .collect();

            if inject_unknown {
                // Unknown placeholder injected → at least one finding must exist
                prop_assert!(
                    !unbound_findings.is_empty(),
                    "expected unbound-placeholder finding when unknown '{}' injected, \
                     placeholders: {:?}",
                    unknown_name,
                    placeholders
                );
                // All findings must have severity "error"
                for f in &unbound_findings {
                    prop_assert_eq!(
                        &f.severity, "error",
                        "unbound-placeholder finding must have severity error"
                    );
                }
                // At least one finding must mention the unknown placeholder name
                let mentions_unknown = unbound_findings
                    .iter()
                    .any(|f| f.message.contains(&unknown_name));
                prop_assert!(
                    mentions_unknown,
                    "expected at least one finding to mention '{}', findings: {:?}",
                    unknown_name,
                    unbound_findings.iter().map(|f| &f.message).collect::<Vec<_>>()
                );
            } else {
                // Only known placeholders → no unbound-placeholder findings
                prop_assert!(
                    unbound_findings.is_empty(),
                    "expected no unbound-placeholder finding when only known placeholders used, \
                     placeholders: {:?}, findings: {:?}",
                    placeholders,
                    unbound_findings.iter().map(|f| &f.message).collect::<Vec<_>>()
                );
            }
        }
    }

    // --- Strict mode exit behavior (Property 13) ---

    /// Strategy: generate a random DoctorFinding with severity "error" or "warning".
    fn arb_finding() -> impl Strategy<Value = DoctorFinding> {
        (
            prop::sample::select(vec!["error".to_owned(), "warning".to_owned()]),
            "[a-z\\-]{3,15}",
            "[a-zA-Z0-9 ]{5,30}",
        )
            .prop_map(|(severity, code, message)| DoctorFinding {
                severity,
                code,
                message,
            })
    }

    /// Build a minimal DoctorReport with the given findings.
    fn make_report(findings: Vec<DoctorFinding>) -> DoctorReport {
        DoctorReport {
            repo_root: ".".to_owned(),
            config_path: "proofrun.toml".to_owned(),
            cargo_manifest_path: "Cargo.toml".to_owned(),
            package_count: 0,
            packages: vec![],
            issues: vec![],
            findings,
        }
    }

    // Feature: proofrun-differentiation, Property 13: Doctor strict mode exit behavior
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 17.1, 17.2, 17.3**
        ///
        /// For any DoctorReport with random findings of varying severity,
        /// `should_fail_strict` returns true iff any finding has severity "error".
        /// When no finding has severity "error", it returns false.
        #[test]
        fn prop_strict_mode_exit_behavior(
            findings in prop::collection::vec(arb_finding(), 0..=5),
        ) {
            let has_error = findings.iter().any(|f| f.severity == "error");
            let report = make_report(findings);
            let result = should_fail_strict(&report);

            prop_assert_eq!(
                result,
                has_error,
                "should_fail_strict should return true iff any finding has severity \"error\""
            );
        }
    }

    // --- Missing tool detection tests (R16) ---

    #[test]
    fn test_is_tool_available_finds_existing_tool() {
        // `git` and `cargo` should be available in any dev environment
        assert!(is_tool_available("git"), "git should be available");
        assert!(is_tool_available("cargo"), "cargo should be available");
    }

    #[test]
    fn test_is_tool_available_returns_false_for_nonexistent() {
        assert!(
            !is_tool_available("this-tool-definitely-does-not-exist-xyz-123"),
            "nonexistent tool should not be available"
        );
    }

    #[test]
    fn test_check_missing_tools_severity_mapping() {
        // We can't control which tools are installed, but we can verify
        // the structure of any findings returned.
        let findings = check_missing_tools();
        for f in &findings {
            assert_eq!(f.code, "missing-tool");
            match f.message.as_str() {
                m if m.contains("'git'") => assert_eq!(f.severity, "error"),
                m if m.contains("'cargo'") => assert_eq!(f.severity, "error"),
                m if m.contains("'cargo-nextest'") => assert_eq!(f.severity, "warning"),
                m if m.contains("'cargo-mutants'") => assert_eq!(f.severity, "warning"),
                _ => panic!("unexpected missing-tool finding: {}", f.message),
            }
        }
    }

    #[test]
    fn test_check_missing_tools_no_duplicates() {
        let findings = check_missing_tools();
        let tool_names: Vec<&str> = findings.iter().map(|f| f.message.as_str()).collect();
        let unique: std::collections::BTreeSet<&str> = tool_names.iter().copied().collect();
        assert_eq!(
            tool_names.len(),
            unique.len(),
            "each tool should appear at most once"
        );
    }
}
