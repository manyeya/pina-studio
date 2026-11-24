//! Compose: bars of the song written by a composer model (MIDI-GPT and its kind), downloaded
//! from where its makers host it and run on this computer. The tracks picked in the headers are
//! written together, each hearing the others and the rest of the song, over the bars the cycle
//! loops. Takes come in one at a time, notes drawn on the lanes as they're written; the take
//! picked is heard in the song in place of what's there, and kept with one undo.

mod settings;
mod sheet;

pub use sheet::Act;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use futures::{StreamExt, channel::mpsc::unbounded};
use gpui::{AppContext, Context, Entity};
use pina_engine::{
    composer::{
        self, Installed,
        catalog::{KNOWN, Known},
        score::{self, Ask, Part},
        server,
    },
    song::project::{Content, Note, Project, Region},
};
use serde_json::Value;

use crate::{
    ui::text::TextInput,
    workspace::{Bottom, Workspace},
};

/// What takes are written for: the tracks, by id, and the bars (from 0, the first written, the
/// last not).
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub tracks: Vec<u64>,
    pub bars: (u32, u32),
}

/// A take: its seed (to write it again) and its notes, by track id, at their ticks in the song.
#[derive(Clone, Debug, PartialEq)]
pub struct Take {
    pub seed: i64,
    pub notes: Vec<(u64, Vec<Note>)>,
}

impl Take {
    pub fn count(&self) -> usize {
        self.notes.iter().map(|(_, n)| n.len()).sum()
    }
}

/// What a write is doing.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// Its family's Python being made, uv's last line.
    Python(String),
    /// The model loading.
    Starting,
    /// Take `take` (from 1) of `of`.
    Writing { take: u32, of: u32 },
}

/// A write going on.
pub struct Job {
    pub stage: Stage,
    pub started: Instant,
    pub stop: Arc<AtomicBool>,
    pub(crate) run: u32,
    pub tokens: u64,
    /// The take being written: its notes so far, by track id.
    pub live: Vec<(u64, Note)>,
}

/// The Compose tab and Settings › Composers: what's asked, what's going on, the takes.
pub struct ComposerUi {
    /// The model picked, by id: the first downloaded until one is.
    pub model: Option<&'static str>,
    /// How far from the likeliest note it may stray: 1 as the model learned, lower safer.
    pub temperature: f32,
    pub takes_n: u32,
    /// Melodic parts kept to the song's key.
    pub in_key: bool,
    /// What each track's part is heard as, by track id, when not as it sounds.
    pub parts: HashMap<u64, Part>,
    /// The model's own controls for each track, by track id and the control's name.
    pub steer: HashMap<u64, BTreeMap<String, i64>>,
    /// What each model said it can do, by id.
    pub info: HashMap<&'static str, Value>,
    pub job: Option<Job>,
    /// What the takes are for, the takes, and the one picked (heard in the song).
    pub target: Option<Target>,
    pub takes: Vec<Take>,
    pub picked: Option<usize>,
    /// A take kept: which, so the sheet says so.
    pub kept: Option<usize>,
    pub error: Option<String>,
    /// Each download going, by model id: the bytes come down so far, and its stop.
    pub downloads: HashMap<&'static str, (Arc<AtomicU64>, Arc<AtomicBool>)>,
    /// The Hugging Face token being typed in, and whether one's kept.
    pub token: Option<Entity<TextInput>>,
    pub signed_in: Option<bool>,
    /// The model whose server answered last time it was asked.
    pub up: Option<String>,
    run: u32,
}

impl Default for ComposerUi {
    fn default() -> Self {
        ComposerUi {
            model: None,
            temperature: 1.0,
            takes_n: 4,
            in_key: true,
            parts: HashMap::new(),
            steer: HashMap::new(),
            info: HashMap::new(),
            job: None,
            target: None,
            takes: Vec::new(),
            picked: None,
            kept: None,
            error: None,
            downloads: HashMap::new(),
            token: None,
            signed_in: None,
            up: None,
            run: 0,
        }
    }
}

impl ComposerUi {
    /// Whether something's under way (a download or a write): the window redraws while it is.
    pub fn working(&self) -> bool {
        self.job.is_some() || !self.downloads.is_empty()
    }

    /// Whether a Hugging Face token is kept (read once, then as it changes).
    pub fn signed_in(&mut self) -> bool {
        *self
            .signed_in
            .get_or_insert_with(|| composer::hf_token().is_some())
    }
}

/// What the background write says, as it goes.
enum Said {
    Line(String),
    Starting,
    Info(&'static str, Value),
    Take(u32, u32),
    Notes(u64, Vec<(u64, Note)>),
    Done(Take),
    Failed(String),
    Finished,
}

/// The models downloaded.
pub fn installed() -> Vec<Installed> {
    composer::installed(&composer::folder())
}

/// A take put in a song in place of what's in its bars on its tracks: a region of its own on
/// each, what was there making way (cut at its ends, so what's either side stays).
pub fn put_take(p: &mut Project, target: &Target, take: &Take) {
    let bar = p.bar().max(1);
    let (a, b) = (target.bars.0 as u64 * bar, target.bars.1 as u64 * bar);
    for (id, notes) in &take.notes {
        let Some(t) = p.tracks.iter().position(|tr| tr.id == *id) else {
            continue;
        };
        // Those across either end split there: what's either side stays, and what was in the
        // bars goes (not just hidden, as trimming would leave it).
        for edge in [b, a] {
            let across = p.tracks[t]
                .regions
                .iter()
                .position(|r| r.start < edge && r.start + r.length > edge);
            if let Some(i) = across {
                p.split_region((t, i), edge);
            }
        }
        let regions = &mut p.tracks[t].regions;
        regions.push(Region {
            name: "Composed".into(),
            start: a,
            length: b - a,
            content: Content::Midi {
                notes: notes
                    .iter()
                    .map(|n| Note {
                        start: n.start - a as i64,
                        ..n.clone()
                    })
                    .collect(),
                controls: Vec::new(),
            },
            ..Default::default()
        });
        let at = (t, regions.len() - 1);
        p.clear_under(at);
    }
}

impl Workspace {
    /// The tracks Compose writes: those picked in the headers, or the selected one; only those
    /// that play notes.
    pub fn compose_tracks(&self) -> Vec<usize> {
        let p = &self.session.project;
        self.picked_tracks()
            .into_iter()
            .filter(|&t| p.tracks[t].instrument.is_some())
            .collect()
    }

    /// The bars Compose writes, from 0 (the last not written), and where they come from: the
    /// cycle's, to whole bars; else the selected region's; else four from the playhead's bar.
    pub fn compose_bars(&self) -> ((u32, u32), &'static str) {
        let p = &self.session.project;
        let bar = p.bar().max(1);
        let region = self
            .region
            .filter(|&(t, r)| p.tracks.get(t).is_some_and(|t| r < t.regions.len()))
            .map(|at| p.region(at));
        let ((a, b), from) = match (p.cycle.filter(|c| c.end > c.start), region) {
            (Some(c), _) => ((c.start, c.end), "the loop"),
            (None, Some(r)) => ((r.start, r.start + r.length), "the region"),
            (None, None) => {
                let s = self.playhead() / bar * bar;
                ((s, s + 4 * bar), "the playhead")
            }
        };
        let b0 = (a / bar) as u32;
        ((b0, (b.div_ceil(bar) as u32).max(b0 + 1)), from)
    }

    /// The model Compose writes with: the one picked, if it's downloaded, else the first that is.
    pub fn compose_model(&self) -> Option<Installed> {
        let have = installed();
        have.iter()
            .find(|h| Some(h.known.id) == self.composer.model)
            .or(have.first())
            .cloned()
    }

    /// What `k` can do: as it said while running, or as kept from the last time.
    pub fn compose_info(&mut self, k: &'static Known) -> Option<Value> {
        if let Some(i) = self.composer.info.get(k.id) {
            return Some(i.clone());
        }
        let saved = composer::saved_info(k, &composer::folder())?;
        self.composer.info.insert(k.id, saved.clone());
        Some(saved)
    }

    /// Writes takes for the tracks and bars as they are now: more of them if it's what the takes
    /// there were written for, else a fresh set. Installs the model's Python first if it has to,
    /// and starts the model; each take's notes show on the lanes as they're written.
    pub fn compose_write(&mut self, cx: &mut Context<Self>) {
        if self.composer.job.is_some() {
            return;
        }
        let tracks = self.compose_tracks();
        if tracks.is_empty() {
            self.composer.error = Some(
                "Pick the tracks to write in the headers (⌘-click for several): tracks that play notes".into(),
            );
            return cx.notify();
        }
        let Some(m) = self.compose_model() else {
            self.composer.error = Some("Download a model first, in Settings › Composers".into());
            return cx.notify();
        };
        let (bars, _) = self.compose_bars();
        let p = &self.session.project;
        let target = Target {
            tracks: tracks.iter().map(|&t| p.tracks[t].id).collect(),
            bars,
        };
        if self.composer.target.as_ref() != Some(&target) {
            self.composer.takes.clear();
            self.composer.picked = None;
        }
        self.composer.kept = None;
        self.composer.error = None;
        self.composer.target = Some(target);
        let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
        let ask = Ask {
            tracks: tracks
                .iter()
                .map(|&t| (t, self.composer.parts.get(&ids[t]).copied()))
                .collect(),
            bars,
            temperature: self.composer.temperature,
            takes: self.composer.takes_n,
            seed: None,
            in_key: self.composer.in_key,
            attributes: BTreeMap::new(),
            steer: tracks
                .iter()
                .filter_map(|&t| Some((t, self.composer.steer.get(&ids[t])?.clone())))
                .collect(),
        };
        let project = self.session.project.clone();
        let device = match self.settings.compose_on_cpu {
            true => "cpu",
            false => "auto",
        };
        let stop = Arc::new(AtomicBool::new(false));
        self.composer.run += 1;
        let run = self.composer.run;
        let python = !server::ready(m.known.family, &composer::folder());
        self.composer.job = Some(Job {
            stage: match python {
                true => Stage::Python(format!("Getting {}'s Python", m.known.family.name())),
                false => Stage::Starting,
            },
            started: Instant::now(),
            stop: stop.clone(),
            run,
            tokens: 0,
            live: Vec::new(),
        });
        self.busy = Some((format!("Composing with {}", m.known.name), Instant::now()));
        let (tx, mut rx) = unbounded::<Said>();
        let n = self.composer.takes_n.clamp(1, 16);
        cx.background_spawn(async move {
            let say = |s: Said| {
                let _ = tx.unbounded_send(s);
            };
            let dir = composer::folder();
            let wrote = (|| -> Result<(), String> {
                if !server::ready(m.known.family, &dir) {
                    server::install(
                        m.known.family,
                        &dir,
                        &mut |l| say(Said::Line(l.into())),
                        &stop,
                    )?;
                }
                say(Said::Starting);
                let running = server::start(&m, &dir, device, &mut |_| {}, &stop)?;
                let info = server::info(&running.url)?;
                composer::save_info(m.known, &dir, &info);
                say(Said::Info(m.known.id, info.clone()));
                let prompt = score::prompt(&project, &ask, &info)?;
                // The ticks its notes come back in: not `info`'s resolution (the model's own), but
                // what its tokenizer writes them out at, learned from its answers (1920 for
                // MIDI-GPT).
                let mut info = info;
                let mut resolution = info["output_resolution"].as_u64().unwrap_or(1920) as u32;
                for k in 0..n {
                    say(Said::Take(k + 1, n));
                    let answer = server::generate(
                        &running.url,
                        &prompt.take(k, None),
                        &mut |p, new| {
                            let notes = score::written(&new, &prompt, resolution)
                                .into_iter()
                                .map(|(t, n)| (project.tracks[t].id, n))
                                .collect();
                            say(Said::Notes(p.tokens, notes));
                        },
                        &stop,
                    )?;
                    if let Some(r) = answer["score"]["resolution"]
                        .as_u64()
                        .filter(|&r| r as u32 != resolution && r > 0)
                    {
                        resolution = r as u32;
                        info["output_resolution"] = r.into();
                        composer::save_info(m.known, &dir, &info);
                    }
                    for t in score::takes(&answer, &prompt, &ask)? {
                        say(Said::Done(Take {
                            seed: t.seed,
                            notes: t
                                .notes
                                .into_iter()
                                .map(|(t, n)| (project.tracks[t].id, n))
                                .collect(),
                        }));
                    }
                }
                Ok(())
            })();
            say(match wrote {
                Ok(()) => Said::Finished,
                Err(e) => Said::Failed(e),
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while let Some(s) = rx.next().await {
                let end = matches!(s, Said::Finished | Said::Failed(_));
                let ours = this.update(cx, |ws, cx| ws.compose_said(run, s, cx));
                if end || !matches!(ours, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// What the write said, while it's still the one going (`run`).
    fn compose_said(&mut self, run: u32, s: Said, cx: &mut Context<Self>) -> bool {
        let Some(job) = self.composer.job.as_mut().filter(|j| j.run == run) else {
            return false;
        };
        match s {
            Said::Line(l) => job.stage = Stage::Python(l),
            Said::Starting => job.stage = Stage::Starting,
            Said::Info(id, info) => {
                self.composer.info.insert(id, info);
                self.composer.up = Some(id.to_string());
            }
            Said::Take(take, of) => {
                job.stage = Stage::Writing { take, of };
                job.live.clear();
                job.tokens = 0;
            }
            Said::Notes(tokens, notes) => {
                job.tokens = tokens;
                job.live.extend(notes);
            }
            Said::Done(take) => {
                job.live.clear();
                self.composer.takes.push(take);
                // The first take in is picked, so it's heard straight away.
                if self.composer.picked.is_none() {
                    self.composer.picked = Some(self.composer.takes.len() - 1);
                    self.heard_changed(cx);
                }
            }
            Said::Failed(e) => {
                self.composer.job = None;
                self.busy = None;
                self.composer.error = (e != "Stopped").then_some(e);
            }
            Said::Finished => {
                self.composer.job = None;
                self.busy = None;
                let n = self.composer.takes.len();
                self.status = Some(format!(
                    "{n} take{} written · Space hears the one picked in the song",
                    if n == 1 { "" } else { "s" }
                ));
            }
        }
        cx.notify();
        true
    }

    /// Stops the write going: the takes in so far stay.
    pub fn compose_stop(&mut self, cx: &mut Context<Self>) {
        if let Some(j) = &self.composer.job {
            j.stop.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    /// Take `k` picked: heard in the song in place of what's in its bars.
    pub fn compose_pick(&mut self, k: usize, cx: &mut Context<Self>) {
        if k < self.composer.takes.len() {
            self.composer.picked = Some(k);
            self.heard_changed(cx);
        }
    }

    /// Take `k` heard from its first bar.
    pub fn compose_hear(&mut self, k: usize, cx: &mut Context<Self>) {
        self.compose_pick(k, cx);
        let Some(t) = &self.composer.target else {
            return;
        };
        let at = t.bars.0 as u64 * self.session.project.bar();
        self.locate_tick(at, cx);
        if !self.player.playing() {
            self.play_stop(cx);
        }
    }

    /// The take picked put in the song, for good (one undo takes it back).
    pub fn compose_keep(&mut self, cx: &mut Context<Self>) {
        let (Some(k), Some(target)) = (self.composer.picked, self.composer.target.clone()) else {
            return;
        };
        let Some(take) = self.composer.takes.get(k).cloned() else {
            return;
        };
        self.composer.picked = None;
        self.edit(cx, |ws| put_take(&mut ws.session.project, &target, &take));
        self.composer.kept = Some(k);
        self.status = Some(format!("Take {} kept · ⌘Z takes it back", k + 1));
    }

    /// The takes let go: the song plays as it is again.
    pub fn compose_clear(&mut self, cx: &mut Context<Self>) {
        self.composer.takes.clear();
        self.composer.picked = None;
        self.composer.kept = None;
        self.composer.target = None;
        self.heard_changed(cx);
    }

    /// The take picked as the engine hears it: in place of what's in its bars, while Compose is
    /// open (closed, the song plays as it is).
    pub(crate) fn compose_preview(&self, p: &mut Project) -> bool {
        let c = &self.composer;
        let (Some(k), Some(target)) = (c.picked, &c.target) else {
            return false;
        };
        if self.bottom != Some(Bottom::Compose) {
            return false;
        }
        let Some(take) = c.takes.get(k) else {
            return false;
        };
        put_take(p, target, take);
        true
    }

    /// The engine told what it plays now: a take picked, or let go.
    pub(crate) fn heard_changed(&mut self, cx: &mut Context<Self>) {
        let project = self.engine_project().into_owned();
        self.status = self.player.sync(&project).err().or(self.status.take());
        cx.notify();
    }

    /// A model downloaded from where its makers host it, with the person's token where they ask
    /// for one; `downloads` says how far it's got.
    pub fn compose_download(&mut self, k: &'static Known, cx: &mut Context<Self>) {
        if self.composer.downloads.contains_key(k.id) {
            return;
        }
        let (done, stop) = (Arc::<AtomicU64>::default(), Arc::<AtomicBool>::default());
        self.composer
            .downloads
            .insert(k.id, (done.clone(), stop.clone()));
        self.composer.error = None;
        let job = cx.background_spawn(async move {
            let token = composer::hf_token();
            composer::catalog::download(
                k,
                &composer::folder(),
                token.as_deref(),
                |b| done.store(b, Ordering::Relaxed),
                &stop,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |ws, cx| {
                ws.composer.downloads.remove(k.id);
                match result {
                    Ok(_) => {
                        ws.composer.model = Some(k.id);
                        ws.status = Some(format!(
                            "{} downloaded: pick tracks and bars, then Compose writes them",
                            k.name
                        ));
                    }
                    Err(e) if e == "Stopped" => ws.status = Some("Download stopped".into()),
                    Err(e) => ws.composer.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// A downloaded model deleted, its server stopped first.
    pub fn compose_remove(&mut self, k: &'static Known, cx: &mut Context<Self>) {
        let dir = composer::folder();
        server::stop_server(k.family, &dir);
        if let Err(e) = composer::remove(k, &dir) {
            self.composer.error = Some(e);
        }
        self.composer.info.remove(k.id);
        if self.composer.model == Some(k.id) {
            self.composer.model = None;
        }
        self.composer.up = None;
        cx.notify();
    }

    /// The token typed in, kept where `hf auth login` keeps it.
    pub fn compose_save_token(&mut self, token: &str, cx: &mut Context<Self>) {
        let token = token.trim();
        if token.is_empty() {
            return;
        }
        match composer::save_hf_token(token) {
            Ok(()) => {
                self.composer.signed_in = Some(true);
                self.composer.error = None;
                self.status = Some("Signed in to Hugging Face".into());
            }
            Err(e) => self.composer.error = Some(e),
        }
        if let Some(t) = &self.composer.token {
            t.update(cx, |t, cx| t.set_text("", cx));
        }
        cx.notify();
    }

    /// Whether a model's server still answers, asked in the background: Settings says.
    pub fn compose_check_up(&mut self, cx: &mut Context<Self>) {
        let job = cx.background_spawn(async move {
            let dir = composer::folder();
            KNOWN
                .iter()
                .map(|k| k.family)
                .find_map(|f| server::noted_server(f, &dir))
                .filter(|(_, url, _)| server::healthy(url))
                .map(|(model, ..)| model)
        });
        cx.spawn(async move |this, cx| {
            let up = job.await;
            let _ = this.update(cx, |ws, cx| {
                ws.composer.up = up;
                cx.notify();
            });
        })
        .detach();
    }

    /// A model's server stopped now, rather than when it's been idle ten minutes.
    pub fn compose_stop_server(&mut self, cx: &mut Context<Self>) {
        let dir = composer::folder();
        for k in KNOWN {
            server::stop_server(k.family, &dir);
        }
        self.composer.up = None;
        cx.notify();
    }
}

/// How long something's taken, as the sheet says it: 0:07.
pub fn elapsed(since: Instant) -> String {
    let s = since.elapsed().as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}
