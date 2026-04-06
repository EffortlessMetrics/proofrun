use crate::config::Config;
use crate::model::{ChangedPath, ObligationReason};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    pub id: String,
    pub reasons: Vec<ObligationReason>,
}

/// Convert a glob pattern to a compiled regex.
///
/// Translation rules (matching the Python reference `glob_to_regex`):
/// - `**/` at any position → `(?:.*/)?` (zero or more directory prefixes)
/// - `**` at end of pattern → `.*` (any suffix)
/// - `*` → `[^/]*` (segment wildcard, doesn't cross `/`)
/// - `?` → `[^/]` (single non-separator char)
/// - All other characters → `regex::escape`
/// - Anchored with `^` and `$`
/// - Leading `/` is stripped from the pattern before processing.
pub fn glob_to_regex(pattern: &str) -> Regex {
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let mut i = 0;
    let bytes = pattern.as_bytes();
    let mut out = String::from("^");

    while i < bytes.len() {
        // Check for `**/` (at any position)
        if i + 2 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'/' {
            out.push_str("(?:.*/)?");
            i += 3;
            continue;
        }
        // Check for trailing `**`
        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            out.push_str(".*");
            i += 2;
            continue;
        }
        let ch = bytes[i] as char;
        if ch == '*' {
            out.push_str("[^/]*");
        } else if ch == '?' {
            out.push_str("[^/]");
        } else {
            out.push_str(&regex::escape(&ch.to_string()));
        }
        i += 1;
    }

    out.push('$');
    Regex::new(&out).expect("glob_to_regex produced an invalid regex")
}

/// Test whether `path` matches the glob `pattern`.
///
/// Both path and pattern have leading `/` stripped before matching.
pub fn match_path(path: &str, pattern: &str) -> bool {
    let path = path.strip_prefix('/').unwrap_or(path);
    glob_to_regex(pattern).is_match(path)
}

/// Expand `{key}` placeholders in a template string using the provided values map.
///
/// Matches placeholders of the form `{key}` where key is `[A-Za-z0-9_.-]+`.
/// Returns an error if any placeholder key is not found in the values map.
///
/// Direct port of the Python reference `expand_template` function.
pub fn expand_template(
    template: &str,
    values: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let re = Regex::new(r"\{([A-Za-z0-9_.\-]+)\}").expect("expand_template regex is valid");
    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for cap in re.captures_iter(template) {
        let whole_match = cap.get(0).unwrap();
        let key = &cap[1];
        match values.get(key) {
            Some(value) => {
                result.push_str(&template[last_end..whole_match.start()]);
                result.push_str(value);
                last_end = whole_match.end();
            }
            None => {
                anyhow::bail!("missing template value for {:?} in {:?}", key, template);
            }
        }
    }
    result.push_str(&template[last_end..]);
    Ok(result)
}

/// Compile obligations from changed paths, config rules, and profile.
///
/// Direct port of the Python reference `derive_obligations` function.
///
/// Sets `owner` on each `ChangedPath` during processing (matching the reference's
/// `object.__setattr__` pattern on frozen dataclasses).
///
/// Returns `(obligations_map, diagnostics)` where:
/// - `obligations_map` is a `BTreeMap<obligation_id, Vec<ObligationReason>>`
/// - `diagnostics` is a `Vec<String>` of warning messages
pub fn compile_obligations(
    config: &Config,
    profile: &str,
    changes: &mut [ChangedPath],
    workspace: &crate::cargo_workspace::WorkspaceGraph,
) -> (BTreeMap<String, Vec<ObligationReason>>, Vec<String>) {
    let mut obligations: BTreeMap<String, Vec<ObligationReason>> = BTreeMap::new();
    let mut diagnostics: Vec<String> = Vec::new();

    fn add_obligation(
        obligations: &mut BTreeMap<String, Vec<ObligationReason>>,
        id: String,
        reason: ObligationReason,
    ) {
        obligations.entry(id).or_default().push(reason);
    }

    for change in changes.iter_mut() {
        let owner = workspace.owner_for_path(&change.path).map(|s| s.to_owned());
        change.owner = owner.clone();

        for (idx, rule) in config.rules.iter().enumerate() {
            let rule_index = idx + 1; // 1-indexed, matching reference
            let patterns = &rule.when.paths;
            let matched_pattern = patterns.iter().find(|pat| match_path(&change.path, pat));

            let matched_pattern = match matched_pattern {
                Some(p) => p.clone(),
                None => continue,
            };

            for emit_template in &rule.emit {
                if emit_template.contains("{owner}") && owner.is_none() {
                    diagnostics.push(format!(
                        "unowned path {} matched rule {}",
                        change.path, rule_index
                    ));
                    if config.unknown.mode == "fail-closed" {
                        for fallback in &config.unknown.fallback {
                            add_obligation(
                                &mut obligations,
                                fallback.clone(),
                                ObligationReason {
                                    source: "unknown-fallback".to_string(),
                                    path: Some(change.path.clone()),
                                    rule: Some(format!("rule:{}", rule_index)),
                                    pattern: Some(matched_pattern.clone()),
                                },
                            );
                        }
                    }
                    continue;
                }

                let mut values = BTreeMap::new();
                values.insert(
                    "owner".to_string(),
                    owner.as_deref().unwrap_or("").to_string(),
                );

                // expand_template should not fail here since we've checked {owner} above
                let obligation_id = match expand_template(emit_template, &values) {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                add_obligation(
                    &mut obligations,
                    obligation_id,
                    ObligationReason {
                        source: "rule".to_string(),
                        path: Some(change.path.clone()),
                        rule: Some(format!("rule:{}", rule_index)),
                        pattern: Some(matched_pattern.clone()),
                    },
                );
            }
        }
    }

    // Add profile `always` obligations
    if let Some(prof) = config.profiles.get(profile) {
        for obligation_id in &prof.always {
            add_obligation(
                &mut obligations,
                obligation_id.clone(),
                ObligationReason {
                    source: "profile".to_string(),
                    path: None,
                    rule: Some(profile.to_string()),
                    pattern: None,
                },
            );
        }
    }

    // Empty-range fallback: if no obligations and fail-closed
    if obligations.is_empty() && config.unknown.mode == "fail-closed" {
        for fallback in &config.unknown.fallback {
            add_obligation(
                &mut obligations,
                fallback.clone(),
                ObligationReason {
                    source: "empty-range-fallback".to_string(),
                    path: None,
                    rule: None,
                    pattern: None,
                },
            );
        }
    }

    (obligations, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: rust-native-planner, Property 1: Glob-to-regex parity with reference
    //
    // Validates: Requirements 3.1, 3.5
    //
    // Since we cannot call the Python reference from Rust tests, we verify
    // self-consistency properties that the glob_to_regex engine must satisfy
    // to produce identical results to the reference implementation.

    /// Strategy: generate a path segment of 1-8 chars from [a-z0-9_.]
    fn path_segment() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z0-9_.]{1,8}").unwrap()
    }

    /// Strategy: generate a random path with 1-4 segments joined by `/`
    fn random_path() -> impl Strategy<Value = String> {
        prop::collection::vec(path_segment(), 1..=4).prop_map(|segs| segs.join("/"))
    }

    /// Strategy: generate a pattern segment — either a literal, `*`, `**`, or
    /// a literal with `?` replacing some chars
    fn pattern_segment() -> impl Strategy<Value = String> {
        prop_oneof![
            // literal segment
            path_segment(),
            // single star wildcard
            Just("*".to_string()),
            // double star wildcard
            Just("**".to_string()),
            // literal with ? mixed in (1-4 chars, some replaced with ?)
            prop::collection::vec(
                prop_oneof![
                    path_segment().prop_map(|s| s.chars().next().unwrap_or('a').to_string()),
                    Just("?".to_string()),
                ],
                1..=4
            )
            .prop_map(|parts| parts.join("")),
        ]
    }

    /// Strategy: generate a random pattern with 1-4 segments joined by `/`
    fn random_pattern() -> impl Strategy<Value = String> {
        prop::collection::vec(pattern_segment(), 1..=4).prop_map(|segs| segs.join("/"))
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        /// Property: `**/X` matches path iff path == X or path ends with `/X`
        /// This directly mirrors the Python reference's `(?:.*/)?X$` regex.
        #[test]
        fn prop_doublestar_slash_prefix_semantics(
            suffix in path_segment(),
            prefix_segs in prop::collection::vec(path_segment(), 0..=3),
        ) {
            let pattern = format!("**/{suffix}");

            // Path that is exactly the suffix — should match
            prop_assert!(match_path(&suffix, &pattern),
                "Expected '{}' to match pattern '{}'", suffix, pattern);

            // Path with prefix directories ending in the suffix — should match
            if !prefix_segs.is_empty() {
                let mut full_path = prefix_segs.join("/");
                full_path.push('/');
                full_path.push_str(&suffix);
                prop_assert!(match_path(&full_path, &pattern),
                    "Expected '{}' to match pattern '{}'", full_path, pattern);
            }

            // Path where suffix is only a prefix of the last segment — should NOT match
            let non_match = format!("{}extra", suffix);
            prop_assert!(!match_path(&non_match, &pattern),
                "Expected '{}' NOT to match pattern '{}'", non_match, pattern);
        }

        /// Property: `*` does not match across `/` separators.
        /// A pattern `A/*/B` should match `A/x/B` but not `A/x/y/B`.
        #[test]
        fn prop_single_star_no_slash_crossing(
            a in path_segment(),
            b in path_segment(),
            mid in path_segment(),
            extra in path_segment(),
        ) {
            let pattern = format!("{a}/*/{b}");

            // Single segment in the middle — should match
            let one_seg = format!("{a}/{mid}/{b}");
            prop_assert!(match_path(&one_seg, &pattern),
                "Expected '{}' to match '{}'", one_seg, pattern);

            // Two segments in the middle — should NOT match
            let two_seg = format!("{a}/{mid}/{extra}/{b}");
            prop_assert!(!match_path(&two_seg, &pattern),
                "Expected '{}' NOT to match '{}'", two_seg, pattern);
        }

        /// Property: exact literal patterns match only the exact path (anchoring).
        /// glob_to_regex("foo/bar") should match "foo/bar" but not "xfoo/bar",
        /// "foo/barx", or "prefix/foo/bar".
        #[test]
        fn prop_exact_literal_anchoring(
            segs in prop::collection::vec(path_segment(), 1..=3),
            extra in path_segment(),
        ) {
            let literal = segs.join("/");

            // Exact match
            prop_assert!(match_path(&literal, &literal),
                "Exact literal '{}' should match itself", literal);

            // Suffix appended — should NOT match
            let suffixed = format!("{literal}{extra}");
            if suffixed != literal {
                prop_assert!(!match_path(&suffixed, &literal),
                    "Expected '{}' NOT to match literal pattern '{}'", suffixed, literal);
            }

            // Prefix prepended — should NOT match
            let prefixed = format!("{extra}/{literal}");
            prop_assert!(!match_path(&prefixed, &literal),
                "Expected '{}' NOT to match literal pattern '{}'", prefixed, literal);
        }

        /// Property: leading `/` stripping is consistent — match_path with or
        /// without leading `/` on path and pattern gives the same result.
        #[test]
        fn prop_leading_slash_invariance(
            path in random_path(),
            pattern in random_pattern(),
        ) {
            let base = match_path(&path, &pattern);
            let with_slash_path = format!("/{path}");
            let with_slash_pattern = format!("/{pattern}");

            prop_assert_eq!(match_path(&with_slash_path, &pattern), base,
                "Leading / on path should not change result for path='{}', pattern='{}'",
                path, pattern);
            prop_assert_eq!(match_path(&path, &with_slash_pattern), base,
                "Leading / on pattern should not change result for path='{}', pattern='{}'",
                path, pattern);
            prop_assert_eq!(match_path(&with_slash_path, &with_slash_pattern), base,
                "Leading / on both should not change result for path='{}', pattern='{}'",
                path, pattern);
        }

        /// Property: trailing `**` matches any suffix after the prefix.
        /// `prefix/**` should match `prefix/anything` and `prefix/a/b/c`.
        #[test]
        fn prop_trailing_doublestar_matches_any_suffix(
            prefix in path_segment(),
            suffix_segs in prop::collection::vec(path_segment(), 1..=3),
        ) {
            let pattern = format!("{prefix}/**");
            let path = format!("{prefix}/{}", suffix_segs.join("/"));
            prop_assert!(match_path(&path, &pattern),
                "Expected '{}' to match '{}'", path, pattern);
        }

        /// Property: `?` matches exactly one non-separator character.
        /// Pattern `a/?/b` matches `a/x/b` but not `a/xy/b` or `a//b`.
        #[test]
        fn prop_question_mark_single_char(
            a in path_segment(),
            b in path_segment(),
            ch in "[a-z0-9_.]",
            extra in "[a-z0-9_.]{1,3}",
        ) {
            let pattern = format!("{a}/?/{b}");

            // Single char — should match
            let single = format!("{a}/{ch}/{b}");
            prop_assert!(match_path(&single, &pattern),
                "Expected '{}' to match '{}'", single, pattern);

            // Multiple chars — should NOT match
            let multi = format!("{a}/{ch}{extra}/{b}");
            prop_assert!(!match_path(&multi, &pattern),
                "Expected '{}' NOT to match '{}'", multi, pattern);
        }
    }

    #[test]
    fn test_glob_star_matches_single_segment() {
        assert!(match_path("src/main.rs", "src/*.rs"));
        assert!(!match_path("src/sub/main.rs", "src/*.rs"));
    }

    #[test]
    fn test_glob_double_star_slash_matches_directories() {
        assert!(match_path("crates/core/src/lib.rs", "crates/*/src/**/*.rs"));
        assert!(match_path(
            "crates/core/src/deep/nested/lib.rs",
            "crates/*/src/**/*.rs"
        ));
    }

    #[test]
    fn test_glob_double_star_trailing() {
        assert!(match_path("docs/guide.md", "docs/**"));
        assert!(match_path("docs/sub/guide.md", "docs/**"));
    }

    #[test]
    fn test_glob_question_mark() {
        assert!(match_path("src/a.rs", "src/?.rs"));
        assert!(!match_path("src/ab.rs", "src/?.rs"));
    }

    #[test]
    fn test_leading_slash_stripped() {
        assert!(match_path("/src/main.rs", "/src/*.rs"));
        assert!(match_path("src/main.rs", "/src/*.rs"));
        assert!(match_path("/src/main.rs", "src/*.rs"));
    }

    #[test]
    fn test_double_star_slash_at_start() {
        // `**/Cargo.toml` should match at any depth
        assert!(match_path("Cargo.toml", "**/Cargo.toml"));
        assert!(match_path("crates/core/Cargo.toml", "**/Cargo.toml"));
    }

    #[test]
    fn test_exact_match() {
        assert!(match_path("Cargo.lock", "Cargo.lock"));
        assert!(!match_path("Cargo.lock.bak", "Cargo.lock"));
    }

    #[test]
    fn test_reference_patterns() {
        // Patterns from the default config in the reference implementation
        assert!(match_path("crates/core/src/lib.rs", "crates/*/src/**/*.rs"));
        assert!(match_path(
            "crates/app/tests/integration.rs",
            "crates/*/tests/**/*.rs"
        ));
        assert!(match_path("Cargo.toml", "**/Cargo.toml"));
        assert!(match_path("crates/core/Cargo.toml", "**/Cargo.toml"));
        assert!(match_path(".cargo/config.toml", ".cargo/**"));
        assert!(match_path("docs/guide.md", "docs/**"));
        assert!(match_path("README.md", "**/*.md"));
        assert!(match_path("docs/sub/notes.md", "**/*.md"));
    }

    // --- expand_template tests ---
    // Validates: Requirements 4.1, 6.3

    #[test]
    fn test_expand_template_known_keys() {
        let mut values = BTreeMap::new();
        values.insert("owner".to_string(), "core".to_string());
        values.insert("profile".to_string(), "ci".to_string());
        let result = expand_template("pkg:{owner}:tests", &values).unwrap();
        assert_eq!(result, "pkg:core:tests");
    }

    #[test]
    fn test_expand_template_missing_key_returns_error() {
        let values = BTreeMap::new();
        let result = expand_template("pkg:{owner}:tests", &values);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("owner"),
            "error should mention the missing key: {err_msg}"
        );
    }

    #[test]
    fn test_expand_template_no_placeholders() {
        let values = BTreeMap::new();
        let result = expand_template("workspace:smoke", &values).unwrap();
        assert_eq!(result, "workspace:smoke");
    }

    #[test]
    fn test_expand_template_multiple_placeholders() {
        let mut values = BTreeMap::new();
        values.insert("owner".to_string(), "core".to_string());
        values.insert("profile".to_string(), "ci".to_string());
        values.insert(
            "artifacts.diff_patch".to_string(),
            ".proofrun/diff.patch".to_string(),
        );
        let result = expand_template(
            "cargo mutants --in-diff {artifacts.diff_patch} --package {owner} --profile {profile}",
            &values,
        )
        .unwrap();
        assert_eq!(
            result,
            "cargo mutants --in-diff .proofrun/diff.patch --package core --profile ci"
        );
    }

    // --- compile_obligations tests ---
    // Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5, 4.6

    use crate::cargo_workspace::{PackageInfo, WorkspaceGraph};
    use crate::config::{Config, Defaults, Profile, Rule, RuleWhen, UnknownConfig};
    use crate::model::ChangedPath;
    use camino::Utf8PathBuf;

    /// Helper: build a minimal Config for testing compile_obligations.
    fn test_config(
        rules: Vec<Rule>,
        profiles: std::collections::BTreeMap<String, Profile>,
        unknown: UnknownConfig,
    ) -> Config {
        Config {
            version: 1,
            defaults: Defaults {
                output_dir: ".proofrun".to_string(),
            },
            profiles,
            surfaces: vec![],
            rules,
            unknown,
        }
    }

    /// Helper: build a WorkspaceGraph from (name, dir) pairs.
    fn test_workspace(entries: &[(&str, &str)]) -> WorkspaceGraph {
        let packages = entries
            .iter()
            .map(|(name, dir)| PackageInfo {
                name: name.to_string(),
                dir: Utf8PathBuf::from(*dir),
                manifest: Utf8PathBuf::from(format!("{dir}/Cargo.toml")),
                dependencies: vec![],
            })
            .collect();
        WorkspaceGraph {
            packages,
            reverse_deps: BTreeMap::new(),
        }
    }

    #[test]
    fn test_compile_obligations_basic_rule_matching() {
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["crates/*/src/**/*.rs".to_string()],
            },
            emit: vec![
                "pkg:{owner}:tests".to_string(),
                "pkg:{owner}:mutation-diff".to_string(),
            ],
        }];
        let config = test_config(rules, BTreeMap::new(), UnknownConfig::default());
        let workspace = test_workspace(&[("core", "crates/core"), ("app", "crates/app")]);

        let mut changes = vec![ChangedPath {
            path: "crates/core/src/lib.rs".to_string(),
            status: "M".to_string(),
            owner: None,
        }];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        // Owner should be set on the changed path
        assert_eq!(changes[0].owner.as_deref(), Some("core"));

        // Should produce two obligations from the rule
        assert!(obligations.contains_key("pkg:core:tests"));
        assert!(obligations.contains_key("pkg:core:mutation-diff"));

        // Each obligation should have one reason with source "rule"
        let reasons = &obligations["pkg:core:tests"];
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].source, "rule");
        assert_eq!(reasons[0].path.as_deref(), Some("crates/core/src/lib.rs"));
        assert_eq!(reasons[0].rule.as_deref(), Some("rule:1"));
        assert_eq!(reasons[0].pattern.as_deref(), Some("crates/*/src/**/*.rs"));

        // No diagnostics
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_compile_obligations_unowned_path_with_owner_template() {
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["**/*.md".to_string(), "docs/**".to_string()],
            },
            emit: vec!["pkg:{owner}:docs".to_string()],
        }];
        let unknown = UnknownConfig {
            fallback: vec!["workspace:smoke".to_string()],
            mode: "fail-closed".to_string(),
        };
        let config = test_config(rules, BTreeMap::new(), unknown);
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![ChangedPath {
            path: "README.md".to_string(),
            status: "M".to_string(),
            owner: None,
        }];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        // Owner should be None (README.md is not under any package)
        assert_eq!(changes[0].owner, None);

        // Should produce a diagnostic about unowned path
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("unowned path"));
        assert!(diagnostics[0].contains("README.md"));
        assert!(diagnostics[0].contains("rule 1"));

        // Should produce fallback obligation since fail-closed
        assert!(obligations.contains_key("workspace:smoke"));
        let reasons = &obligations["workspace:smoke"];
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].source, "unknown-fallback");
        assert_eq!(reasons[0].path.as_deref(), Some("README.md"));
    }

    #[test]
    fn test_compile_obligations_profile_always() {
        let mut profiles = std::collections::BTreeMap::new();
        profiles.insert(
            "ci".to_string(),
            Profile {
                always: vec!["workspace:smoke".to_string()],
            },
        );
        let config = test_config(vec![], profiles, UnknownConfig::default());
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        // Should have the profile always obligation
        assert!(obligations.contains_key("workspace:smoke"));
        let reasons = &obligations["workspace:smoke"];
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].source, "profile");
        assert_eq!(reasons[0].path, None);
        assert_eq!(reasons[0].rule.as_deref(), Some("ci"));
        assert_eq!(reasons[0].pattern, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_compile_obligations_empty_range_fallback() {
        // No rules, no profile always, fail-closed → should get empty-range-fallback
        let unknown = UnknownConfig {
            fallback: vec!["workspace:smoke".to_string()],
            mode: "fail-closed".to_string(),
        };
        let config = test_config(vec![], BTreeMap::new(), unknown);
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![];

        let (obligations, diagnostics) =
            compile_obligations(&config, "nonexistent-profile", &mut changes, &workspace);

        // Should have the empty-range-fallback obligation
        assert!(obligations.contains_key("workspace:smoke"));
        let reasons = &obligations["workspace:smoke"];
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].source, "empty-range-fallback");
        assert_eq!(reasons[0].path, None);
        assert_eq!(reasons[0].rule, None);
        assert_eq!(reasons[0].pattern, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_compile_obligations_no_fallback_when_not_fail_closed() {
        // Unowned path with {owner} template, but mode is NOT fail-closed
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["**/*.md".to_string()],
            },
            emit: vec!["pkg:{owner}:docs".to_string()],
        }];
        let unknown = UnknownConfig {
            fallback: vec!["workspace:smoke".to_string()],
            mode: "fail-open".to_string(),
        };
        let config = test_config(rules, BTreeMap::new(), unknown);
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![ChangedPath {
            path: "README.md".to_string(),
            status: "M".to_string(),
            owner: None,
        }];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        // Should produce diagnostic but NO fallback obligation
        assert_eq!(diagnostics.len(), 1);
        assert!(obligations.is_empty());
    }

    #[test]
    fn test_compile_obligations_multiple_rules_multiple_paths() {
        let rules = vec![
            Rule {
                when: RuleWhen {
                    paths: vec!["crates/*/src/**/*.rs".to_string()],
                },
                emit: vec!["pkg:{owner}:tests".to_string()],
            },
            Rule {
                when: RuleWhen {
                    paths: vec!["**/*.md".to_string()],
                },
                emit: vec!["workspace:docs".to_string()],
            },
        ];
        let config = test_config(rules, BTreeMap::new(), UnknownConfig::default());
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![
            ChangedPath {
                path: "crates/core/src/lib.rs".to_string(),
                status: "M".to_string(),
                owner: None,
            },
            ChangedPath {
                path: "docs/guide.md".to_string(),
                status: "A".to_string(),
                owner: None,
            },
        ];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        assert!(obligations.contains_key("pkg:core:tests"));
        assert!(obligations.contains_key("workspace:docs"));

        // Rule 1 matched the .rs file
        let test_reasons = &obligations["pkg:core:tests"];
        assert_eq!(test_reasons[0].rule.as_deref(), Some("rule:1"));

        // Rule 2 matched the .md file
        let doc_reasons = &obligations["workspace:docs"];
        assert_eq!(doc_reasons[0].rule.as_deref(), Some("rule:2"));

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_compile_obligations_template_without_owner_on_unowned_path() {
        // A rule with emit templates that DON'T use {owner} should work fine for unowned paths
        let rules = vec![Rule {
            when: RuleWhen {
                paths: vec!["docs/**".to_string()],
            },
            emit: vec!["workspace:docs".to_string()],
        }];
        let config = test_config(rules, BTreeMap::new(), UnknownConfig::default());
        let workspace = test_workspace(&[("core", "crates/core")]);

        let mut changes = vec![ChangedPath {
            path: "docs/guide.md".to_string(),
            status: "A".to_string(),
            owner: None,
        }];

        let (obligations, diagnostics) =
            compile_obligations(&config, "ci", &mut changes, &workspace);

        // Should produce the obligation without any diagnostic
        assert!(obligations.contains_key("workspace:docs"));
        assert!(diagnostics.is_empty());
    }

    // Feature: rust-native-planner, Property 4: Obligation compiler produces correct obligations with reasons
    //
    // **Validates: Requirements 4.1, 4.4, 4.6**

    /// Strategy: generate a package name from a small pool.
    fn arb_pkg_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("alpha".to_string()),
            Just("beta".to_string()),
            Just("gamma".to_string()),
        ]
    }

    /// Strategy: generate a literal file name segment (no wildcards).
    fn arb_file_segment() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z]{1,6}\\.[a-z]{1,3}").unwrap()
    }

    /// Strategy: generate a literal file path under a known package dir.
    /// Returns (pkg_name, full_path) so we know the expected owner.
    fn arb_owned_path() -> impl Strategy<Value = (String, String)> {
        (arb_pkg_name(), arb_file_segment()).prop_map(|(pkg, file)| {
            let path = format!("crates/{pkg}/src/{file}");
            (pkg, path)
        })
    }

    /// Strategy: generate a rule with 1-2 literal path patterns and 1-2 emit templates.
    /// Takes a pool of literal paths to use as patterns (guaranteeing matches).
    fn arb_rule_from_paths(paths: Vec<String>) -> impl Strategy<Value = Rule> {
        let n = paths.len();
        // Pick 1-2 patterns from the available paths
        let pat_count = 1..=n.clamp(1, 2);
        let paths_clone = paths.clone();
        pat_count.prop_flat_map(move |count| {
            let ps = paths_clone.clone();
            proptest::sample::subsequence((0..ps.len()).collect::<Vec<_>>(), count)
                .prop_flat_map(move |indices| {
                    let selected: Vec<String> = indices.iter().map(|&i| ps[i].clone()).collect();
                    // Generate 1-2 emit templates
                    let emit_strat = prop::collection::vec(
                        prop_oneof![
                            Just("test:{owner}".to_string()),
                            Just("workspace:check".to_string()),
                            Just("lint:{owner}".to_string()),
                        ],
                        1..=2,
                    );
                    (Just(selected), emit_strat)
                })
                .prop_map(|(patterns, emit)| Rule {
                    when: RuleWhen { paths: patterns },
                    emit,
                })
        })
    }

    /// Strategy: generate a test scenario for compile_obligations.
    /// Returns (config, profile_name, changed_paths, workspace, owned_paths_info).
    fn arb_obligation_scenario() -> impl Strategy<
        Value = (
            Config,
            String,
            Vec<ChangedPath>,
            WorkspaceGraph,
            Vec<(String, String)>, // (pkg_name, path) for each changed path
        ),
    > {
        // Generate 1-5 owned paths
        prop::collection::vec(arb_owned_path(), 1..=5)
            .prop_flat_map(|owned_paths| {
                let all_paths: Vec<String> = owned_paths.iter().map(|(_, p)| p.clone()).collect();
                let owned = owned_paths.clone();

                // Generate 1-3 rules using the literal paths as patterns
                let rules_strat =
                    prop::collection::vec(arb_rule_from_paths(all_paths.clone()), 1..=3);

                // Optionally include a profile with always obligations
                let profile_always_strat = prop::collection::vec(
                    prop_oneof![
                        Just("workspace:smoke".to_string()),
                        Just("workspace:lint".to_string()),
                    ],
                    0..=2,
                );

                (rules_strat, profile_always_strat, Just(owned))
            })
            .prop_map(|(rules, profile_always, owned_paths)| {
                // Build workspace with the packages referenced by owned paths
                let mut pkg_dirs: BTreeMap<String, String> = BTreeMap::new();
                for (pkg, _) in &owned_paths {
                    pkg_dirs
                        .entry(pkg.clone())
                        .or_insert_with(|| format!("crates/{pkg}"));
                }
                let entries: Vec<(&str, &str)> = pkg_dirs
                    .iter()
                    .map(|(n, d)| (n.as_str(), d.as_str()))
                    .collect();
                let workspace = test_workspace(&entries);

                // Build changed paths
                let changes: Vec<ChangedPath> = owned_paths
                    .iter()
                    .map(|(_, path)| ChangedPath {
                        path: path.clone(),
                        status: "M".to_string(),
                        owner: None,
                    })
                    .collect();

                // Build config with optional profile
                let mut profiles = std::collections::BTreeMap::new();
                if !profile_always.is_empty() {
                    profiles.insert(
                        "ci".to_string(),
                        Profile {
                            always: profile_always,
                        },
                    );
                }

                let config = test_config(
                    rules,
                    profiles,
                    UnknownConfig {
                        fallback: vec![],
                        mode: "fail-open".to_string(),
                    },
                );

                (config, "ci".to_string(), changes, workspace, owned_paths)
            })
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        #[test]
        fn prop_obligation_compiler_correct_reasons(
            (config, profile, mut changes, workspace, owned_paths) in arb_obligation_scenario()
        ) {
            let (obligations, _diagnostics) =
                compile_obligations(&config, &profile, &mut changes, &workspace);

            // (a) Every obligation from a rule match has source "rule"
            // We verify by re-running the matching logic and checking each emitted obligation
            for (idx, rule) in config.rules.iter().enumerate() {
                let rule_index = idx + 1;
                for (pkg_name, path) in owned_paths.iter() {
                    let matched_pattern = rule.when.paths.iter().find(|pat| match_path(path, pat));
                    if let Some(pattern) = matched_pattern {
                        for emit_template in &rule.emit {
                            let mut values = BTreeMap::new();
                            values.insert("owner".to_string(), pkg_name.clone());
                            if let Ok(obligation_id) = expand_template(emit_template, &values) {
                                // This obligation should exist with a "rule" source reason
                                prop_assert!(
                                    obligations.contains_key(&obligation_id),
                                    "Expected obligation '{}' from rule {} matching path '{}'",
                                    obligation_id, rule_index, path
                                );
                                let reasons = &obligations[&obligation_id];
                                let has_rule_reason = reasons.iter().any(|r| {
                                    r.source == "rule"
                                        && r.path.as_deref() == Some(path.as_str())
                                        && r.rule.as_deref() == Some(&format!("rule:{}", rule_index))
                                        && r.pattern.as_deref() == Some(pattern.as_str())
                                });
                                prop_assert!(
                                    has_rule_reason,
                                    "Obligation '{}' missing rule reason for path='{}', rule={}, pattern='{}'.\nActual reasons: {:?}",
                                    obligation_id, path, rule_index, pattern, reasons
                                );
                            }
                        }
                    }
                }
            }

            // (b) Every profile `always` obligation has source "profile"
            if let Some(prof) = config.profiles.get(&profile) {
                for always_id in &prof.always {
                    prop_assert!(
                        obligations.contains_key(always_id),
                        "Expected profile always obligation '{}'", always_id
                    );
                    let reasons = &obligations[always_id];
                    let has_profile_reason = reasons.iter().any(|r| {
                        r.source == "profile"
                            && r.path.is_none()
                            && r.rule.as_deref() == Some(profile.as_str())
                            && r.pattern.is_none()
                    });
                    prop_assert!(
                        has_profile_reason,
                        "Profile always obligation '{}' missing profile reason.\nActual reasons: {:?}",
                        always_id, reasons
                    );
                }
            }

            // (c) All reasons have non-empty source fields
            for (id, reasons) in &obligations {
                prop_assert!(
                    !reasons.is_empty(),
                    "Obligation '{}' has empty reasons list", id
                );
                for reason in reasons {
                    prop_assert!(
                        !reason.source.is_empty(),
                        "Obligation '{}' has a reason with empty source", id
                    );
                }
            }

            // (d) Rule-match reasons have path, rule, and pattern fields set
            for reasons in obligations.values() {
                for reason in reasons {
                    if reason.source == "rule" {
                        prop_assert!(
                            reason.path.is_some(),
                            "Rule reason missing path field: {:?}", reason
                        );
                        prop_assert!(
                            reason.rule.is_some(),
                            "Rule reason missing rule field: {:?}", reason
                        );
                        prop_assert!(
                            reason.pattern.is_some(),
                            "Rule reason missing pattern field: {:?}", reason
                        );
                    }
                }
            }
        }
    }
}
