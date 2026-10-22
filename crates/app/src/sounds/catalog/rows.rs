//! The Browser's rows for a search: devices, their presets, and sounds.

use super::*;

impl Workspace {
    /// The plugins came in from their scan: the Browser has them.
    pub fn catalog_plugins(&mut self) {
        let plugins: Vec<PluginInfo> = self.plugins.iter().flatten().cloned().collect();
        self.catalog.devices = devices(&plugins);
    }

    pub fn catalog_query(&self, cx: &gpui::App) -> String {
        self.catalog.search.read(cx).text().trim().to_lowercase()
    }

    /// The devices this song uses, by key.
    pub(crate) fn in_song(&self) -> Vec<String> {
        let p = &self.session.project;
        let mut keys = Vec::new();
        let plugin_key = |class: &str, path: &std::path::Path| {
            self.catalog
                .devices
                .iter()
                .find(|d| d.formats.iter().any(|f| f.class == class && f.path == path))
                .map(|d| d.key.clone())
        };
        for t in &p.tracks {
            match &t.instrument {
                Some(Instrument::Synth) => keys.push("own:synth".into()),
                Some(Instrument::Drums) => keys.push("own:drums".into()),
                Some(Instrument::Sampler(_)) => keys.push("own:sampler".into()),
                Some(Instrument::Plugin(r)) => keys.extend(plugin_key(&r.class, &r.path)),
                None => {}
            }
            if t.eq.is_some() {
                keys.push("own:eq".into());
            }
            if t.comp.is_some() {
                keys.push("own:comp".into());
            }
            if t.sat.is_some() {
                keys.push("own:sat".into());
            }
            for r in &t.inserts {
                keys.extend(plugin_key(&r.class, &r.path));
            }
        }
        for r in p
            .auxes
            .iter()
            .flat_map(|a| &a.inserts)
            .chain(&p.master_inserts)
        {
            keys.extend(plugin_key(&r.class, &r.path));
        }
        let mut seen = Vec::new();
        keys.retain(|k| {
            let new = !seen.contains(k);
            seen.push(k.clone());
            new
        });
        keys
    }

    /// The rows the Browser shows, as the search, scope and category say.
    pub fn catalog_rows(&mut self, cx: &gpui::App) -> Vec<Row> {
        let query = self.catalog_query(cx);
        let c = &self.catalog;
        let fits = |d: &Device| {
            (match c.scope {
                Scope::All | Scope::Sounds => true,
                Scope::Instruments => d.instrument(),
                Scope::Effects => !d.instrument(),
            }) && c.cat.is_none_or(|cat| d.cat == cat)
        };
        let mut rows = Vec::new();
        if !query.is_empty() {
            let mut found: Vec<(u32, usize)> = c
                .devices
                .iter()
                .enumerate()
                .filter(|(_, d)| fits(d))
                .filter_map(|(i, d)| {
                    let fav = if c.favourites.contains(&d.key) { 3 } else { 0 };
                    score(d, &query).map(|s| (s + fav, i))
                })
                .collect();
            found.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then(c.devices[a.1].name.cmp(&c.devices[b.1].name))
            });
            if !found.is_empty() {
                rows.push(Row::Heading(format!("Devices · {}", found.len())));
                rows.extend(found.into_iter().map(|(_, i)| Row::Device(i)));
            }
            // Sounds and songs that say it too, when the scope isn't only devices.
            if matches!(c.scope, Scope::All) && c.cat.is_none() {
                let sounds = self.catalog_samples(&query, 5);
                if !sounds.is_empty() {
                    rows.push(Row::Heading("Sounds".into()));
                    rows.extend(sounds.into_iter().map(Row::Sample));
                }
                let songs: Vec<crate::pages::projects::Song> = self
                    .songs
                    .iter()
                    .filter(|s| {
                        query
                            .split_whitespace()
                            .all(|w| s.name.to_lowercase().contains(w))
                    })
                    .take(4)
                    .cloned()
                    .collect();
                if !songs.is_empty() {
                    rows.push(Row::Heading("Songs".into()));
                    rows.extend(songs.into_iter().map(Row::Song));
                }
            }
            return rows;
        }
        let keyed = |keys: &[String]| -> Vec<usize> {
            keys.iter()
                .filter_map(|k| c.device(k))
                .filter(|&i| fits(&c.devices[i]))
                .collect()
        };
        let in_song = self.in_song();
        let section = |rows: &mut Vec<Row>, title: &str, list: Vec<usize>| {
            if !list.is_empty() {
                rows.push(Row::Heading(title.into()));
                rows.extend(list.into_iter().map(Row::Device));
            }
        };
        if c.cat.is_none() {
            section(&mut rows, "Favourites", keyed(&c.favourites));
            section(&mut rows, "In this song", keyed(&in_song));
            section(
                &mut rows,
                "Used lately",
                keyed(&c.recent[..c.recent.len().min(6)]),
            );
        }
        // Then every device, by category.
        let cats: Vec<Cat> = match c.cat {
            Some(cat) => vec![cat],
            None => INSTRUMENT_CATS
                .iter()
                .chain(&[Cat::OtherInstrument])
                .chain(EFFECT_CATS.iter())
                .chain(&[Cat::OtherFx])
                .copied()
                .collect(),
        };
        for cat in cats {
            let mut list: Vec<usize> = (0..c.devices.len())
                .filter(|&i| c.devices[i].cat == cat && fits(&c.devices[i]))
                .collect();
            list.sort_by_key(|&i| (c.devices[i].own.is_none(), c.devices[i].name.to_lowercase()));
            section(&mut rows, cat.name(), list);
        }
        // A plugin's presets, under it.
        if let Some(key) = c.open.clone()
            && let Some(d) = self.catalog.device(&key)
        {
            let presets: Vec<Preset> = self.catalog.presets(d).to_vec();
            let mut out = Vec::new();
            for r in rows {
                let here = matches!(r, Row::Device(i) if i == d);
                out.push(r);
                if here {
                    out.extend(presets.iter().cloned().map(|p| Row::Preset(d, p)));
                    // Only under its first place.
                    if presets.is_empty() {
                        out.push(Row::Heading("No preset files for it".into()));
                    }
                }
            }
            return out;
        }
        rows
    }

    /// Samples in the library that a search's words find.
    pub(crate) fn catalog_samples(&self, query: &str, limit: usize) -> Vec<usize> {
        let q = pina_engine::samples::search::Query {
            text: query.to_string(),
            limit,
            ..Default::default()
        };
        pina_engine::samples::search::find(&self.browser.library.samples, &q)
            .into_iter()
            .map(|h| h.index)
            .collect()
    }
}
