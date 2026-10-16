//! Hosting Audio Units (v2, and v3 through the v2 API). Not written yet.

use super::{Plugin, Processor};
use crate::{project::PluginRef, scan::PluginInfo};

/// The Audio Units installed, the system's own included.
pub fn list() -> Vec<PluginInfo> {
    Vec::new()
}

/// Loads the Audio Unit `r` names.
pub fn instantiate(
    r: &PluginRef,
    _rate: f64,
    _max_block: usize,
    _state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    Err(format!("{}: Audio Units aren't supported yet", r.class))
}
