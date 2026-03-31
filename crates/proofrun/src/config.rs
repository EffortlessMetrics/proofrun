use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub defaults: Defaults,
    pub profiles: std::collections::BTreeMap<String, Profile>,
    #[serde(rename = "surface")]
    pub surfaces: Vec<SurfaceTemplate>,
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
    pub unknown: UnknownConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
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
    pub mode: String,
}

fn default_output_dir() -> String {
    ".proofrun".to_owned()
}

pub fn load_config(repo_root: &Utf8Path) -> Result<Config> {
    let config_path = repo_root.join("proofrun.toml");
    let raw = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config at {}", config_path))?;
    let config: Config = toml::from_str(&raw).context("failed to parse proofrun.toml")?;
    Ok(config)
}

pub fn default_config_path(repo_root: &Utf8Path) -> Utf8PathBuf {
    repo_root.join("proofrun.toml")
}
