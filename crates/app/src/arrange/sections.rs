//! Sections in the marker strip: a marker's section runs to the next. Dragged, a section moves
//! with everything in it (dropped on another's middle, the two swap; ⌥, a copy goes; ⌘, the
//! marker alone moves); its menu deletes it (the gap closed), clears it, copies it to the end,
//! puts a bar before it or cycles it; + adds one after the last.

use crate::{
    ui::menu::{MARKER_NAMES, MenuAction},
    workspace::{MarkerHow, Workspace},
};

/// Where a dragged section would go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionDrop {
    /// It and this one change places.
    Swap(usize),
    /// Before this section (one past the last: the end).
    Before(usize),
}

/// Something the sections' menus and buttons do.
#[derive(Clone, Debug, PartialEq)]
pub enum SectionOp {
    Delete(usize),
    Clear(usize),
    CopyToEnd(usize),
    /// A bar of nothing before it.
    BarBefore(usize),
    Cycle(usize),
    Add,
}

impl Workspace {
    /// Where section `i`, dragged `how`, would go if let go at window x `x`: none where it is.
    pub fn section_drop(&self, i: usize, x: f32, how: MarkerHow) -> Option<SectionDrop> {
        let sections = self.session.project.sections();
        let (first, last) = (sections.first()?, sections.last()?);
        let t = self.tick_at(x) as u64;
        let at = match t {
            t if t < first.0 => SectionDrop::Before(0),
            t if t >= last.1 => SectionDrop::Before(sections.len()),
            t => {
                let k = sections.iter().position(|s| t >= s.0 && t < s.1)?;
                let (a, b) = sections[k];
                let quarter = (b - a) / 4;
                match how != MarkerHow::Copy && k != i && t > a + quarter && t < b - quarter {
                    true => SectionDrop::Swap(k),
                    false => SectionDrop::Before(if t < a + (b - a) / 2 { k } else { k + 1 }),
                }
            }
        };
        match (at, how) {
            (SectionDrop::Before(b), MarkerHow::Section) if b == i || b == i + 1 => None,
            _ => Some(at),
        }
    }

    /// Lets go of a dragged section: it goes where `section_drop` says. Whether anything moved.
    pub fn drop_section(&mut self, i: usize, x: f32, how: MarkerHow) -> bool {
        let Some(at) = self.section_drop(i, x, how) else {
            return false;
        };
        let names: Vec<String> = self
            .session
            .project
            .markers
            .iter()
            .map(|m| m.name.clone())
            .collect();
        let p = &mut self.session.project;
        let done = match (at, how) {
            (SectionDrop::Swap(k), _) => p.swap_sections(i, k),
            (SectionDrop::Before(b), MarkerHow::Copy) => p.copy_section(i, b),
            (SectionDrop::Before(b), _) => p.move_section(i, b),
        };
        let name = names.get(i).cloned().unwrap_or_default();
        self.status = Some(match (done, at, how) {
            (Err(e), ..) => e,
            (Ok(()), SectionDrop::Swap(k), _) => format!(
                "{name} and {} changed places",
                names.get(k).cloned().unwrap_or_default()
            ),
            (Ok(()), _, MarkerHow::Copy) => format!("A copy of {name}, everything in it"),
            (Ok(()), ..) => format!("{name} moved, everything in it"),
        });
        true
    }

    /// The window x a section drop shows at: the boundary it goes in at.
    pub fn section_drop_x(&self, at: SectionDrop) -> Option<f32> {
        let sections = self.session.project.sections();
        let tick = match at {
            SectionDrop::Before(b) => sections
                .get(b)
                .map(|s| s.0)
                .or(sections.last().map(|s| s.1))?,
            SectionDrop::Swap(_) => return None,
        };
        Some(tick as f32 * self.px_per_tick() - self.scroll_x)
    }

    /// A section's menu items, after its naming ones.
    pub fn section_items(&self, i: usize) -> Vec<(String, MenuAction)> {
        let op = |label: &str, o: SectionOp| (label.to_string(), MenuAction::Section(o));
        vec![
            (String::new(), MenuAction::Nothing),
            op("Cycle It", SectionOp::Cycle(i)),
            op("Copy It to the End", SectionOp::CopyToEnd(i)),
            op("Insert a Bar Before It", SectionOp::BarBefore(i)),
            op("Clear It (keep its time)", SectionOp::Clear(i)),
            op("Delete Section (close the gap)", SectionOp::Delete(i)),
        ]
    }

    /// What the sections' menus and buttons do.
    pub fn section_op(&mut self, op: SectionOp, cx: &mut gpui::Context<Self>) {
        let mut said: Result<(), String> = Ok(());
        match op {
            SectionOp::Delete(i) => self.edit(cx, |ws| said = ws.session.project.delete_section(i)),
            SectionOp::Clear(i) => self.edit(cx, |ws| said = ws.session.project.clear_section(i)),
            SectionOp::CopyToEnd(i) => self.edit(cx, |ws| {
                let n = ws.session.project.sections().len();
                said = ws.session.project.copy_section(i, n);
            }),
            SectionOp::BarBefore(i) => self.edit(cx, |ws| {
                let p = &mut ws.session.project;
                // The bar is the section before's (or the song's start).
                if let Some(&(from, _)) = p.sections().get(i) {
                    let bar = p.bar();
                    p.insert_time(from, bar);
                }
            }),
            SectionOp::Cycle(i) => {
                if let Some(&(start, end)) = self.session.project.sections().get(i) {
                    self.edit(cx, |ws| {
                        ws.session.project.cycle = Some(pina_engine::song::project::Cycle {
                            start,
                            end,
                            off: false,
                        })
                    });
                }
            }
            SectionOp::Add => {
                let n = self.session.project.markers.len();
                let name = MARKER_NAMES
                    .get(n)
                    .map_or(format!("Section {}", n + 1), |s| s.to_string());
                self.edit(cx, |ws| {
                    ws.session.project.add_section(&name, 8);
                });
                self.pending_name = self
                    .session
                    .project
                    .markers
                    .iter()
                    .position(|m| m.name == name)
                    .map(crate::workspace::Name::Marker);
            }
        }
        if let Err(e) = said {
            self.status = Some(e);
        }
        cx.notify();
    }
}
