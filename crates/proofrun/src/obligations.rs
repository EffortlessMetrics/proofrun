use crate::config::Config;
use crate::model::{ChangedPath, ObligationReason};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    pub id: String,
    pub reasons: Vec<ObligationReason>,
}

pub fn compile_obligations(
    _config: &Config,
    _profile: &str,
    _changed_paths: &[ChangedPath],
) -> anyhow::Result<BTreeMap<String, Vec<ObligationReason>>> {
    anyhow::bail!("obligation compiler scaffolded but not yet ported from the reference implementation")
}
