//! Playbooks opened, picked, edited as text, saved, duplicated, reset and used.

use super::*;

/// A text a genre has, that a field edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Text {
    Name,
    Harmony,
    Sound(usize),
    Write(usize),
    Note(usize),
    Section(usize),
}

/// The fields for the genre being edited.
pub(crate) struct Fields {
    pub(crate) name: Entity<TextInput>,
    pub(crate) harmony: Entity<TextInput>,
    pub(crate) sounds: Vec<Entity<TextInput>>,
    pub(crate) writes: Vec<Entity<TextInput>>,
    pub(crate) notes: Vec<Entity<TextInput>>,
    pub(crate) section: Option<Entity<TextInput>>,
    pub(crate) _subs: Vec<Subscription>,
}

impl Workspace {
    /// The Playbooks page, on `id` (else the song's genre, else the first).
    pub fn open_genres(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        let (list, problems) = playbook::load();
        self.genres.list = list;
        self.genres.problems = problems;
        self.view = View::Playbooks;
        let want = id
            .map(str::to_string)
            .or(self.genres.draft.as_ref().map(|d| d.id.clone()))
            .or(self.session.project.genre.clone());
        let id = want
            .filter(|w| self.genres.list.iter().any(|p| p.id == *w))
            .or(self.genres.list.first().map(|p| p.id.clone()));
        if let Some(id) = id {
            self.select_genre(&id, cx);
        }
        cx.notify();
    }

    pub(crate) fn select_genre(&mut self, id: &str, cx: &mut Context<Self>) {
        let other = self.genres.draft.as_ref().is_none_or(|d| d.id != id);
        self.genres.draft = self.genres.list.iter().find(|p| p.id == id).cloned();
        self.genres.section = None;
        if other {
            self.genres.scroll.set_offset(point(px(0.), px(0.)));
        }
        self.genre_fields(cx);
        cx.notify();
    }

    /// Fields for the draft's texts, each saving what's typed into it.
    pub(crate) fn genre_fields(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            self.genres.fields = None;
            return;
        };
        let mut subs = Vec::new();
        let mut field = |ws: &mut Self, text: &str, hint: &str, what: Text| {
            let f = cx.new(|cx| match what {
                Text::Name => TextInput::form(text, hint, cx).typed(36., 44., FontWeight::BOLD),
                _ => TextInput::form(text, hint, cx),
            });
            subs.push(cx.subscribe(&f, move |ws, _, e: &TextEvent, cx| {
                if let TextEvent::Confirm(t) = e {
                    ws.set_genre_text(what, t.clone(), cx);
                }
            }));
            let _ = ws;
            f
        };
        let name = field(self, &p.name, "Name", Text::Name);
        let harmony = field(
            self,
            &p.harmony,
            "Words for its chords: dark, soulful…",
            Text::Harmony,
        );
        let sounds = (0..p.parts.len())
            .map(|i| {
                field(
                    self,
                    &p.parts[i].sound,
                    "Sounds to look for",
                    Text::Sound(i),
                )
            })
            .collect();
        let writes = (0..p.parts.len())
            .map(|i| field(self, &p.parts[i].write, "How to write it", Text::Write(i)))
            .collect();
        let notes = (0..p.notes.len())
            .map(|i| field(self, &p.notes[i], "A note for the agent", Text::Note(i)))
            .collect();
        let section = self
            .genres
            .section
            .and_then(|s| p.sections.get(s))
            .map(|s| s.name.clone());
        let section = section.map(|name| {
            let i = self.genres.section.unwrap();
            field(self, &name, "Section", Text::Section(i))
        });
        self.genres.fields = Some(Fields {
            name,
            harmony,
            sounds,
            writes,
            notes,
            section,
            _subs: subs,
        });
    }

    pub(crate) fn set_genre_text(&mut self, what: Text, t: String, cx: &mut Context<Self>) {
        self.edit_genre(false, cx, move |p| match what {
            Text::Name if !t.is_empty() => p.name = t,
            Text::Name => {}
            Text::Harmony => p.harmony = t,
            Text::Sound(i) => {
                if let Some(x) = p.parts.get_mut(i) {
                    x.sound = t
                }
            }
            Text::Write(i) => {
                if let Some(x) = p.parts.get_mut(i) {
                    x.write = t
                }
            }
            Text::Note(i) if t.is_empty() && i < p.notes.len() => drop(p.notes.remove(i)),
            Text::Note(i) => {
                if let Some(x) = p.notes.get_mut(i) {
                    *x = t
                }
            }
            Text::Section(i) if !t.is_empty() => {
                if let Some(s) = p.sections.get_mut(i) {
                    s.name = t
                }
            }
            Text::Section(_) => {}
        });
    }

    /// Changes the genre being edited and saves it; `shape` when parts, sections or notes come
    /// or go, so the fields are made again.
    pub(crate) fn edit_genre(
        &mut self,
        shape: bool,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Playbook),
    ) {
        let Some(p) = self.genres.draft.as_mut() else {
            return;
        };
        let before = (p.parts.len(), p.notes.len(), p.sections.len());
        f(p);
        let shape = shape || before != (p.parts.len(), p.notes.len(), p.sections.len());
        let saved = p.clone();
        if let Some(s) = self.genres.section
            && s >= saved.sections.len()
        {
            self.genres.section = saved.sections.len().checked_sub(1);
        }
        self.save_genre(&saved, cx);
        if shape {
            self.genre_fields(cx);
        }
        cx.notify();
    }

    pub(crate) fn save_genre(&mut self, p: &Playbook, cx: &mut Context<Self>) {
        match playbook::save(p) {
            Ok(_) => {
                match self.genres.list.iter_mut().find(|x| x.id == p.id) {
                    Some(x) => *x = p.clone(),
                    None => self.genres.list.push(p.clone()),
                }
                self.status = Some(format!("Saved {}", p.name));
            }
            Err(e) => self.status = Some(format!("{}: {e}", p.name)),
        }
        cx.notify();
    }

    /// A copy of the genre being edited, as a new genre of the person's.
    pub(crate) fn duplicate_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        let taken: Vec<String> = self.genres.list.iter().map(|p| p.id.clone()).collect();
        let name = format!("{} 2", p.name);
        let copy = Playbook {
            id: playbook::new_id(&name, &taken),
            name,
            ..p
        };
        self.save_genre(&copy, cx);
        self.select_genre(&copy.id.clone(), cx);
    }

    /// Pina Studio's own back, for a genre the person changed; or the person's genre gone.
    pub(crate) fn reset_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        if let Err(e) = playbook::remove(&p.id) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let gone = playbook::origin(&p) == Origin::Yours;
        self.status = Some(match gone {
            true => format!("Deleted {}", p.name),
            false => format!("{} is Pina Studio's again", p.name),
        });
        let next = match gone {
            true => None,
            false => Some(p.id.as_str()),
        };
        self.genres.draft = None;
        self.open_genres(next, cx);
    }

    /// The song plays this genre: what its agents and critic aim at.
    pub(crate) fn use_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        let tempo = self.session.project.tempo;
        self.edit(cx, |ws| ws.session.project.genre = Some(p.id.clone()));
        self.status = Some(match tempo < p.tempo.0 || tempo > p.tempo.1 {
            true => format!(
                "This song is {} now. Its {tempo:.0} BPM is outside {}'s {:.0}–{:.0}.",
                p.name, p.name, p.tempo.0, p.tempo.1
            ),
            false => format!(
                "This song is {} now: its agents and critic follow it.",
                p.name
            ),
        });
    }
}
