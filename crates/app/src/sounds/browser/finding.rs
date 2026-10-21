//! Searching: the query from the search field and filters, the results, and alike sounds.

use super::*;

impl Workspace {
    /// The key the results keep to, if any.
    pub fn sample_key(&self) -> Option<Key> {
        match self.browser.key {
            KeyFilter::Any => None,
            KeyFilter::Song => self.session.project.song_key(),
            KeyFilter::Key(k) => Some(k),
        }
    }

    pub(crate) fn sample_bpm(&self) -> Option<f32> {
        self.browser
            .song_tempo
            .then_some(self.session.project.tempo as f32)
    }

    /// Whether the results follow the song's key and tempo.
    pub fn fits_song(&self) -> bool {
        self.browser.key == KeyFilter::Song && self.browser.song_tempo
    }

    pub fn sample_query(&self) -> Query {
        let b = &self.browser;
        Query {
            text: b.text.clone(),
            kind: b.kind,
            categories: b.categories.clone(),
            traits: b.traits.clone(),
            key: self.sample_key().map(|k| k.name()),
            bpm: self.sample_bpm(),
            limit: usize::MAX,
            ..Default::default()
        }
    }

    /// Finds `q`, in compatible keys too if they're asked for.
    pub(crate) fn find_keyed(&self, q: &Query) -> Vec<Hit> {
        let q = Query {
            compatible: self.browser.compatible,
            ..q.clone()
        };
        search::find(&self.browser.library.samples, &q)
    }

    /// Finds what the filters ask for, and how many each category and trait would give.
    pub fn find_samples(&mut self) {
        let q = self.sample_query();
        let pool = self.find_keyed(&Query {
            categories: Vec::new(),
            ..q.clone()
        });
        let samples = &self.browser.library.samples;
        let mut counts = HashMap::new();
        for h in &pool {
            *counts
                .entry(samples[h.index].analysis.category)
                .or_insert(0) += 1;
        }
        let mut hits = match q.categories.is_empty() {
            true => pool,
            false => self.find_keyed(&q),
        };
        if let Some(area) = &self.browser.area {
            hits.retain(|h| area.contains(&h.index));
        }
        let mut trait_counts = HashMap::new();
        for h in &hits {
            for t in &samples[h.index].analysis.traits {
                *trait_counts.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let b = &mut self.browser;
        b.hits = hits;
        b.counts = counts;
        b.trait_counts = trait_counts;
        b.seen = (self.session.project.song_key(), self.session.project.tempo);
        if b.view == Results::Kit {
            self.build_kit();
        }
    }

    /// What sounds like the selected sample, weighing each aspect as the sliders say.
    pub fn find_similar(&mut self) {
        let b = &self.browser;
        let Some(s) = b.selected.and_then(|i| b.sample(i)) else {
            self.browser.similar.clear();
            return;
        };
        let me = b.selected;
        self.browser.similar = search::similar(&b.library.samples, &s.path, b.aspects, 13)
            .into_iter()
            .filter(|h| Some(h.index) != me)
            .take(12)
            .collect();
    }

    /// A filter changed: find again, and keep the list at the top.
    pub fn refilter(&mut self, cx: &mut Context<Self>) {
        self.browser.like = None;
        self.find_samples();
        self.browser.list.scroll_to_item(0, ScrollStrategy::Top);
        self.browser
            .panel_list
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub fn set_sample_key(&mut self, key: KeyFilter, cx: &mut Context<Self>) {
        self.browser.key = key;
        self.refilter(cx);
    }

    /// Follows the song's key and tempo, or lets them go.
    pub fn set_fits_song(&mut self, on: bool, cx: &mut Context<Self>) {
        self.browser.key = if on { KeyFilter::Song } else { KeyFilter::Any };
        self.browser.song_tempo = on;
        self.refilter(cx);
    }

    pub fn toggle_category(&mut self, c: Category, cx: &mut Context<Self>) {
        let cats = &mut self.browser.categories;
        match cats.iter().position(|&x| x == c) {
            Some(i) => drop(cats.remove(i)),
            None => cats.push(c),
        }
        self.refilter(cx);
    }

    pub fn toggle_trait(&mut self, t: &str, cx: &mut Context<Self>) {
        let traits = &mut self.browser.traits;
        match traits.iter().position(|x| x == t) {
            Some(i) => drop(traits.remove(i)),
            None => traits.push(t.to_string()),
        }
        self.refilter(cx);
    }

    /// The key filter's menu: any key, the song's, or one picked; and whether related keys count.
    pub fn sample_key_menu(&self, at: Point<Pixels>) -> Menu {
        let b = &self.browser;
        let tick = |on: bool, label: String| match on {
            true => format!("✓ {label}"),
            false => label,
        };
        let song = match self.session.project.song_key() {
            Some(k) => format!("Song key ({})", k.name()),
            None => "Song key (none yet)".into(),
        };
        let mut items = vec![
            (
                tick(b.key == KeyFilter::Any, "Any key".into()),
                MenuAction::SampleKey(KeyFilter::Any),
            ),
            (
                tick(b.key == KeyFilter::Song, song),
                MenuAction::SampleKey(KeyFilter::Song),
            ),
        ];
        items.extend(Key::all().map(|k| {
            (
                tick(b.key == KeyFilter::Key(k), k.name()),
                MenuAction::SampleKey(KeyFilter::Key(k)),
            )
        }));
        let tail = (
            match b.compatible {
                true => "✓ Compatible keys too".to_string(),
                false => "    Compatible keys too".to_string(),
            },
            MenuAction::SampleCompatible(at),
        );
        Menu {
            at,
            items,
            columns: 2,
            lead: 2,
            tail: Some(tail),
            ..Default::default()
        }
    }

    /// Takes the words naming a category or trait out of the search.
    pub(crate) fn unsay(
        &mut self,
        c: Option<Category>,
        t: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        let text = search::unsay(&self.browser.text, c, t);
        self.browser
            .search
            .update(cx, |i, cx| i.set_text(&text, cx));
    }
}
