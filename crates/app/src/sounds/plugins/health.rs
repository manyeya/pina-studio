//! Health: how much room the plugins take and how much can be had back (copies of a plugin in
//! formats other than the one Pina Studio uses, plugins that won't load, plugins this Mac can't
//! run without Rosetta), what the songs use, and what's never been in a song.

use std::path::PathBuf;

use super::*;

/// What Health finds, worked out from the facts.
pub(crate) struct Checkup {
    pub total: u64,
    /// The files that could go: copies, plugins that won't load, those needing Rosetta.
    pub spare: Vec<PathBuf>,
    pub spare_size: u64,
    pub copies: Vec<usize>,
    pub copies_size: u64,
    pub wont_load: Vec<usize>,
    pub rosetta: Vec<usize>,
    pub in_songs: Vec<usize>,
    pub never: Vec<usize>,
    pub never_size: u64,
}

impl Workspace {
    pub(crate) fn checkup(&self) -> Checkup {
        let page = &self.plugin_page;
        let file = |p: &PathBuf| page.files.get(p).cloned().unwrap_or_default();
        let mut c = Checkup {
            total: 0,
            spare: Vec::new(),
            spare_size: 0,
            copies: Vec::new(),
            copies_size: 0,
            wont_load: Vec::new(),
            rosetta: Vec::new(),
            in_songs: Vec::new(),
            never: Vec::new(),
            never_size: 0,
        };
        let mut counted = std::collections::HashSet::new();
        for (i, d) in self.catalog.devices.iter().enumerate() {
            if d.own.is_some() {
                continue;
            }
            for p in &d.formats {
                if counted.insert(p.path.clone()) {
                    c.total += file(&p.path).size;
                }
            }
            let spare = |p: &PathBuf, c: &mut Checkup| {
                let f = file(p);
                if !f.system && !c.spare.contains(p) {
                    c.spare_size += f.size;
                    c.spare.push(p.clone());
                }
            };
            if d.formats.len() > 1 {
                c.copies.push(i);
                for p in &d.formats[1..] {
                    // A bundle holding several plugins (a suite) isn't a copy of this one.
                    c.copies_size += file(&p.path).size;
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::WontLoad) {
                c.wont_load.push(i);
                for p in &d.formats {
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::NeedsRosetta) {
                c.rosetta.push(i);
                for p in d.formats.iter().filter(|p| file(&p.path).intel_only) {
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::InSongs) {
                c.in_songs.push(i);
            }
            if self.in_set(d, HealthSet::NeverInASong) {
                c.never.push(i);
                c.never_size += self.device_size(d);
            }
        }
        // A file another plugin is used from (a suite in one bundle) stays.
        let kept: std::collections::HashSet<&PathBuf> = self
            .catalog
            .devices
            .iter()
            .filter(|d| {
                d.own.is_none()
                    && !self.in_set(d, HealthSet::WontLoad)
                    && !self.in_set(d, HealthSet::NeedsRosetta)
            })
            .filter_map(|d| d.formats.first().map(|p| &p.path))
            .collect();
        c.spare.retain(|p| !kept.contains(p));
        c.spare_size = c.spare.iter().map(|p| file(p).size).sum();
        let uses = page
            .songs
            .as_ref()
            .map(|s| s.used.clone())
            .unwrap_or_default();
        c.in_songs.sort_by_key(|&i| {
            std::cmp::Reverse(uses.get(&self.catalog.devices[i].key).copied().unwrap_or(0))
        });
        c.never
            .sort_by_key(|&i| std::cmp::Reverse(self.device_size(&self.catalog.devices[i])));
        c
    }
}
