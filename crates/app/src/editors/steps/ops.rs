//! What the step sequencer's menus and buttons do: one op each, and the menus that offer them
//! (Edit, Functions, View, adding and assigning rows, rates, playback modes, lengths, a row's
//! and a step's own).

use gpui::{Pixels, Point};
use pina_engine::song::project::{
    Condition, RATES, Randomize, Scale, SortRows, StepCopy, paste_row, paste_row_settings,
    rate_name,
};

use super::*;

/// One thing the step sequencer does, from a menu, a button or a key.
#[derive(Clone, Debug, PartialEq)]
pub enum StepsOp {
    Mode(EditMode),
    ShowValues(ShowValues),
    ShowColors,
    Sort(SortRows),
    SortDescending(bool),
    Pane(Option<Pane>),
    // The pattern.
    Length(usize),
    PatternRate(u64),
    PatternPlay(PlayMode),
    SwingEighths(bool),
    Legato(bool),
    Key(u8, bool),
    Scale(Option<Scale>),
    Conform,
    Transpose(i32),
    RotatePattern(i32),
    StepPattern(bool),
    RandomizePattern(Option<bool>),
    ClearPatternValues(Option<bool>),
    ClearPattern,
    // Rows.
    AddNote(u8),
    AddMelodic,
    AddAutomation(RowTarget),
    Learn(Option<Learn>),
    AssignNote(usize, u8),
    AssignMelodic(usize),
    AssignAutomation(usize, RowTarget),
    RowRate(usize, Option<u64>),
    RowPlay(usize, Option<PlayMode>),
    RowLoop(usize, usize, usize),
    RowColor(usize, Option<u32>),
    Slide(usize, bool),
    Mute(usize),
    Solo(usize),
    Rename(usize),
    CopyRow(usize),
    PasteRow(usize),
    PasteToNewRow,
    PasteRowSettings(usize),
    PasteSteps(usize),
    DuplicateRow(usize),
    DeleteRow(usize),
    DeleteUnused,
    ClearRow(usize),
    RotateRow(usize, Option<Lane>, i32),
    StepRow(usize, Option<Lane>, bool),
    RandomizeRow(usize, Option<Lane>),
    ClearRowValues(usize, Option<Lane>),
    SubrowLane(usize, usize, Lane),
    AddSubrow(usize, usize),
    DeleteSubrow(usize, usize),
    // Steps.
    CutSteps,
    CopySteps,
    PasteStepsAt(usize, usize),
    RepeatStep,
    ClearStep,
    StepCondition(usize, usize, Condition),
    StepRate(usize, usize, Option<u64>),
    // Pages.
    CutPage(usize),
    CopyPage(usize),
    PastePage(usize),
    ClearPage(usize),
    // Recording and templates.
    LiveVelocity,
    LiveLength,
    LiveQuantize,
    StoreTemplate,
    RevertTemplate,
    Randomizing(Randomize),
    /// A menu opening another, where it was.
    Open(StepsMenu, Point<Pixels>),
    // The browser.
    Load(library::Entry),
    SavePattern,
    SaveTemplate,
    NewFolder,
    DeleteFolder,
    UseProjectKey,
}

/// The menus a menu can open.
#[derive(Clone, Debug, PartialEq)]
pub enum StepsMenu {
    Notes(Option<usize>),
    Automation(Option<usize>),
    Cc(Option<usize>),
    RowColor(usize),
    Randomize,
}

impl Workspace {
    fn steps_menu(&mut self, at: Point<Pixels>, items: Vec<(String, MenuAction)>) {
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
    }

    /// The selected row's steps picked with ⇧-click, else the step under the cursor.
    fn picked_steps(&self) -> Vec<(usize, usize)> {
        let mut picked = self.steps.picked.clone();
        if picked.is_empty()
            && let Some(c) = self.steps.cursor
        {
            picked.push(c);
        }
        picked.sort();
        picked
    }

    pub fn steps_op(&mut self, op: StepsOp, cx: &mut Context<Self>) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |d| d.as_nanos() as u64);
        let lane = self.steps.mode.lane();
        match op {
            StepsOp::Mode(m) => {
                self.steps.mode = m;
                if m != EditMode::OnOff {
                    self.steps.value_mode = m;
                }
            }
            StepsOp::ShowValues(v) => self.steps.values = v,
            StepsOp::ShowColors => self.steps.colors ^= true,
            StepsOp::Sort(by) => {
                let desc = self.steps.sort_desc;
                let names: Vec<(PatternRow, String)> = match self.shown_pattern() {
                    Some((at, p)) => {
                        let looks = self.row_looks(at, p);
                        p.rows
                            .iter()
                            .cloned()
                            .zip(looks.into_iter().map(|l| l.name))
                            .collect()
                    }
                    None => return,
                };
                self.edit_pattern(cx, move |p| {
                    p.sort(by, desc, |r| {
                        names
                            .iter()
                            .find(|(row, _)| row == r)
                            .map_or_else(String::new, |(_, n)| n.clone())
                    })
                });
            }
            StepsOp::SortDescending(d) => self.steps.sort_desc = d,
            StepsOp::Pane(p) => self.steps.pane = p,
            StepsOp::Length(n) => self.edit_pattern(cx, |p| p.set_length(n)),
            StepsOp::PatternRate(r) => self.edit_pattern(cx, |p| p.step = r.max(1)),
            StepsOp::PatternPlay(m) => self.edit_pattern(cx, |p| p.mode = m),
            StepsOp::SwingEighths(e) => self.edit_pattern(cx, |p| p.swing_eighths = e),
            StepsOp::Legato(l) => self.edit_pattern(cx, |p| p.legato = l),
            StepsOp::Key(k, moves) => self.edit_pattern(cx, |p| p.set_key(k, moves)),
            StepsOp::Scale(s) => self.edit_pattern(cx, |p| p.scale = s),
            StepsOp::Conform => self.edit_pattern(cx, |p| p.conform()),
            StepsOp::Transpose(by) => self.edit_pattern(cx, |p| p.transpose(by)),
            StepsOp::RotatePattern(by) => self.edit_pattern(cx, |p| {
                let n = p.steps();
                for row in &mut p.rows {
                    row.rotate(n, by, lane);
                }
            }),
            StepsOp::StepPattern(up) => self.edit_pattern(cx, |p| {
                let n = p.steps();
                for row in p
                    .rows
                    .iter_mut()
                    .filter(|r| lane.is_none_or(|l| l.applies(r.is_automation())))
                {
                    row.step_all(n, lane, up);
                }
            }),
            StepsOp::RandomizePattern(which) => {
                let how = self.steps.randomize;
                self.edit_pattern(cx, |p| p.randomize(lane, &how, which, seed));
            }
            StepsOp::ClearPatternValues(which) => self.edit_pattern(cx, |p| {
                for row in p.rows.iter_mut().filter(|r| {
                    which.is_none_or(|a| a == r.is_automation())
                        && lane.is_none_or(|l| l.applies(r.is_automation()))
                }) {
                    row.clear_lane(lane);
                }
            }),
            StepsOp::ClearPattern => self.edit_pattern(cx, |p| {
                for row in &mut p.rows {
                    row.clear();
                }
            }),
            StepsOp::AddNote(pitch) => self.add_row(PatternRow::note(pitch, 0), cx),
            StepsOp::AddMelodic => {
                let pitch = self.shown_pattern().map_or(60, |(_, p)| 60 + p.key);
                let mut row = PatternRow::note(pitch, 0);
                row.melodic = true;
                self.add_row(row, cx);
            }
            StepsOp::AddAutomation(target) => {
                let value = self.target_now(target);
                self.add_row(PatternRow::automation(target, 0, value), cx);
            }
            StepsOp::Learn(l) => {
                self.steps.learn = l;
                if l.is_some() {
                    self.status = Some(match l {
                        Some(Learn::Add) => "Learn: play notes or move controls to add rows".into(),
                        _ => "Learn: play a note or move a control for the selected row".into(),
                    });
                }
            }
            StepsOp::AssignNote(r, pitch) => self.edit_row(r, cx, |row, _| {
                row.target = None;
                row.pitch = pitch;
                row.melodic = false;
                row.transpose.clear();
                row.octave.clear();
            }),
            StepsOp::AssignMelodic(r) => self.edit_row(r, cx, |row, _| {
                row.target = None;
                row.melodic = true;
            }),
            StepsOp::AssignAutomation(r, target) => {
                let value = self.target_now(target);
                self.edit_row(r, cx, |row, n| {
                    row.target = Some(target);
                    row.value = vec![value; n];
                    row.subrows.retain(|l| l.applies(true));
                });
            }
            StepsOp::RowRate(r, rate) => self.edit_row(r, cx, |row, _| row.rate = rate),
            StepsOp::RowPlay(r, m) => self.edit_row(r, cx, |row, _| row.mode = m),
            StepsOp::RowLoop(r, a, b) => self.edit_row(r, cx, |row, n| {
                row.loop_start = a.min(n - 1);
                row.loop_end = Some(b.clamp(row.loop_start + 1, n));
            }),
            StepsOp::RowColor(r, c) => self.edit_row(r, cx, |row, _| row.color = c),
            StepsOp::Slide(r, s) => self.edit_row(r, cx, |row, _| row.slide = s),
            StepsOp::Mute(r) => self.edit_row(r, cx, |row, _| row.mute ^= true),
            StepsOp::Solo(r) => self.edit_row(r, cx, |row, _| row.solo ^= true),
            StepsOp::Rename(r) => {
                if let Some(at) = self.pattern() {
                    self.steps.ask = Some(crate::workspace::Name::PatternRow(at, r));
                }
            }
            StepsOp::CopyRow(r) => {
                if let Some(row) = self.shown_pattern().and_then(|(_, p)| p.rows.get(r)) {
                    self.steps.clip = Some(StepsClip::Row(Box::new(row.clone())));
                }
            }
            StepsOp::PasteRow(r) => {
                if let Some(StepsClip::Row(from)) = self.steps.clip.clone() {
                    self.edit_row(r, cx, |row, _| paste_row(row, &from));
                }
            }
            StepsOp::PasteToNewRow => {
                if let Some(StepsClip::Row(from)) = self.steps.clip.clone() {
                    self.add_row(*from, cx);
                }
            }
            StepsOp::PasteRowSettings(r) => {
                if let Some(StepsClip::Row(from)) = self.steps.clip.clone() {
                    self.edit_row(r, cx, |row, _| paste_row_settings(row, &from));
                }
            }
            StepsOp::PasteSteps(r) => {
                if let Some(StepsClip::Row(from)) = self.steps.clip.clone() {
                    self.edit_row(r, cx, |row, n| {
                        for i in 0..n.min(from.cells.len()) {
                            row.paste_step(i, &from.copy_step(i));
                        }
                    });
                }
            }
            StepsOp::DuplicateRow(r) => self.duplicate_row(r, cx),
            StepsOp::DeleteRow(r) => {
                self.edit_pattern(cx, |p| {
                    if r < p.rows.len() {
                        p.rows.remove(r);
                    }
                });
                self.steps.picked.clear();
            }
            StepsOp::DeleteUnused => self.edit_pattern(cx, |p| p.remove_unused()),
            StepsOp::ClearRow(r) => self.edit_row(r, cx, |row, _| row.clear()),
            StepsOp::RotateRow(r, l, by) => self.edit_row(r, cx, |row, n| row.rotate(n, by, l)),
            StepsOp::StepRow(r, l, up) => self.edit_row(r, cx, |row, n| row.step_all(n, l, up)),
            StepsOp::RandomizeRow(r, l) => {
                let how = self.steps.randomize;
                self.edit_row(r, cx, |row, n| row.randomize(n, l, &how, seed));
            }
            StepsOp::ClearRowValues(r, l) => self.edit_row(r, cx, |row, _| row.clear_lane(l)),
            StepsOp::SubrowLane(r, k, l) => self.edit_row(r, cx, |row, _| {
                if row.subrows.is_empty() {
                    row.subrows = default_subrows(row);
                }
                if let Some(s) = row.subrows.get_mut(k) {
                    *s = l;
                }
            }),
            StepsOp::AddSubrow(r, k) => self.edit_row(r, cx, |row, _| {
                if row.subrows.is_empty() {
                    row.subrows = default_subrows(row);
                }
                let auto = row.is_automation();
                if let Some(next) = Lane::ALL
                    .into_iter()
                    .find(|l| l.applies(auto) && !row.subrows.contains(l))
                {
                    row.subrows.insert((k + 1).min(row.subrows.len()), next);
                }
            }),
            StepsOp::DeleteSubrow(r, k) => self.edit_row(r, cx, |row, _| {
                if row.subrows.is_empty() {
                    row.subrows = default_subrows(row);
                }
                if k < row.subrows.len() {
                    row.subrows.remove(k);
                }
                if row.subrows.is_empty() {
                    row.open = false;
                }
            }),
            StepsOp::CutSteps | StepsOp::CopySteps => {
                let picked = self.picked_steps();
                let Some((_, p)) = self.shown_pattern() else {
                    return;
                };
                let copied: Vec<StepCopy> = picked
                    .iter()
                    .filter_map(|&(r, i)| Some(p.rows.get(r)?.copy_step(i)))
                    .collect();
                if copied.is_empty() {
                    return;
                }
                self.steps.clip = Some(StepsClip::Steps(copied));
                if op == StepsOp::CutSteps {
                    self.edit_pattern(cx, |p| {
                        for (r, i) in picked {
                            if let Some(row) = p.rows.get_mut(r) {
                                row.clear_step(i);
                            }
                        }
                    });
                }
            }
            StepsOp::PasteStepsAt(r, i) => {
                if let Some(StepsClip::Steps(steps)) = self.steps.clip.clone() {
                    self.edit_row(r, cx, |row, n| {
                        for (k, s) in steps.iter().enumerate().filter(|(k, _)| i + k < n) {
                            row.paste_step(i + k, s);
                        }
                    });
                }
            }
            StepsOp::RepeatStep => {
                // The picked steps again, right after the last of them.
                let picked = self.picked_steps();
                let Some(&(r, last)) = picked.last() else {
                    return;
                };
                let first = picked
                    .iter()
                    .filter(|s| s.0 == r)
                    .map(|s| s.1)
                    .min()
                    .unwrap_or(last);
                let span = last - first + 1;
                self.edit_row(r, cx, |row, n| {
                    for k in 0..span {
                        if last + 1 + k < n {
                            let s = row.copy_step(first + k);
                            row.paste_step(last + 1 + k, &s);
                        }
                    }
                });
                self.steps.picked = (last + 1..(last + 1 + span)).map(|i| (r, i)).collect();
            }
            StepsOp::ClearStep => {
                let picked = self.picked_steps();
                self.edit_pattern(cx, |p| {
                    for (r, i) in picked {
                        if let Some(row) = p.rows.get_mut(r) {
                            row.clear_step(i);
                        }
                    }
                });
            }
            StepsOp::StepRate(r, i, rate) => self.edit_row(r, cx, |row, n| {
                row.rates.resize(n, 0);
                row.rates[i.min(n - 1)] = rate.unwrap_or(0);
            }),
            StepsOp::StepCondition(r, i, c) => self.edit_row(r, cx, |row, n| {
                row.condition.resize(n, Condition::Always);
                row.condition[i.min(n - 1)] = c;
            }),
            StepsOp::CutPage(k) | StepsOp::CopyPage(k) | StepsOp::ClearPage(k) => {
                let (from, per) = self.page_span(k);
                if !matches!(op, StepsOp::ClearPage(_))
                    && let Some((_, p)) = self.shown_pattern()
                {
                    let page = p
                        .rows
                        .iter()
                        .map(|row| (from..from + per).map(|i| row.copy_step(i)).collect())
                        .collect();
                    self.steps.page_clip = Some(page);
                }
                if !matches!(op, StepsOp::CopyPage(_)) {
                    self.edit_pattern(cx, |p| {
                        for row in &mut p.rows {
                            for i in from..(from + per).min(row.cells.len()) {
                                row.clear_step(i);
                            }
                        }
                    });
                }
            }
            StepsOp::PastePage(k) => {
                let (from, _) = self.page_span(k);
                if let Some(page) = self.steps.page_clip.clone() {
                    self.edit_pattern(cx, |p| {
                        for (row, steps) in p.rows.iter_mut().zip(page) {
                            for (j, s) in steps.iter().enumerate() {
                                row.paste_step(from + j, s);
                            }
                        }
                    });
                }
            }
            StepsOp::LiveVelocity => self.steps.live_velocity ^= true,
            StepsOp::LiveLength => self.steps.live_length ^= true,
            StepsOp::LiveQuantize => self.steps.live_quantize ^= true,
            StepsOp::StoreTemplate => {
                let Some((at, p)) = self.shown_pattern() else {
                    return;
                };
                let mut template = p.clone();
                for row in &mut template.rows {
                    let n = row.cells.len();
                    row.clear();
                    row.cells = vec![0; n];
                }
                self.edit(cx, |ws| {
                    ws.session.project.tracks[at.0].pattern_template = Some(template)
                });
                self.status = Some("Stored as this track's default pattern".into());
            }
            StepsOp::RevertTemplate => {
                let Some(at) = self.pattern() else {
                    return;
                };
                let template = self.session.project.tracks[at.0].pattern_template.clone();
                let template = template.unwrap_or_else(|| self.starting_pattern(at.0));
                self.edit_pattern(cx, |p| *p = template);
            }
            StepsOp::Randomizing(how) => self.steps.randomize = how,
            StepsOp::Open(menu, at) => {
                let at = if at == Point::default() {
                    self.steps.menu_at
                } else {
                    at
                };
                let items = self.steps_submenu(&menu);
                self.menu = Some(Menu {
                    at,
                    searchable: matches!(menu, StepsMenu::Automation(_) | StepsMenu::Cc(_)),
                    items,
                    ..Default::default()
                });
                cx.notify();
                return;
            }
            StepsOp::Load(entry) => self.load_pattern(&entry, cx),
            StepsOp::SavePattern => self.save_pattern(false, cx),
            StepsOp::SaveTemplate => self.save_pattern(true, cx),
            StepsOp::NewFolder => self.new_pattern_folder(cx),
            StepsOp::DeleteFolder => self.delete_pattern_folder(cx),
            StepsOp::UseProjectKey => {
                if let Some(key) = self.session.project.song_key() {
                    self.edit_pattern(cx, |p| p.set_key(key.root, true));
                }
            }
        }
        cx.notify();
    }

    /// A row added to the shown pattern, as long as its steps, and picked.
    pub(crate) fn add_row(&mut self, mut row: PatternRow, cx: &mut Context<Self>) {
        let Some((_, p)) = self.shown_pattern() else {
            return;
        };
        let (n, at) = (p.steps(), p.rows.len());
        row.cells.resize(n, 0);
        self.edit_pattern(cx, |p| p.rows.push(row));
        self.steps.row = at;
    }

    /// A row's copy under it, on the next sound it doesn't have a row for.
    fn duplicate_row(&mut self, r: usize, cx: &mut Context<Self>) {
        let Some((at, p)) = self.shown_pattern() else {
            return;
        };
        let Some(row) = p.rows.get(r) else {
            return;
        };
        let used: Vec<u8> = p
            .rows
            .iter()
            .filter(|r| !r.is_automation())
            .map(|r| r.pitch)
            .collect();
        let mut copy = row.clone();
        copy.name.clear();
        if !row.is_automation() {
            let instrument = self.session.project.tracks[at.0].instrument.as_ref();
            let next = match kit_sounds(instrument) {
                Some(sounds) => sounds
                    .iter()
                    .map(|s| s.0)
                    .skip_while(|&s| s != row.pitch)
                    .find(|s| !used.contains(s)),
                None => (row.pitch..128).find(|p| !used.contains(p)),
            };
            copy.pitch = next.unwrap_or(row.pitch);
        }
        self.edit_pattern(cx, |p| p.rows.insert(r + 1, copy));
        self.steps.row = r + 1;
    }

    /// An automation target's value now, 0 to 1: where a new row of it starts.
    fn target_now(&self, target: RowTarget) -> f32 {
        let Some(at) = self.pattern() else {
            return pina_engine::song::project::VALUE;
        };
        match target.param() {
            Some(p) => p.to_unit(self.now_value(at.0, p)),
            None => match target {
                RowTarget::PitchBend => 0.5,
                RowTarget::Cc(7) => 100.0 / 127.0,
                RowTarget::Cc(10) => 0.5,
                _ => 0.0,
            },
        }
    }

    /// The first step of page `k`, and how many a page has.
    pub(crate) fn page_span(&self, k: usize) -> (usize, usize) {
        let per = self.steps.geom.borrow().shown.1.max(1);
        (k * per, per)
    }

    /// The pattern a new region on track `t` starts as: its stored template, or a kit's
    /// kick, snare and hats, or the key's chord.
    pub(crate) fn starting_pattern(&self, t: usize) -> Pattern {
        let project = &self.session.project;
        if let Some(p) = project.tracks[t].pattern_template.clone() {
            return p;
        }
        let bar = project.bar();
        let steps = (bar / (PPQ / 4)).clamp(1, 64) as usize;
        let instrument = project.tracks[t].instrument.as_ref();
        let starting: Vec<u8> = match (instrument, kit_sounds(instrument)) {
            (Some(Instrument::Drums), _) => STARTING_ROWS.to_vec(),
            (_, Some(sounds)) => sounds.iter().take(6).map(|s| s.0).collect(),
            _ => {
                let key = project.song_key().unwrap_or(Key {
                    root: 0,
                    minor: false,
                });
                let root = 60 + key.root;
                let third = if key.minor { 3 } else { 4 };
                vec![root + 12, root + 7, root + third, root]
            }
        };
        let rows = starting
            .iter()
            .map(|&pitch| PatternRow::note(pitch, steps))
            .collect();
        let mut p = Pattern::new(PPQ / 4, rows, 50.0);
        p.key = project.song_key().map_or(0, |k| k.root);
        p
    }

    // The menus.

    /// Edit: steps and rows cut, copied and pasted; rows duplicated and deleted.
    pub(crate) fn steps_edit_menu(&self) -> Vec<(String, MenuAction)> {
        let r = self.steps.row;
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        let off = |label: &str| (label.to_string(), MenuAction::Nothing);
        let steps = !self.picked_steps().is_empty();
        let has_row = self.shown_pattern().is_some_and(|(_, p)| r < p.rows.len());
        let row_clip = matches!(self.steps.clip, Some(StepsClip::Row(_)));
        let step_clip = matches!(self.steps.clip, Some(StepsClip::Steps(_)));
        let mut items = vec![
            if steps {
                op("Cut Step\t⌘X", StepsOp::CutSteps)
            } else {
                off("Cut Step\t⌘X")
            },
            if steps {
                op("Copy Step\t⌘C", StepsOp::CopySteps)
            } else {
                off("Copy Step\t⌘C")
            },
        ];
        items.push(match (step_clip, self.picked_steps().first()) {
            (true, Some(&(r, i))) => op("Paste Step\t⌘V", StepsOp::PasteStepsAt(r, i)),
            _ => off("Paste Step\t⌘V"),
        });
        items.push(if steps {
            op("Repeat Step\t⌘R", StepsOp::RepeatStep)
        } else {
            off("Repeat Step\t⌘R")
        });
        items.push((String::new(), MenuAction::Nothing));
        if has_row {
            items.push(op("Copy Row\t⌃C", StepsOp::CopyRow(r)));
        }
        if row_clip && has_row {
            items.push(op("Paste Row\t⌃V", StepsOp::PasteRow(r)));
            items.push(op("Paste to New Row\t⌃⇧V", StepsOp::PasteToNewRow));
            items.push(op("Paste Row Settings\t⌃⌥⇧V", StepsOp::PasteRowSettings(r)));
            items.push(op("Paste Steps\t⌥⇧V", StepsOp::PasteSteps(r)));
        } else {
            items.extend(
                [
                    "Paste Row\t⌃V",
                    "Paste to New Row\t⌃⇧V",
                    "Paste Row Settings\t⌃⌥⇧V",
                    "Paste Steps\t⌥⇧V",
                ]
                .map(off),
            );
        }
        items.push((String::new(), MenuAction::Nothing));
        if has_row {
            items.push(op(
                "Duplicate Row with Next Assignment\t⌘D",
                StepsOp::DuplicateRow(r),
            ));
            items.push(op("Delete Row\t⌘⌫", StepsOp::DeleteRow(r)));
        }
        let unused = self
            .shown_pattern()
            .is_some_and(|(_, p)| p.rows.iter().any(|r| r.cells.iter().all(|&c| c == 0)));
        items.push(match unused {
            true => op("Delete Unused Rows\t⇧⌘⌫", StepsOp::DeleteUnused),
            false => off("Delete Unused Rows\t⇧⌘⌫"),
        });
        items
    }

    /// Functions: transposing, randomizing, clearing, live recording's options, templates.
    pub(crate) fn steps_functions_menu(&self) -> Vec<(String, MenuAction)> {
        let r = self.steps.row;
        let lane = self.steps.mode.lane();
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        let tick = |on: bool, label: &str| format!("{}{label}", if on { "✓ " } else { "" });
        vec![
            op("Transpose +1 Semitone\t⌥↑", StepsOp::Transpose(1)),
            op("Transpose −1 Semitone\t⌥↓", StepsOp::Transpose(-1)),
            op("Transpose +1 Octave\t⌥⇧↑", StepsOp::Transpose(12)),
            op("Transpose −1 Octave\t⌥⇧↓", StepsOp::Transpose(-12)),
            (String::new(), MenuAction::Nothing),
            op(
                "Randomize Current Edit Mode for Pattern\t⇧⌘R",
                StepsOp::RandomizePattern(None),
            ),
            op(
                "Randomize Current Edit Mode for Selected Row\t⌥⌘R",
                StepsOp::RandomizeRow(r, lane),
            ),
            op(
                "Randomize Current Edit Mode for Note Rows",
                StepsOp::RandomizePattern(Some(false)),
            ),
            op(
                "Randomize Current Edit Mode for Automation Rows",
                StepsOp::RandomizePattern(Some(true)),
            ),
            op(
                "Randomize Settings…",
                StepsOp::Open(StepsMenu::Randomize, Point::default()),
            ),
            (String::new(), MenuAction::Nothing),
            op("Clear Step\t⌃⌫", StepsOp::ClearStep),
            op("Clear Row\t⌃⌘⌫", StepsOp::ClearRow(r)),
            op(
                "Clear Current Edit Mode Values for Row\t⌥⌘⌫",
                StepsOp::ClearRowValues(r, lane),
            ),
            op(
                "Clear Current Edit Mode Values for All Rows\t⌥⇧⌘⌫",
                StepsOp::ClearPatternValues(None),
            ),
            op(
                "Clear Current Edit Mode Values for Note Rows",
                StepsOp::ClearPatternValues(Some(false)),
            ),
            op(
                "Clear Current Edit Mode Values for Automation Rows",
                StepsOp::ClearPatternValues(Some(true)),
            ),
            op("Clear Pattern\t⌃⇧⌘⌫", StepsOp::ClearPattern),
            (String::new(), MenuAction::Nothing),
            ("Live Record Options".into(), MenuAction::Nothing),
            op(
                &tick(self.steps.live_velocity, "Velocity"),
                StepsOp::LiveVelocity,
            ),
            op(
                &tick(self.steps.live_length, "Note Length"),
                StepsOp::LiveLength,
            ),
            op(
                &tick(self.steps.live_quantize, "Quantize"),
                StepsOp::LiveQuantize,
            ),
            (String::new(), MenuAction::Nothing),
            op(
                "Store as Default Pattern Template\t⌃⌥⌘D",
                StepsOp::StoreTemplate,
            ),
            op(
                "Revert to Default Pattern Template\t⌃⌥⇧D",
                StepsOp::RevertTemplate,
            ),
        ]
    }

    /// View: when values show, the rows' order and colours, the panes.
    pub(crate) fn steps_view_menu(&self) -> Vec<(String, MenuAction)> {
        let op = |label: String, op: StepsOp| (label, MenuAction::Steps(op));
        let tick = |on: bool, label: &str| format!("{}{label}", if on { "✓ " } else { "" });
        let v = self.steps.values;
        let pane = self.steps.pane;
        vec![
            ("Display Edit Mode Values".into(), MenuAction::Nothing),
            op(
                tick(v == ShowValues::OnlyWhenEditing, "Only When Editing Values"),
                StepsOp::ShowValues(ShowValues::OnlyWhenEditing),
            ),
            op(
                tick(v == ShowValues::SelectedRow, "Always for Selected Row"),
                StepsOp::ShowValues(ShowValues::SelectedRow),
            ),
            op(
                tick(v == ShowValues::AllRows, "Always for All Rows"),
                StepsOp::ShowValues(ShowValues::AllRows),
            ),
            (String::new(), MenuAction::Nothing),
            ("Sort Rows".into(), MenuAction::Nothing),
            op(tick(false, "By Type"), StepsOp::Sort(SortRows::Type)),
            op(
                tick(false, "By Type and Note"),
                StepsOp::Sort(SortRows::TypeAndNote),
            ),
            op(
                tick(false, "By Type and Name"),
                StepsOp::Sort(SortRows::TypeAndName),
            ),
            op(
                tick(!self.steps.sort_desc, "Ascending"),
                StepsOp::SortDescending(false),
            ),
            op(
                tick(self.steps.sort_desc, "Descending"),
                StepsOp::SortDescending(true),
            ),
            (String::new(), MenuAction::Nothing),
            op(
                tick(self.steps.colors, "Show Row Colors"),
                StepsOp::ShowColors,
            ),
            op(
                tick(pane == Some(Pane::Inspector), "Show Local Inspector"),
                StepsOp::Pane((pane != Some(Pane::Inspector)).then_some(Pane::Inspector)),
            ),
            op(
                tick(pane == Some(Pane::Browser), "Show Pattern Browser\t⌥⇧B"),
                StepsOp::Pane((pane != Some(Pane::Browser)).then_some(Pane::Browser)),
            ),
        ]
    }

    /// Add Row: notes (the kit's sounds, or the key's), melodic, automation, learn.
    pub(crate) fn add_row_menu(&self, at: Point<Pixels>) -> Vec<(String, MenuAction)> {
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        vec![
            op("Notes…", StepsOp::Open(StepsMenu::Notes(None), at)),
            op("Melodic", StepsOp::AddMelodic),
            op(
                "Automation…",
                StepsOp::Open(StepsMenu::Automation(None), at),
            ),
            (String::new(), MenuAction::Nothing),
            op("Learn (Add)\t⌥⌘L", StepsOp::Learn(Some(Learn::Add))),
            op("Learn (Assign)", StepsOp::Learn(Some(Learn::Assign))),
        ]
    }

    /// A row's assignment: its sound or parameter, picked again.
    pub(crate) fn assign_menu(&self, r: usize, at: Point<Pixels>) -> Vec<(String, MenuAction)> {
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        vec![
            op("Notes…", StepsOp::Open(StepsMenu::Notes(Some(r)), at)),
            op("Melodic", StepsOp::AssignMelodic(r)),
            op(
                "Automation…",
                StepsOp::Open(StepsMenu::Automation(Some(r)), at),
            ),
            (String::new(), MenuAction::Nothing),
            op("Rename Row\t⇧↩", StepsOp::Rename(r)),
            op("Learn (Assign)", StepsOp::Learn(Some(Learn::Assign))),
        ]
    }

    /// The menus a menu opens.
    fn steps_submenu(&self, menu: &StepsMenu) -> Vec<(String, MenuAction)> {
        let op = |label: String, op: StepsOp| (label, MenuAction::Steps(op));
        let Some(at) = self.pattern() else {
            return Vec::new();
        };
        let project = &self.session.project;
        let track = &project.tracks[at.0];
        match *menu {
            StepsMenu::Notes(row) => {
                let make = |pitch: u8| match row {
                    Some(r) => StepsOp::AssignNote(r, pitch),
                    None => StepsOp::AddNote(pitch),
                };
                match kit_sounds(track.instrument.as_ref()) {
                    Some(sounds) => sounds
                        .into_iter()
                        .map(|(pitch, name)| op(name, make(pitch)))
                        .collect(),
                    None => {
                        let key = project.song_key();
                        let mut items: Vec<_> = scale_notes(key)
                            .into_iter()
                            .map(|p| op(crate::editors::pianoroll::name_in(key, p), make(p)))
                            .collect();
                        items.push((String::new(), MenuAction::Nothing));
                        items.extend((24..=96).rev().map(|p| {
                            op(format!("{}  ", crate::editors::pianoroll::name(p)), make(p))
                        }));
                        items
                    }
                }
            }
            StepsMenu::Automation(row) => {
                let make = |t: RowTarget| match row {
                    Some(r) => StepsOp::AssignAutomation(r, t),
                    None => StepsOp::AddAutomation(t),
                };
                let mut items: Vec<(String, MenuAction)> = self
                    .auto_params(at.0, 0)
                    .into_iter()
                    .map(|(label, action)| match action {
                        MenuAction::Auto(_, crate::arrange::automation::AutoOp::Lane(_, p)) => op(
                            label.trim_start_matches("● ").to_string(),
                            make(RowTarget::from_param(p)),
                        ),
                        other => (label, other),
                    })
                    .collect();
                items.push(("MIDI".into(), MenuAction::Nothing));
                items.push(op(
                    "MIDI CC…".into(),
                    StepsOp::Open(StepsMenu::Cc(row), Point::default()),
                ));
                items.push(op("Pitch Bend".into(), make(RowTarget::PitchBend)));
                items.push(op("Channel Pressure".into(), make(RowTarget::Pressure)));
                items
            }
            StepsMenu::Cc(row) => (0..128u8)
                .map(|n| {
                    let t = RowTarget::Cc(n);
                    op(
                        t.name(),
                        match row {
                            Some(r) => StepsOp::AssignAutomation(r, t),
                            None => StepsOp::AddAutomation(t),
                        },
                    )
                })
                .collect(),
            StepsMenu::RowColor(r) => {
                let now = self.shown_pattern().and_then(|(_, p)| p.rows.get(r)?.color);
                std::iter::once(op(
                    format!("{}Its sound's", if now.is_none() { "✓ " } else { "" }),
                    StepsOp::RowColor(r, None),
                ))
                .chain(ROW_COLORS.iter().map(|&(name, c)| {
                    op(
                        format!("{}{name}", if now == Some(c) { "✓ " } else { "" }),
                        StepsOp::RowColor(r, Some(c)),
                    )
                }))
                .collect()
            }
            StepsMenu::Randomize => {
                let how = self.steps.randomize;
                let tick =
                    |on: bool, label: String| format!("{}{label}", if on { "✓ " } else { "" });
                let mut items = vec![(
                    "Probability (On/Off, Tie, Skip)".to_string(),
                    MenuAction::Nothing,
                )];
                for p in [10u8, 25, 50, 75, 90] {
                    items.push(op(
                        tick(how.probability == p, format!("{p}%")),
                        StepsOp::Randomizing(Randomize {
                            probability: p,
                            ..how
                        }),
                    ));
                }
                items.push((String::new(), MenuAction::Nothing));
                items.push(("Values".into(), MenuAction::Nothing));
                for s in [10u8, 25, 50, 100] {
                    items.push(op(
                        tick(
                            !how.within && how.spread == s,
                            format!("Offset ±{s}% of each step's"),
                        ),
                        StepsOp::Randomizing(Randomize {
                            within: false,
                            spread: s,
                            ..how
                        }),
                    ));
                }
                let full = how.within && how.low <= f32::MIN / 2.0;
                items.push(op(
                    tick(full, "Range: its whole range".into()),
                    StepsOp::Randomizing(Randomize {
                        within: true,
                        low: f32::MIN,
                        high: f32::MAX,
                        ..how
                    }),
                ));
                items
            }
        }
    }

    /// A rate pop-up: the pattern's, or a row's (which can follow the pattern's).
    pub(crate) fn rate_menu(&self, row: Option<usize>) -> Vec<(String, MenuAction)> {
        let Some((_, p)) = self.shown_pattern() else {
            return Vec::new();
        };
        let now = match row {
            Some(r) => p.rows.get(r).and_then(|r| r.rate),
            None => Some(p.step),
        };
        let mut items = Vec::new();
        if let Some(r) = row {
            items.push((
                format!(
                    "{}Pattern's ({})",
                    if now.is_none() { "✓ " } else { "" },
                    rate_name(p.step)
                ),
                MenuAction::Steps(StepsOp::RowRate(r, None)),
            ));
        }
        for (k, (name, ticks)) in RATES.iter().enumerate() {
            if k == 9 || k == 15 {
                items.push((String::new(), MenuAction::Nothing));
            }
            let on = now == Some(*ticks);
            items.push((
                format!("{}{name}", if on { "✓ " } else { "" }),
                MenuAction::Steps(match row {
                    Some(r) => StepsOp::RowRate(r, Some(*ticks)),
                    None => StepsOp::PatternRate(*ticks),
                }),
            ));
        }
        items
    }

    /// A playback mode pop-up: the pattern's, or a row's.
    pub(crate) fn play_menu(&self, row: Option<usize>) -> Vec<(String, MenuAction)> {
        let Some((_, p)) = self.shown_pattern() else {
            return Vec::new();
        };
        let now = match row {
            Some(r) => p.rows.get(r).and_then(|r| r.mode),
            None => Some(p.mode),
        };
        let mut items = Vec::new();
        if let Some(r) = row {
            items.push((
                format!(
                    "{}Pattern's ({})",
                    if now.is_none() { "✓ " } else { "" },
                    p.mode.name()
                ),
                MenuAction::Steps(StepsOp::RowPlay(r, None)),
            ));
        }
        for m in PlayMode::ALL {
            items.push((
                format!("{}{}", if now == Some(m) { "✓ " } else { "" }, m.name()),
                MenuAction::Steps(match row {
                    Some(r) => StepsOp::RowPlay(r, Some(m)),
                    None => StepsOp::PatternPlay(m),
                }),
            ));
        }
        items
    }

    /// The pattern length pop-up, in fours, threes, fives and sevens.
    pub(crate) fn length_menu(&self, at: Point<Pixels>) -> Menu {
        let now = self.shown_pattern().map_or(16, |(_, p)| p.steps());
        let column = |title: &str, by: usize, upto: usize| {
            let mut c = vec![(title.to_string(), MenuAction::Nothing)];
            c.extend((1..=upto / by).map(|k| {
                let n = k * by;
                (
                    format!("{}{n} Steps", if n == now { "✓ " } else { "" }),
                    MenuAction::Steps(StepsOp::Length(n)),
                )
            }));
            c
        };
        let mut columns = [
            column("4 Based (4/4)", 4, 64),
            column("3 Based (3/4)", 3, 63),
            column("5 Based (5/4)", 5, 60),
            column("7 Based (7/8)", 7, 63),
        ];
        let tall = columns.iter().map(|c| c.len()).max().unwrap_or(0);
        for c in &mut columns {
            c.resize(tall, (" ".into(), MenuAction::Nothing));
        }
        Menu {
            at,
            items: columns.into_iter().flatten().collect(),
            columns: 4,
            ..Default::default()
        }
    }

    /// A row's header, right-clicked.
    pub(crate) fn row_menu(&self, r: usize, at: Point<Pixels>) -> Vec<(String, MenuAction)> {
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        let row_clip = matches!(self.steps.clip, Some(StepsClip::Row(_)));
        let mut items = vec![op("Copy Row\t⌃C", StepsOp::CopyRow(r))];
        if row_clip {
            items.push(op("Paste Row\t⌃V", StepsOp::PasteRow(r)));
            items.push(op("Paste to New Row\t⌃⇧V", StepsOp::PasteToNewRow));
            items.push(op("Paste Row Settings\t⌃⌥⇧V", StepsOp::PasteRowSettings(r)));
        }
        items.extend([
            op(
                "Duplicate Row with Next Assignment\t⌘D",
                StepsOp::DuplicateRow(r),
            ),
            op("Clear Row\t⌃⌘⌫", StepsOp::ClearRow(r)),
            op("Delete Row\t⌘⌫", StepsOp::DeleteRow(r)),
            (String::new(), MenuAction::Nothing),
            op("Rename Row\t⇧↩", StepsOp::Rename(r)),
            op("Row Color…", StepsOp::Open(StepsMenu::RowColor(r), at)),
        ]);
        items
    }

    /// A step, right-clicked: cut, copy, paste, repeat.
    pub(crate) fn step_menu(&self, r: usize, i: usize) -> Vec<(String, MenuAction)> {
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        let mut items = vec![
            op("Cut\t⌘X", StepsOp::CutSteps),
            op("Copy\t⌘C", StepsOp::CopySteps),
        ];
        if matches!(self.steps.clip, Some(StepsClip::Steps(_))) {
            items.push(op("Paste\t⌘V", StepsOp::PasteStepsAt(r, i)));
        }
        items.push(op("Repeat\t⌘R", StepsOp::RepeatStep));
        items.push((String::new(), MenuAction::Nothing));
        items.push(op("Clear Step\t⌃⌫", StepsOp::ClearStep));
        items.push(op("Clear Row\t⌃⌘⌫", StepsOp::ClearRow(r)));
        items
    }

    /// A page's overview, right-clicked.
    pub(crate) fn page_menu(&self, k: usize) -> Vec<(String, MenuAction)> {
        let op = |label: &str, op: StepsOp| (label.to_string(), MenuAction::Steps(op));
        let mut items = vec![
            op("Cut", StepsOp::CutPage(k)),
            op("Copy", StepsOp::CopyPage(k)),
        ];
        if self.steps.page_clip.is_some() {
            items.push(op("Paste", StepsOp::PastePage(k)));
        }
        items.push(op("Clear", StepsOp::ClearPage(k)));
        items
    }

    /// The pattern's key, and whether choosing one moves the notes.
    pub(crate) fn pattern_key_menu(&self) -> Vec<(String, MenuAction)> {
        let now = self.shown_pattern().map_or(0, |(_, p)| p.key);
        const KEYS: [&str; 12] = [
            "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
        ];
        let mut items: Vec<_> = KEYS
            .iter()
            .enumerate()
            .map(|(k, name)| {
                (
                    format!("{}{name}", if k as u8 == now { "✓ " } else { "" }),
                    MenuAction::Steps(StepsOp::Key(k as u8, true)),
                )
            })
            .collect();
        items.push((String::new(), MenuAction::Nothing));
        items.push(("No Transpose".into(), MenuAction::Nothing));
        items.extend(KEYS.iter().enumerate().map(|(k, name)| {
            (
                format!("{name}, notes kept"),
                MenuAction::Steps(StepsOp::Key(k as u8, false)),
            )
        }));
        items
    }

    pub(crate) fn pattern_scale_menu(&self) -> Vec<(String, MenuAction)> {
        let now = self.shown_pattern().and_then(|(_, p)| p.scale);
        std::iter::once((
            format!("{}Chromatic", if now.is_none() { "✓ " } else { "" }),
            MenuAction::Steps(StepsOp::Scale(None)),
        ))
        .chain(Scale::ALL.into_iter().map(|s| {
            (
                format!("{}{}", if now == Some(s) { "✓ " } else { "" }, s.name()),
                MenuAction::Steps(StepsOp::Scale(Some(s))),
            )
        }))
        .collect()
    }

    /// A subrow's lane, picked again: those its row has no subrow for yet.
    pub(crate) fn subrow_menu(&self, r: usize, k: usize) -> Vec<(String, MenuAction)> {
        let Some(row) = self.shown_pattern().and_then(|(_, p)| p.rows.get(r)) else {
            return Vec::new();
        };
        let subrows = match row.subrows.is_empty() {
            true => default_subrows(row),
            false => row.subrows.clone(),
        };
        let auto = row.is_automation();
        Lane::ALL
            .into_iter()
            .filter(|l| l.applies(auto))
            .map(|l| {
                let now = subrows.get(k) == Some(&l);
                let used = !now && subrows.contains(&l);
                let label = format!(
                    "{}{}",
                    if now { "✓ " } else { "" },
                    lane_name(l, Some(auto))
                );
                match used {
                    true => (label, MenuAction::Nothing),
                    false => (label, MenuAction::Steps(StepsOp::SubrowLane(r, k, l))),
                }
            })
            .collect()
    }

    /// The edit mode pop-up.
    pub(crate) fn edit_mode_menu(&self) -> Vec<(String, MenuAction)> {
        EditMode::MENU
            .into_iter()
            .map(|m| {
                (
                    format!(
                        "{}{}",
                        if self.steps.value_mode == m {
                            "✓ "
                        } else {
                            ""
                        },
                        m.name()
                    ),
                    MenuAction::Steps(StepsOp::Mode(m)),
                )
            })
            .collect()
    }

    pub(crate) fn open_steps_menu(
        &mut self,
        at: Point<Pixels>,
        items: Vec<(String, MenuAction)>,
        cx: &mut Context<Self>,
    ) {
        self.steps.menu_at = at;
        self.steps_menu(at, items);
        cx.notify();
    }
}
