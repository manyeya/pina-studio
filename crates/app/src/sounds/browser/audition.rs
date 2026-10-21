//! Hearing samples: auditioned in the song's key and tempo, stepped through, liked.

use super::*;

impl Workspace {
    /// Semitones to move a preview by: into the key the results keep to (else the song's) with
    /// Play in Key on, as the search worked it out or else from the sample's note or key.
    pub fn sample_transpose(&self, index: usize) -> f32 {
        let b = &self.browser;
        if !b.play_in_key {
            return 0.0;
        }
        let Some(s) = b.sample(index) else {
            return 0.0;
        };
        if let Some(t) = b.hit(index).and_then(|h| h.transpose) {
            let cents = s.analysis.pitch.map_or(0.0, |p| p.cents);
            return t as f32 - cents / 100.0;
        }
        match self
            .sample_key()
            .or_else(|| self.session.project.song_key())
        {
            Some(key) => transpose_to(&s.analysis, key),
            None => 0.0,
        }
    }

    /// How much longer a loop plays at the song's tempo (1 for a one-shot, or a loop without one).
    pub(crate) fn preview_stretch(&self, a: &Analysis) -> f64 {
        let tempo = self.session.project.tempo;
        match a.kind {
            Kind::Loop => loop_ticks(a, tempo)
                .map(|t| t as f64 / PPQ as f64 * 60.0 / tempo / a.seconds.max(0.01) as f64)
                .filter(|r| (r - 1.0).abs() > 0.002)
                .unwrap_or(1.0),
            Kind::OneShot => 1.0,
        }
    }

    /// Plays a sample over the mix as it would be in the song: a loop at its tempo, in its key
    /// with Play in Key on (both by the same stretch the song uses); and while the song plays, a
    /// loop in time with it.
    pub fn audition_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(s) = self.browser.sample(index) else {
            return;
        };
        let (path, a) = (s.path.clone(), &s.analysis);
        let semis = self.sample_transpose(index);
        let gain = match a.loudness {
            l if (-60.0..0.0).contains(&l) => (PREVIEW_LUFS - l).clamp(-12.0, 6.0),
            _ => 0.0,
        };
        let ratio = self.preview_stretch(a);
        let vocal = a.category == Category::Vocal;
        let synced = a.kind == Kind::Loop && a.bpm.is_some();
        let seconds = a.seconds * ratio as f32;
        self.browser.wanted = Some(index);
        let key = (path.clone(), ratio.to_bits(), semis.to_bits());
        if let Some(data) = self.browser.heard.get(&key).cloned() {
            self.play_heard(index, data, synced, gain, seconds, cx);
            return;
        }
        // Read and stretched off the UI thread; played when it's ready, if it's still wanted.
        let rate = self.player.rate();
        let raw = self
            .browser
            .heard
            .get(&(path.clone(), 1f64.to_bits(), 0f32.to_bits()))
            .cloned();
        let made = cx.background_spawn({
            let path = path.clone();
            async move {
                let raw = match raw {
                    Some(raw) => raw,
                    None => Arc::new(pina_engine::audio::engine::load_audio(&path, rate)?),
                };
                let heard = match ratio == 1.0 && semis == 0.0 {
                    true => raw.clone(),
                    // A voice keeps its formants, moved into the key.
                    false => Arc::new(stretch::render(
                        &raw,
                        &stretch::Fit {
                            semitones: semis as f64,
                            formants: vocal,
                            ..stretch::Fit::ratio(ratio)
                        },
                        rate,
                    )),
                };
                Ok::<_, String>((raw, heard))
            }
        });
        cx.spawn(async move |ws, cx| {
            let made = made.await;
            ws.update(cx, |ws, cx| match made {
                Ok((raw, data)) => {
                    // ponytail: forgets everything past 64 files; an LRU if browsing gets big.
                    if ws.browser.heard.len() >= 64 {
                        ws.browser.heard.clear();
                    }
                    ws.browser
                        .heard
                        .insert((path, 1f64.to_bits(), 0f32.to_bits()), raw);
                    ws.browser.heard.insert(key, data.clone());
                    if ws.browser.wanted == Some(index) {
                        ws.play_heard(index, data, synced, gain, seconds, cx);
                    }
                }
                Err(e) => {
                    ws.status = Some(e);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn play_heard(
        &mut self,
        index: usize,
        data: Arc<pina_engine::audio::engine::AudioData>,
        synced: bool,
        gain: f32,
        seconds: f32,
        cx: &mut Context<Self>,
    ) {
        // In time with the song, it starts where the song is in it.
        let looping = synced && self.player.playing();
        let since = match looping {
            true => {
                let len = data.channels()[0].len().max(1) as u64;
                let into = (self.player.position() % len) as f64 / self.player.rate();
                Instant::now() - Duration::from_secs_f64(into)
            }
            false => Instant::now(),
        };
        let played = match synced {
            true => self.player.audition_synced(data, gain),
            false => self.player.audition_audio(data, 0.0, gain),
        };
        match played {
            Ok(()) => {
                self.browser.audition = Some((index, since, seconds));
                self.browser.looping = looping;
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    pub fn stop_audition(&mut self, cx: &mut Context<Self>) {
        self.player.stop_audition();
        self.browser.audition = None;
        cx.notify();
    }

    /// Selects a sample (and hears it): its details and what sounds like it show.
    pub fn select_sample(&mut self, index: usize, hear: bool, cx: &mut Context<Self>) {
        self.browser.selected = Some(index);
        self.find_similar();
        if hear {
            self.audition_sample(index, cx);
        }
        cx.notify();
    }

    /// Up and down: the next sample in the results, heard as it's picked.
    pub fn step_sample(&mut self, by: i32, cx: &mut Context<Self>) {
        let b = &self.browser;
        let panel = self.view == View::Arrange;
        let order: Vec<usize> = match (&b.like, b.view) {
            (Some((_, like)), _) if panel => like.iter().map(|h| h.index).collect(),
            (_, Results::Kit) if !panel => b
                .kit
                .iter()
                .filter_map(|k| k.hit().map(|h| h.index))
                .collect(),
            _ => b.hits.iter().map(|h| h.index).collect(),
        };
        if order.is_empty() {
            return;
        }
        let at = b.selected.and_then(|i| order.iter().position(|&o| o == i));
        let next = match at {
            Some(at) => (at as i32 + by).clamp(0, order.len() as i32 - 1) as usize,
            None if by > 0 => 0,
            None => order.len() - 1,
        };
        if panel {
            b.panel_list.scroll_to_item(next, ScrollStrategy::Nearest);
        } else if b.view != Results::Kit {
            b.list.scroll_to_item(next, ScrollStrategy::Nearest);
        }
        self.select_sample(order[next], true, cx);
    }

    /// The compact list shows what sounds like a sample, or (again) the results.
    pub fn like_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let b = &self.browser;
        let like = match (&b.like, b.sample(index)) {
            (Some((i, _)), _) if *i == index => None,
            (_, None) => None,
            (_, Some(s)) => Some((
                index,
                search::similar(&b.library.samples, &s.path, b.aspects, 31)
                    .into_iter()
                    .filter(|h| h.index != index)
                    .take(30)
                    .collect(),
            )),
        };
        self.browser.like = like;
        self.browser
            .panel_list
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// How far through its preview a sample is, 0 to 1, while it's heard.
    pub(crate) fn heard(&self, index: usize) -> Option<f32> {
        let (_, since, seconds) = self.browser.audition.filter(|a| a.0 == index)?;
        Some((since.elapsed().as_secs_f32() / seconds.max(0.01)).min(1.0))
    }
}
