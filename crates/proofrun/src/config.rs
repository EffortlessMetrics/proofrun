use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

/// Built-in default config matching the Python reference `DEFAULT_CONFIG_TOML`.
/// Used when `proofrun.toml` does not exist in the repo root.
pub const DEFAULT_CONFIG_TOML: &str = r#"
version = 1

[defaults]
output_dir = ".proofrun"

[profiles.local]
always = ["workspace:smoke"]

[profiles.ci]
always = ["workspace:smoke"]

[[surface]]
id = "tests.pkg"
covers = ["pkg:{pkg}:tests"]
cost = 3
run = ["cargo", "nextest", "run", "--profile", "{profile}", "-E", "package({pkg})"]

[[surface]]
id = "tests.rdeps"
covers = ["pkg:{pkg}:rdeps"]
cost = 8
run = ["cargo", "nextest", "run", "--profile", "{profile}", "-E", "rdeps({pkg})"]

[[surface]]
id = "workspace.all-tests"
covers = ["pkg:*:tests"]
cost = 10
run = ["cargo", "nextest", "run", "--profile", "{profile}", "--workspace"]

[[surface]]
id = "mutation.diff"
covers = ["pkg:{pkg}:mutation-diff"]
cost = 13
run = ["cargo", "mutants", "--in-diff", "{artifacts.diff_patch}", "--package", "{pkg}"]

[[surface]]
id = "workspace.docs"
covers = ["workspace:docs"]
cost = 4
run = ["cargo", "doc", "--workspace", "--no-deps"]

[[surface]]
id = "workspace.smoke"
covers = ["workspace:smoke"]
cost = 2
run = ["cargo", "test", "--workspace", "--quiet"]

[[rule]]
when.paths = ["crates/*/src/**/*.rs", "crates/*/tests/**/*.rs"]
emit = ["pkg:{owner}:tests", "pkg:{owner}:mutation-diff"]

[[rule]]
when.paths = ["**/Cargo.toml", "Cargo.lock", ".cargo/**", "**/build.rs"]
emit = ["pkg:{owner}:tests", "pkg:{owner}:rdeps", "workspace:smoke"]

[[rule]]
when.paths = ["docs/**", "book/**", "**/*.md"]
emit = ["workspace:docs"]

[unknown]
fallback = ["workspace:smoke"]
mode = "fail-closed"
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub profiles: std::collections::BTreeMap<String, Profile>,
    #[serde(default, rename = "surface")]
    pub surfaces: Vec<SurfaceTemplate>,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub unknown: UnknownConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    #[serde(default)]
    pub always: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceTemplate {
    pub id: String,
    pub covers: Vec<String>,
    pub cost: f64,
    pub run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub when: RuleWhen,
    pub emit: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleWhen {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownConfig {
    #[serde(default)]
    pub fallback: Vec<String>,
    #[serde(default = "default_mode")]
    pub mode: String,
}

impl Default for UnknownConfig {
    fn default() -> Self {
        Self {
            fallback: Vec::new(),
            mode: default_mode(),
        }
    }
}

fn default_output_dir() -> String {
    ".proofrun".to_owned()
}

fn default_mode() -> String {
    "fail-closed".to_owned()
}

/// Load config from `proofrun.toml` in the repo root, falling back to the
/// built-in default config if the file does not exist.
pub fn load_config(repo_root: &Utf8Path) -> Result<Config> {
    let config_path = repo_root.join("proofrun.toml");
    let raw = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config at {}", config_path))?
    } else {
        DEFAULT_CONFIG_TOML.to_owned()
    };
    let config: Config = toml::from_str(&raw).context("failed to parse proofrun.toml")?;
    Ok(config)
}

pub fn default_config_path(repo_root: &Utf8Path) -> Utf8PathBuf {
    repo_root.join("proofrun.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_config() {
        let config: Config =
            toml::from_str(DEFAULT_CONFIG_TOML).expect("DEFAULT_CONFIG_TOML should parse");
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.output_dir, ".proofrun");
        assert_eq!(config.surfaces.len(), 6);
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.unknown.mode, "fail-closed");
        assert_eq!(config.unknown.fallback, vec!["workspace:smoke"]);
        assert!(config.profiles.contains_key("local"));
        assert!(config.profiles.contains_key("ci"));
    }

    #[test]
    fn serde_defaults_applied_for_minimal_config() {
        let minimal = "version = 1\n";
        let config: Config = toml::from_str(minimal).expect("minimal config should parse");
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.output_dir, ".proofrun");
        assert!(config.profiles.is_empty());
        assert!(config.surfaces.is_empty());
        assert!(config.rules.is_empty());
        assert_eq!(config.unknown.mode, "fail-closed");
        assert!(config.unknown.fallback.is_empty());
    }

    #[test]
    fn load_config_falls_back_to_default() {
        // Use a non-existent directory so proofrun.toml won't be found
        let fake_root = Utf8Path::new("/tmp/proofrun-test-nonexistent-dir-12345");
        let config = load_config(fake_root).expect("should fall back to default config");
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.output_dir, ".proofrun");
        assert_eq!(config.surfaces.len(), 6);
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.unknown.mode, "fail-closed");
        assert_eq!(config.unknown.fallback, vec!["workspace:smoke"]);
    }

    #[test]
    fn load_config_reads_existing_file() {
        // Use the repo root which has a proofrun.toml
        let repo_root = Utf8Path::new(".");
        let config = load_config(repo_root).expect("should read existing proofrun.toml");
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.output_dir, ".proofrun");
        // The repo's proofrun.toml has the same content as the default
        assert_eq!(config.surfaces.len(), 6);
        assert_eq!(config.rules.len(), 3);
    }

    #[test]
    fn default_config_matches_reference_content() {
        // Verify the default config has the same surfaces as the reference
        let config: Config =
            toml::from_str(DEFAULT_CONFIG_TOML).expect("DEFAULT_CONFIG_TOML should parse");

        let surface_ids: Vec<&str> = config.surfaces.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            surface_ids,
            vec![
                "tests.pkg",
                "tests.rdeps",
                "workspace.all-tests",
                "mutation.diff",
                "workspace.docs",
                "workspace.smoke",
            ]
        );

        // Verify rule patterns
        assert_eq!(
            config.rules[0].when.paths,
            vec!["crates/*/src/**/*.rs", "crates/*/tests/**/*.rs"]
        );
        assert_eq!(
            config.rules[0].emit,
            vec!["pkg:{owner}:tests", "pkg:{owner}:mutation-diff"]
        );

        assert_eq!(
            config.rules[1].when.paths,
            vec!["**/Cargo.toml", "Cargo.lock", ".cargo/**", "**/build.rs"]
        );
        assert_eq!(
            config.rules[1].emit,
            vec!["pkg:{owner}:tests", "pkg:{owner}:rdeps", "workspace:smoke"]
        );

        assert_eq!(
            config.rules[2].when.paths,
            vec!["docs/**", "book/**", "**/*.md"]
        );
        assert_eq!(config.rules[2].emit, vec!["workspace:docs"]);

        // Verify profiles
        assert_eq!(config.profiles["local"].always, vec!["workspace:smoke"]);
        assert_eq!(config.profiles["ci"].always, vec!["workspace:smoke"]);
    }
}
