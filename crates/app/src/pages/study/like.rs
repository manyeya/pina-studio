//! Songs like a reference, on the Song Study page: a spectrogram picture studied and read (its
//! plot dragged out, its axes and colours given), any study's blueprint, a new song set up to be
//! like it for the agent to write, and the song compared with it, section by section, with
//! revisions asked for in words: the agent revises, Pina compares again, and it stops once the
//! request is met or a pass no longer helps (three at most). Each pass is one of the agent's
//! turns, so undo takes it back.

use std::{cell::Cell, collections::HashMap, rc::Rc};

use gpui::Entity;
use pina_engine::study::reference::{
    Blueprint, Calibration, Closeness, Colors, FreqScale, Source,
    compare::{self, Report, Target, Verdict},
};
use serde_json::Value;

use super::*;
use crate::{cli::reference, ui::text::TextInput};

/// The most passes a revision takes.
pub const PASSES: u8 = 3;

#[derive(Default)]
pub struct Likes {
    /// Pictures of spectrograms studied, the latest first.
    pub pictures: Vec<Blueprint>,
    /// The audio studies' blueprints, worked out as they're shown; those being worked out.
    pub blueprints: HashMap<PathBuf, Blueprint>,
    pub making: Vec<PathBuf>,
    /// The open picture's reading, being changed.
    pub reading: Option<Reading>,
    /// Create a song like this, open.
    pub create: Option<Create>,
    /// The song's last comparison with its reference.
    pub compared: Option<Compared>,
    pub comparing: bool,
    pub revising: Option<Revision>,
    /// What the person asks to change, in words.
    pub ask: Option<Entity<TextInput>>,
}

/// A picture's reading as the person sets it: how long, its frequencies and how they run, its
/// colours, its levels' span, and its plot (dragged out on the picture).
pub struct Reading {
    pub file: PathBuf,
    pub size: (u32, u32),
    pub seconds: Entity<TextInput>,
    pub low: Entity<TextInput>,
    pub high: Entity<TextInput>,
    pub db: Entity<TextInput>,
    pub scale: Option<FreqScale>,
    pub colors: Option<Colors>,
    pub crop: Option<[u32; 4]>,
    /// Dragging out a plot: where it started, in the picture's pixels.
    pub dragging: Option<(f32, f32)>,
    /// Where the picture was drawn.
    pub drawn: Rc<Cell<Option<Bounds<Pixels>>>>,
}

pub struct Create {
    pub file: PathBuf,
    pub style: Entity<TextInput>,
    pub rhythm: Closeness,
    pub form: Closeness,
    pub sound: Closeness,
}

/// A song compared with its reference: which song (its folder), what with, and how it went.
pub struct Compared {
    pub dir: PathBuf,
    pub reference: PathBuf,
    pub report: Report,
    pub song: Blueprint,
    /// As agents read it: bars and tracks.
    pub json: Value,
}

/// A revision under way: what was asked, which pass, what it aims at, the comparison before it,
/// and how each pass went.
pub struct Revision {
    /// The song being revised (its folder).
    pub dir: PathBuf,
    pub request: String,
    pub pass: u8,
    pub target: Target,
    pub before: Report,
    pub history: Vec<compare::Progress>,
    /// The agent's at work on this pass.
    pub waiting: bool,
    /// Why it stopped short, if it did.
    pub stopped: Option<String>,
}

fn number(e: &Entity<TextInput>, cx: &App) -> Option<f32> {
    parse_number(e.read(cx).text())
}

/// A number as people type one: "180", "180 s", "60 dB", "16k" or "16 kHz" (thousands).
fn parse_number(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase();
    let t = t.trim_end_matches(['s', 'z', 'h', 'b', 'd']).trim_end();
    let (t, by) = match t.strip_suffix('k') {
        Some(t) => (t.trim_end(), 1000.0),
        None => (t, 1.0),
    };
    t.parse::<f32>().ok().map(|v| v * by)
}

impl Workspace {
    /// The picture shown, if a spectrogram's is.
    pub(crate) fn opened_picture(&self) -> Option<Blueprint> {
        let f = self.studies.open.as_ref()?;
        self.studies
            .like
            .pictures
            .iter()
            .find(|b| b.file == *f)
            .cloned()
    }

    /// Studies a picture of a spectrogram in the background, as it reads on its own (or as last
    /// read), and shows it.
    pub(crate) fn study_picture(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.studies.busy.contains(&path) {
            return;
        }
        self.studies.busy.push(path.clone());
        self.status = Some(format!("Reading {}…", file_name(&path)));
        cx.notify();
        self.read_picture(path, Calibration::default(), false, cx);
    }

    /// A picture read as `given` says, in the background (`fresh`: afresh, not as last kept):
    /// kept, shown, its reading's fields set.
    pub(crate) fn read_picture(
        &mut self,
        path: PathBuf,
        given: Calibration,
        fresh: bool,
        cx: &mut Context<Self>,
    ) {
        let job = {
            let path = path.clone();
            cx.background_spawn(async move {
                match fresh {
                    true => reference::reread(&path, &given),
                    false => reference::blueprint(&path, &given),
                }
            })
        };
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |ws, cx| {
                ws.studies.busy.retain(|p| *p != path);
                match result {
                    Ok(bp) => {
                        ws.status = Some(match bp.needs.is_empty() {
                            true => format!("Read {}", file_name(&path)),
                            false => {
                                format!("Read {}: give its axes to say more", file_name(&path))
                            }
                        });
                        ws.studies.like.pictures.retain(|b| b.file != bp.file);
                        ws.studies.open = Some(bp.file.clone());
                        ws.studies.pick = Pick::File;
                        ws.set_reading(&bp, cx);
                        ws.studies.like.pictures.insert(0, bp);
                    }
                    Err(e) => ws.status = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The reading's fields, as a blueprint was read.
    pub(crate) fn set_reading(&mut self, bp: &Blueprint, cx: &mut Context<Self>) {
        let c = bp.calibration.clone().unwrap_or_default();
        let field = |v: Option<f32>, hint: &str, cx: &mut Context<Self>| {
            let text = v.map_or(String::new(), |v| format!("{v}"));
            cx.new(|cx| TextInput::form(&text, hint, cx))
        };
        self.studies.like.reading = Some(Reading {
            file: bp.file.clone(),
            size: pina_engine::study::reference::image::size(&bp.file).unwrap_or((1, 1)),
            seconds: field(c.seconds, "seconds", cx),
            low: field(c.low_hz, "lowest Hz", cx),
            high: field(c.high_hz, "highest Hz", cx),
            db: field(c.db_range, "dB span", cx),
            scale: c.scale,
            colors: c.colors,
            crop: c.crop,
            dragging: None,
            drawn: Rc::default(),
        });
    }

    /// The open picture read again as its fields say.
    pub(crate) fn reread_picture(&mut self, cx: &mut Context<Self>) {
        let Some(r) = &self.studies.like.reading else {
            return;
        };
        let given = Calibration {
            crop: r.crop,
            seconds: number(&r.seconds, cx),
            low_hz: number(&r.low, cx),
            high_hz: number(&r.high, cx),
            scale: r.scale,
            colors: r.colors,
            db_range: number(&r.db, cx),
        };
        let file = r.file.clone();
        self.studies.busy.push(file.clone());
        self.status = Some(format!("Reading {} again…", file_name(&file)));
        cx.notify();
        self.read_picture(file, given, true, cx);
    }

    /// A study's blueprint: kept, or worked out in the background (shown once it's ready).
    pub(crate) fn study_blueprint(
        &mut self,
        file: &Path,
        cx: &mut Context<Self>,
    ) -> Option<Blueprint> {
        if let Some(bp) = self.studies.like.blueprints.get(file) {
            return Some(bp.clone());
        }
        if self.studies.like.making.iter().any(|f| f == file) {
            return None;
        }
        self.studies.like.making.push(file.to_path_buf());
        let path = file.to_path_buf();
        let job = {
            let path = path.clone();
            cx.background_spawn(async move { reference::blueprint(&path, &Calibration::default()) })
        };
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |ws, cx| {
                match result {
                    Ok(bp) => {
                        ws.studies.like.making.retain(|f| *f != path);
                        ws.studies.like.blueprints.insert(path, bp);
                    }
                    // Left as being made: a file that can't be read isn't tried again each frame.
                    Err(e) => ws.status = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    pub(crate) fn forget_picture(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        reference::forget_picture(&file);
        self.studies.like.pictures.retain(|b| b.file != file);
        if self.studies.open.as_ref() == Some(&file) {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
        }
        cx.notify();
    }

    /// Create a song like this, open on a reference.
    pub(crate) fn open_create(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        let style = cx.new(|cx| {
            TextInput::form(
                "",
                "Its style, in your words: \"darker, half-time, live drums\"",
                cx,
            )
        });
        self.studies.like.create = Some(Create {
            file,
            style,
            rhythm: Closeness::Close,
            form: Closeness::Close,
            sound: Closeness::Close,
        });
        cx.notify();
    }

    /// A new song set up to be like the reference, opened, and the agent asked to write it.
    pub(crate) fn create_like(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.studies.like.create.take() else {
            return;
        };
        let given = match self.studies.like.reading.as_ref() {
            Some(r) if r.file == c.file => self
                .studies
                .like
                .pictures
                .iter()
                .find(|b| b.file == c.file)
                .and_then(|b| b.calibration.clone())
                .unwrap_or_default(),
            _ => Calibration::default(),
        };
        let choices = reference::Choices {
            title: None,
            style: c.style.read(cx).text().trim().to_string(),
            rhythm: c.rhythm,
            form: c.form,
            sound: c.sound,
        };
        let file = c.file.clone();
        self.status = Some(format!("Setting up a song like {}…", file_name(&file)));
        cx.notify();
        let root = crate::pages::projects::root();
        let job =
            cx.background_spawn(async move { reference::create(&file, &given, &choices, &root) });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            this.update_in(cx, |ws, window, cx| match result {
                Ok((dir, bp)) => {
                    ws.open_song(dir.clone(), window, cx);
                    let Some(like) = ws.session.project.like.clone() else {
                        return;
                    };
                    ws.view = View::Arrange;
                    ws.show_browser = false;
                    ws.show_agent = true;
                    ws.ask_agent(reference::compose_prompt(&dir, &like, &bp), cx);
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

    /// The song rendered and compared with its reference, in the background; then a revision
    /// under way is judged.
    pub(crate) fn compare_like(&mut self, cx: &mut Context<Self>) {
        if self.studies.like.comparing {
            return;
        }
        if self.session.project.like.is_none() {
            self.status = Some("This song isn't set up to be like a reference.".into());
            cx.notify();
            return;
        }
        let (p, dir) = (self.session.project.clone(), self.session.dir.clone());
        self.studies.like.comparing = true;
        self.status = Some("Rendering the song to compare it with its reference…".into());
        cx.notify();
        let job = cx.background_spawn(async move {
            let (report, reference, song) = reference::report(&p, &dir)?;
            let json = reference::describe(&report, &reference, &song, &p);
            Ok::<_, String>(Compared {
                dir,
                reference: reference.file,
                report,
                song,
                json,
            })
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |ws, cx| {
                ws.studies.like.comparing = false;
                match result {
                    Ok(c) => {
                        ws.status = Some(format!("Compared with {}", file_name(&c.reference)));
                        ws.note_comparison(&c);
                        ws.studies.like.compared = Some(c);
                        ws.judge_pass(cx);
                    }
                    Err(e) => {
                        ws.status = Some(e);
                        ws.studies.like.revising = None;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The comparison's biggest differences, noted in the agent's conversation too.
    fn note_comparison(&mut self, c: &Compared) {
        let o = &c.report.overall;
        let pct = |v: Option<f32>| v.map_or("–".into(), |v| format!("{:.0}%", v * 100.0));
        let mut lines = vec![format!(
            "Groove {} · notes {} · balance {} · energy {}",
            pct(o.groove),
            pct(o.tonal),
            pct(o.balance),
            pct(o.energy)
        )];
        lines.extend(c.report.differences.iter().take(3).map(|d| d.says.clone()));
        self.chat.compared(&file_name(&c.reference), lines);
    }

    /// The song's own comparison, if it's been compared (not another song's, like the same
    /// reference).
    pub(crate) fn song_compared(&self) -> Option<&Compared> {
        self.studies
            .like
            .compared
            .as_ref()
            .filter(|c| c.dir == self.session.dir)
    }

    /// A revision asked for in words: the agent revises what it aims at (`aim`, or what the
    /// words name), then it's compared.
    pub(crate) fn revise_like(
        &mut self,
        request: String,
        aim: Option<Target>,
        cx: &mut Context<Self>,
    ) {
        let request = request.trim().to_string();
        if request.is_empty() {
            return;
        }
        let Some(c) = self.song_compared() else {
            self.status = Some("Compare the song with its reference first.".into());
            cx.notify();
            return;
        };
        let report = c.report.clone();
        self.studies.like.revising = Some(Revision {
            dir: self.session.dir.clone(),
            target: aim.unwrap_or_else(|| compare::target(&request, &report)),
            request: request.clone(),
            pass: 1,
            before: report,
            history: Vec::new(),
            waiting: true,
            stopped: None,
        });
        if let Some(ask) = &self.studies.like.ask {
            ask.update(cx, |t, cx| t.set_text("", cx));
        }
        self.ask_revision(cx);
    }

    /// The agent asked for the revision's next pass.
    fn ask_revision(&mut self, cx: &mut Context<Self>) {
        let (Some(r), Some(c)) = (&self.studies.like.revising, self.song_compared()) else {
            return;
        };
        let prompt = reference::revise_prompt(
            &self.session.dir,
            &self.session.project,
            &r.request,
            &r.target,
            &c.report,
            &c.song,
        );
        self.status = Some(format!(
            "Revising: {} (pass {} of {PASSES})",
            r.request, r.pass
        ));
        self.ask_agent(prompt, cx);
    }

    /// The agent couldn't be asked: a revision waiting on it stops.
    pub(crate) fn agent_unavailable(&mut self, _cx: &mut Context<Self>) {
        if let Some(r) = &mut self.studies.like.revising
            && r.waiting
        {
            (r.waiting, r.pass) = (false, PASSES + 1);
            r.stopped = Some("the Studio Agent couldn't be asked".into());
            self.status = Some("The revision stopped: the Studio Agent couldn't be asked.".into());
        }
    }

    /// The agent's turn is over: a revision waiting on it is compared.
    pub(crate) fn agent_turn_over(&mut self, cx: &mut Context<Self>) {
        // Another song open since: its turn isn't this revision's.
        if self
            .studies
            .like
            .revising
            .as_ref()
            .is_some_and(|r| r.dir != self.session.dir)
        {
            self.studies.like.revising = None;
            return;
        }
        if let Some(r) = &mut self.studies.like.revising
            && r.waiting
        {
            r.waiting = false;
            self.compare_like(cx);
        }
    }

    /// A pass judged against the comparison before it: reached or stalled ends the revision;
    /// better asks for another pass, up to `PASSES`.
    fn judge_pass(&mut self, cx: &mut Context<Self>) {
        let (Some(r), Some(c)) = (&mut self.studies.like.revising, &self.studies.like.compared)
        else {
            return;
        };
        if r.waiting || r.dir != c.dir {
            return;
        }
        let p = compare::progress(&r.before, &c.report, &r.target);
        r.history.push(p.clone());
        let pct = |v: Option<f32>| v.map_or("–".into(), |v| format!("{:.0}%", v * 100.0));
        let moved = format!("{} → {}", pct(p.before), pct(p.after));
        let request = r.request.clone();
        match p.verdict {
            Verdict::Better if r.pass < PASSES => {
                r.pass += 1;
                r.before = c.report.clone();
                r.waiting = true;
                self.ask_revision(cx);
            }
            v => {
                self.status = Some(match v {
                    Verdict::Reached => format!("\"{request}\": done ({moved})"),
                    Verdict::Better => {
                        format!("\"{request}\": better after {PASSES} passes ({moved})")
                    }
                    Verdict::Stalled if p.others < -0.03 => format!(
                        "\"{request}\": no better ({moved}), and the rest got worse: undo the last turn to go back"
                    ),
                    Verdict::Stalled => format!("\"{request}\": no better ({moved}): stopped"),
                });
                // Kept to show how it went; no longer waiting on anything.
                if let Some(r) = &mut self.studies.like.revising {
                    r.pass = PASSES + 1;
                }
            }
        }
    }

    /// The reference this song is like, shown on the Song Study page (studied first if it isn't
    /// yet), with the song against it.
    pub(crate) fn show_like_reference(&mut self, cx: &mut Context<Self>) {
        let Some(like) = self.session.project.like.clone() else {
            return;
        };
        if self.studies.busy.contains(&like.file) {
            return;
        }
        self.studies.dock = Dock::Compare;
        match like.source {
            Source::Image => {
                if !self
                    .studies
                    .like
                    .pictures
                    .iter()
                    .any(|b| b.file == like.file)
                {
                    self.read_picture(
                        like.file.clone(),
                        like.calibration.clone().unwrap_or_default(),
                        false,
                        cx,
                    );
                }
            }
            _ => {
                if !self.studies.list.iter().any(|s| s.file == like.file) {
                    self.study_file(like.file.clone(), cx);
                }
            }
        }
        self.studies.open = Some(like.file);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_read_as_people_type_them() {
        let read = super::parse_number;
        assert_eq!(read("180"), Some(180.0));
        assert_eq!(read(" 180 s "), Some(180.0));
        assert_eq!(read("16k"), Some(16_000.0));
        assert_eq!(read("16 kHz"), Some(16_000.0));
        assert_eq!(read("22.05kHz"), Some(22_050.0));
        assert_eq!(read("40 Hz"), Some(40.0));
        assert_eq!(read("60 dB"), Some(60.0));
        assert_eq!(read(""), None);
        assert_eq!(read("long"), None);
    }
}
