use crate::model::{Plan, Receipt, ReceiptStep};
use anyhow::Result;

#[derive(Debug, Clone, Copy)]
pub enum ExecutionMode {
    Execute,
    DryRun,
}

pub fn execute_plan(_plan: &Plan, _mode: ExecutionMode) -> Result<Receipt> {
    anyhow::bail!("execution scaffolded but not yet ported from the reference implementation")
}
