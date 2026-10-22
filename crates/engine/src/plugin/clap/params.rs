//! A parameter's range, plain values to 0 to 1 and back, and how it's shown.

use super::*;

/// A parameter's range, to turn values from 0 to 1 into the plugin's own and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub(crate) id: u32,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) stepped: bool,
}

impl Range {
    pub(crate) fn of(info: &clap_param_info) -> Range {
        Range {
            id: info.id,
            min: info.min_value,
            max: info.max_value,
            stepped: info.flags & CLAP_PARAM_IS_STEPPED != 0,
        }
    }

    /// The plugin's value for `v`, 0 to 1; a stepped parameter's is a whole number.
    pub(crate) fn plain(self, v: f64) -> f64 {
        let x = self.min + v.clamp(0.0, 1.0) * (self.max - self.min);
        if self.stepped { x.round() } else { x }
    }

    /// The plugin's value `x`, 0 to 1.
    pub(crate) fn normal(self, x: f64) -> f64 {
        if self.max > self.min {
            ((x - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub(crate) fn steps(self) -> u32 {
        match self.stepped {
            true => (self.max - self.min).round().max(0.0) as u32,
            false => 0,
        }
    }
}

/// Every parameter the plugin has, as it describes them.
pub(crate) fn infos(
    plugin: *const clap_plugin,
    params: Option<&clap_plugin_params>,
) -> Vec<clap_param_info> {
    let Some((count, get)) = params.and_then(|p| Some((p.count?, p.get_info?))) else {
        return Vec::new();
    };
    unsafe {
        (0..count(plugin))
            .filter_map(|i| {
                let mut info: clap_param_info = std::mem::zeroed();
                get(plugin, i, &mut info).then_some(info)
            })
            .collect()
    }
}

/// Their ranges, by id.
pub(crate) fn ranges(infos: &[clap_param_info]) -> Vec<Range> {
    let mut ranges: Vec<Range> = infos.iter().map(Range::of).collect();
    ranges.sort_by_key(|r| r.id);
    ranges
}

pub(crate) fn range_of(ranges: &[Range], id: u32) -> Option<Range> {
    let i = ranges.binary_search_by_key(&id, |r| r.id).ok()?;
    Some(ranges[i])
}

/// The parameters a person can turn.
pub(crate) fn shown(flags: clap_param_info_flags) -> bool {
    flags & CLAP_PARAM_IS_AUTOMATABLE != 0
        && flags & (CLAP_PARAM_IS_HIDDEN | CLAP_PARAM_IS_READONLY | CLAP_PARAM_IS_BYPASS) == 0
}
