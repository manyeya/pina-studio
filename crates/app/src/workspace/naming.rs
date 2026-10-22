//! Names typed in place: tracks, regions, markers, buses, values.

use super::*;

/// What a name being typed is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Name {
    Track(usize),
    Aux(usize),
    Marker(usize),
    Region(At),
    /// A track's volume or pan, typed: "-6", "L30", "C".
    Volume(usize),
    Pan(usize),
    /// A value of the EQ window's: a band's (by id; 0 the EQ's own), typed.
    Eq(u16, crate::mix::eq::EqKnob),
    /// A step pattern's row, by its region and place; the pattern browser's search; a pattern
    /// (or template) saved, and a folder made, by the name typed.
    PatternRow(At, usize),
    PatternSearch,
    PatternSave(bool),
    PatternFolder,
    /// A collection on the Plugins page.
    PluginCollection(usize),
}

/// A name being typed, in a text field where the name shows.
pub struct Naming {
    pub what: Name,
    pub input: Entity<TextInput>,
    _subscriptions: [Subscription; 2],
}

impl Workspace {
    /// What something is called now.
    pub fn name_of(&self, what: Name) -> String {
        let p = &self.session.project;
        match what {
            Name::Track(i) => p.tracks.get(i).map(|t| t.name.clone()),
            Name::Aux(i) => p.auxes.get(i).map(|a| a.name.clone()),
            Name::Marker(i) => p.markers.get(i).map(|m| m.name.clone()),
            Name::Region(at) => p
                .tracks
                .get(at.0)
                .and_then(|t| t.regions.get(at.1))
                .map(|r| r.name.clone()),
            Name::Volume(i) => p.tracks.get(i).map(|t| match t.volume_db <= strip::SILENT {
                true => "-inf".into(),
                false => format!("{:.1}", t.volume_db),
            }),
            Name::Pan(i) => p.tracks.get(i).map(|t| strip::pan_label(t.pan)),
            Name::Eq(id, knob) => Some(self.eq_value_text(id, knob)),
            Name::PatternRow(at, r) => p
                .tracks
                .get(at.0)
                .and_then(|t| t.regions.get(at.1))
                .and_then(|reg| match &reg.content {
                    pina_engine::song::project::Content::Pattern(pt) => pt.rows.get(r),
                    _ => None,
                })
                .map(|row| row.name.clone()),
            Name::PatternSearch => Some(self.steps.query.clone()),
            Name::PatternSave(_) => self
                .shown_pattern()
                .map(|(at, _)| self.session.project.region(at).name.clone())
                .filter(|n| !n.is_empty())
                .or_else(|| Some("Pattern".into())),
            Name::PatternFolder => Some("New Folder".into()),
            Name::PluginCollection(i) => self.catalog.collections.get(i).map(|c| c.name.clone()),
        }
        .unwrap_or_default()
    }

    /// Opens a text field to name something, where its name shows.
    pub fn start_naming(&mut self, what: Name, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_naming(None, cx);
        let input = cx.new(|cx| TextInput::new(&self.name_of(what), cx));
        let done = cx.subscribe(&input, |ws, _, e: &TextEvent, cx| {
            let text = match e {
                TextEvent::Confirm(text) => Some(text.clone()),
                TextEvent::Cancel => None,
            };
            ws.finish_naming(text, cx);
        });
        let focus = input.read(cx).focus_handle(cx);
        // Clicking elsewhere keeps what's typed.
        let blur = cx.on_blur(&focus, window, |ws, _, cx| {
            if let Some(n) = &ws.naming {
                n.input.update(cx, |i, cx| i.confirm(cx));
            }
        });
        window.focus(&focus, cx);
        // Started from a click: the workspace mustn't take the keyboard back as it bubbles up.
        window.prevent_default();
        self.naming = Some(Naming {
            what,
            input,
            _subscriptions: [done, blur],
        });
        cx.notify();
    }

    /// Closes the text field, naming the thing `text` if given. A track, bus or marker keeps its
    /// name rather than going without; a region without one goes by its track's.
    pub fn finish_naming(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        self.refocus = true;
        let what = naming.what;
        // The step sequencer's: a row's name (empty goes back to its sound's), the browser's
        // search, a pattern or folder to save or make.
        match (what, text.as_ref()) {
            (Name::PatternRow(at, r), Some(text)) => {
                let text = text.trim().to_string();
                self.edit(cx, |ws| {
                    if let Some(pina_engine::song::project::Content::Pattern(p)) = ws
                        .session
                        .project
                        .tracks
                        .get_mut(at.0)
                        .and_then(|t| t.regions.get_mut(at.1))
                        .map(|reg| &mut reg.content)
                        && let Some(row) = p.rows.get_mut(r)
                    {
                        row.name = text;
                    }
                });
                return;
            }
            (Name::PatternSearch, text) => {
                if let Some(t) = text {
                    self.steps.query = t.trim().to_string();
                }
                cx.notify();
                return;
            }
            (Name::PatternSave(template), Some(text)) if !text.trim().is_empty() => {
                self.write_pattern(text.trim(), template);
                cx.notify();
                return;
            }
            (Name::PatternFolder, Some(text)) if !text.trim().is_empty() => {
                self.make_pattern_folder(text.trim());
                cx.notify();
                return;
            }
            (Name::PluginCollection(i), Some(text)) if !text.trim().is_empty() => {
                if let Some(c) = self.catalog.collections.get_mut(i) {
                    c.name = text.trim().to_string();
                }
                self.catalog.save();
                cx.notify();
                return;
            }
            (
                Name::PatternRow(..)
                | Name::PatternSave(_)
                | Name::PatternFolder
                | Name::PluginCollection(_),
                _,
            ) => {
                cx.notify();
                return;
            }
            _ => {}
        }
        let text = text.filter(|t| *t != self.name_of(what));
        if let (Name::Eq(id, knob), Some(text)) = (what, &text) {
            self.eq_value_typed(id, knob, text, cx);
            cx.notify();
            return;
        }
        // A volume or pan typed: set if it reads as one.
        if let (Name::Volume(i) | Name::Pan(i), Some(text)) = (what, &text) {
            match (what, strip::parse_db(text), strip::parse_pan(text)) {
                (Name::Volume(_), Some(db), _) => self.edit(cx, |ws| {
                    ws.set_volume(Channel::Track(i), db);
                    ws.moved_once(i, Param::Volume, db);
                }),
                (Name::Pan(_), _, Some(pan)) => self.edit(cx, |ws| {
                    ws.set_pan(Channel::Track(i), pan);
                    ws.moved_once(i, Param::Pan, pan);
                }),
                (Name::Volume(_), ..) => {
                    self.status = Some(format!("{text:?} isn't a level: type dB, as -6 or 0"))
                }
                _ => self.status = Some(format!("{text:?} isn't a pan: type L30, C or R45")),
            }
            cx.notify();
            return;
        }
        if let Some(text) = text.filter(|t| !t.is_empty() || matches!(what, Name::Region(_))) {
            self.edit(cx, |ws| {
                let p = &mut ws.session.project;
                match what {
                    Name::Track(i) => p.tracks[i].name = text,
                    Name::Aux(i) => p.auxes[i].name = text,
                    Name::Marker(i) => p.markers[i].name = text,
                    Name::Region(at) => p.tracks[at.0].regions[at.1].name = text,
                    Name::Volume(_)
                    | Name::Pan(_)
                    | Name::Eq(..)
                    | Name::PatternRow(..)
                    | Name::PatternSearch
                    | Name::PatternSave(_)
                    | Name::PatternFolder
                    | Name::PluginCollection(_) => {}
                }
            });
        }
        cx.notify();
    }

    /// A sideways swipe over a track's fader or pan knob: nudges it, one undo a swipe.
    pub fn swipe(&mut self, what: Name, dx: f32, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        if self
            .swiped
            .is_none_or(|(w, at)| w != what || now - at > std::time::Duration::from_millis(600))
        {
            self.session.checkpoint();
        }
        self.swiped = Some((what, now));
        match what {
            Name::Volume(i) => {
                let pos = strip::fader_pos(self.now_value(i, Param::Volume));
                let db = strip::fader_db((pos + dx * 0.002).clamp(0.0, 1.0));
                self.set_volume(Channel::Track(i), db);
                self.moved_once(i, Param::Volume, db);
            }
            Name::Pan(i) => {
                let pan = (self.now_value(i, Param::Pan) + dx * 0.004).clamp(-1.0, 1.0);
                self.set_pan(Channel::Track(i), pan);
                self.moved_once(i, Param::Pan, pan);
            }
            _ => return,
        }
        self.changed(cx);
    }

    /// The text field for `what`, if it's being named.
    pub fn name_field(&self, what: Name) -> Option<Entity<TextInput>> {
        self.naming
            .as_ref()
            .filter(|n| n.what == what)
            .map(|n| n.input.clone())
    }

    /// Whether the agent's message box or the look picker's search has the keyboard.
    pub(crate) fn typing_in_field(&self, window: &Window, cx: &App) -> bool {
        let focused = |input: &Entity<crate::ui::text::TextInput>| {
            input.read(cx).focus_handle(cx).is_focused(window)
        };
        focused(&self.chat.input) || self.picker.as_ref().is_some_and(|p| focused(&p.search))
    }
}
