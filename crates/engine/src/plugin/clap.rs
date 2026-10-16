//! Hosting CLAP plugins (`.clap` bundles). Not written yet.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::{Plugin, Processor};
use crate::{project::PluginRef, scan::PluginInfo};

/// A loaded `.clap` bundle.
pub struct Library;

/// The plugins in a `.clap` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    Err(format!(
        "{}: CLAP plugins aren't supported yet",
        bundle.display()
    ))
}

/// Loads the plugin `r` names, from its bundle (loaded once into `libraries`).
pub fn instantiate(
    _libraries: &mut HashMap<PathBuf, Library>,
    r: &PluginRef,
    _rate: f64,
    _max_block: usize,
    _state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    Err(format!("{}: CLAP plugins aren't supported yet", r.class))
}
