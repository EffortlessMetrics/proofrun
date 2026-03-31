use crate::config::SurfaceTemplate;
use crate::model::SelectedSurface;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct CandidateSurface {
    pub id: String,
    pub template: String,
    pub cost: f64,
    pub covers: BTreeSet<String>,
    pub run: Vec<String>,
}

pub fn build_candidates(
    _templates: &[SurfaceTemplate],
    _obligations: &BTreeMap<String, Vec<crate::model::ObligationReason>>,
    _profile: &str,
    _artifacts_diff_patch: &str,
) -> anyhow::Result<Vec<CandidateSurface>> {
    anyhow::bail!("candidate expansion scaffolded but not yet ported from the reference implementation")
}

pub fn solve_plan(
    _obligations: &BTreeSet<String>,
    _candidates: &[CandidateSurface],
) -> anyhow::Result<Vec<SelectedSurface>> {
    anyhow::bail!("exact solver scaffolded but not yet ported from the reference implementation")
}
