//! Conversations kept. Each lives in its song's folder, so it goes where the song goes; a new one
//! leaves the last on the shelf, and any can be opened and carried on. The agent carries on with
//! its own memory of it when it still has one; when it doesn't (the song's moved, or it's too old)
//! it's told what was said instead, so nothing the person sees changes.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use gpui::{ClipboardItem, MouseDownEvent, Point};
use serde::{Deserialize, Serialize};

use super::*;
use crate::{ui::text::TextInput, workspace::Name};

/// The song's folder for them.
pub const CONVERSATIONS_DIR: &str = "Conversations";
/// How much of what was said a new agent is told, when it can't remember.
const RECAP_CHARS: usize = 6_000;
/// A title's longest.
const TITLE_CHARS: usize = 60;

/// A conversation as it's kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Saved {
    pub title: String,
    /// Named by the person, so it isn't named again from what was asked.
    #[serde(default)]
    pub named: bool,
    pub agent: Kind,
    /// What the agent calls it, to carry it on.
    #[serde(default)]
    pub session: Option<String>,
    pub started: u64,
    pub updated: u64,
    pub items: Vec<Item>,
}

/// A conversation in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Past {
    pub id: String,
    pub title: String,
    pub agent: Kind,
    pub updated: u64,
    /// How many questions it has.
    pub asked: usize,
    /// All that was said, for the search.
    pub said: String,
}

/// Something done to a conversation, from its menu; by its place in the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatOp {
    Rename(usize),
    Copy(usize),
    Delete(usize),
    /// The one showing, all of it.
    CopyThis,
    /// What's selected in it.
    Picked,
}

fn folder(song: &Path) -> PathBuf {
    song.join(CONVERSATIONS_DIR)
}

fn file(song: &Path, id: &str) -> PathBuf {
    folder(song).join(format!("{id}.json"))
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Named by when it began, to the millisecond: two can't share a name.
fn new_id() -> String {
    let at = SystemTime::now().duration_since(UNIX_EPOCH);
    format!("{}", at.map_or(0, |d| d.as_millis()))
}

/// A title from what was asked first: its first line, cut at a word.
pub(crate) fn title_of(items: &[Item]) -> String {
    let asked = items.iter().find_map(|i| match i {
        Item::You(t) => t.lines().find(|l| !l.trim().is_empty()),
        _ => None,
    });
    let line = asked.unwrap_or("New conversation").trim();
    if line.chars().count() <= TITLE_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_CHARS).collect();
    let at = cut.rfind(' ').unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end())
}

fn past(id: String, s: &Saved) -> Past {
    let said: Vec<&str> = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::You(t) | Item::Words(t) => Some(&t[..]),
            _ => None,
        })
        .collect();
    Past {
        id,
        title: s.title.clone(),
        agent: s.agent,
        updated: s.updated,
        asked: s.items.iter().filter(|i| matches!(i, Item::You(_))).count(),
        said: format!("{} {}", s.title, said.join(" ")).to_lowercase(),
    }
}

fn scratch(song: &Path) -> bool {
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let temp = real(&std::env::temp_dir());
    real(song).starts_with(temp) || song.starts_with(std::env::temp_dir())
}

/// Whole or not at all: a file half written would be a conversation lost.
pub(crate) fn write(song: &Path, id: &str, saved: &Saved) -> Result<(), String> {
    // Tests open the demo song where it lies; its folder isn't theirs to write in.
    if cfg!(test) && !scratch(song) {
        return Ok(());
    }
    std::fs::create_dir_all(folder(song)).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
    let (to, part) = (file(song, id), file(song, &format!("{id}.part")));
    std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(part, to).map_err(|e| e.to_string())
}

/// A conversation as it was left: what was running when it was kept is over.
pub(crate) fn read(song: &Path, id: &str) -> Option<Saved> {
    let bytes = std::fs::read(file(song, id)).ok()?;
    let mut saved: Saved = serde_json::from_slice(&bytes).ok()?;
    saved.items.retain(|i| *i != Item::Thinking);
    for item in &mut saved.items {
        if let Item::Tool { step, .. } = item
            && *step == Step::Running
        {
            *step = Step::Done;
        }
    }
    Some(saved)
}

/// A song's conversations, the latest first. A file that can't be read is left alone.
pub(crate) fn list(song: &Path) -> Vec<Past> {
    let files = std::fs::read_dir(folder(song)).into_iter().flatten();
    let mut all: Vec<Past> = files
        .flatten()
        .filter_map(|f| {
            let path = f.path();
            (path.extension()? == "json").then_some(())?;
            let id = path.file_stem()?.to_string_lossy().into_owned();
            let saved = read(song, &id)?;
            Some(past(id, &saved))
        })
        .collect();
    all.sort_by_key(|p| std::cmp::Reverse(p.updated));
    all
}

/// "Just now", "12 min ago", "3 h ago", "Yesterday", "4 days ago".
pub(crate) fn ago(secs: u64) -> String {
    let at = now().saturating_sub(secs);
    match at {
        0..60 => "Just now".into(),
        60..3_600 => format!("{} min ago", at / 60),
        3_600..86_400 => format!("{} h ago", at / 3_600),
        86_400..172_800 => "Yesterday".into(),
        _ => format!("{} days ago", at / 86_400),
    }
}

/// Which heading a conversation goes under.
fn group(secs: u64) -> &'static str {
    match now().saturating_sub(secs) / 86_400 {
        0 => "Today",
        1 => "Yesterday",
        2..7 => "This week",
        _ => "Earlier",
    }
}

/// What the agent said in a turn or conversation, as text to paste: its words, and what was asked.
pub(crate) fn as_text(title: &str, items: &[Item]) -> String {
    let mut out = format!("# {title}\n");
    for item in items {
        match item {
            Item::You(t) => out.push_str(&format!("\n**You:** {t}\n")),
            Item::Words(t) => out.push_str(&format!("\n{}\n", t.trim())),
            Item::End { error: Some(e), .. } => out.push_str(&format!("\n> {e}\n")),
            _ => {}
        }
    }
    out
}

impl Chat {
    /// What's been said, for an agent that wasn't there: the latest of it if there's a lot.
    pub(crate) fn recap(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let mut room = RECAP_CHARS;
        for item in self.items.iter().rev() {
            let line = match item {
                Item::You(t) => format!("The person: {t}"),
                Item::Words(t) => format!("You: {}", t.trim()),
                _ => continue,
            };
            if line.len() > room {
                break;
            }
            room -= line.len();
            lines.push(line);
        }
        lines.reverse();
        lines.join("\n\n")
    }

    /// Whether a conversation's open with something in it.
    pub(crate) fn has_questions(&self) -> bool {
        self.items.iter().any(|i| matches!(i, Item::You(_)))
    }

    fn entry(&mut self, entry: Past) {
        self.past.retain(|p| p.id != entry.id);
        self.past.insert(0, entry);
    }
}

impl Workspace {
    /// Keeps the conversation showing, once something's been asked in it.
    pub(crate) fn keep_chat(&mut self) {
        let dir = self.session.dir.clone();
        let c = &mut self.chat;
        if !c.has_questions() {
            return;
        }
        let id = c.id.get_or_insert_with(new_id).clone();
        c.session = c
            .agent
            .as_ref()
            .and_then(Agent::session)
            .or(c.session.take());
        if !c.named {
            c.title = title_of(&c.items);
        }
        let saved = Saved {
            title: c.title.clone(),
            named: c.named,
            agent: c.kind,
            session: c.session.clone(),
            started: *c.started.get_or_insert_with(now),
            updated: now(),
            items: c
                .items
                .iter()
                .filter(|i| **i != Item::Thinking)
                .cloned()
                .collect(),
        };
        match write(&dir, &id, &saved) {
            Ok(()) => c.entry(past(id, &saved)),
            Err(e) => self.status = Some(format!("Couldn't keep the conversation: {e}")),
        }
    }

    /// The song's conversations read, once for each song, off the UI thread.
    pub(crate) fn past_sync(&mut self, cx: &mut Context<Self>) {
        if self.chat.past_of == self.session.dir {
            return;
        }
        let dir = self.session.dir.clone();
        self.chat.past_of = dir.clone();
        self.chat.past.clear();
        let read = cx.background_spawn({
            let dir = dir.clone();
            async move { list(&dir) }
        });
        cx.spawn(async move |this, cx| {
            let all = read.await;
            this.update(cx, |ws, cx| {
                if ws.chat.past_of == dir {
                    ws.chat.past = all;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens a conversation, to read or carry on. The one showing is kept first.
    pub fn open_chat(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.chat.busy {
            self.status = Some("It's still working: stop it first.".into());
            cx.notify();
            return;
        }
        if self.chat.id.as_deref() == Some(id) {
            self.chat.page = ChatPage::Chat;
            cx.notify();
            return;
        }
        self.keep_chat();
        let Some(saved) = read(&self.session.dir, id) else {
            self.chat.past.retain(|p| p.id != id);
            self.status = Some("That conversation couldn't be read.".into());
            cx.notify();
            return;
        };
        let c = &mut self.chat;
        c.run += 1;
        c.agent = None;
        c.palette_heard = None;
        c.before = None;
        c.pick = None;
        c.opened.clear();
        c.remote = false;
        c.since = None;
        c.id = Some(id.to_string());
        c.title = saved.title;
        c.named = saved.named;
        c.started = Some(saved.started);
        c.session = saved.session;
        c.items = saved.items;
        c.page = ChatPage::Chat;
        if saved.agent != c.kind {
            c.kind = saved.agent;
            c.model = None;
            c.tuning = agent::tuning(saved.agent);
        }
        c.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(crate) fn chat_op(&mut self, op: ChatOp, cx: &mut Context<Self>) {
        let dir = self.session.dir.clone();
        match op {
            ChatOp::CopyThis => {
                let text = as_text(&self.chat.title, &self.chat.items);
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.status = Some("Conversation copied.".into());
            }
            ChatOp::Picked => {
                self.copy_chat_pick(cx);
            }
            ChatOp::Copy(i) => {
                let saved = self.chat.past.get(i).and_then(|p| read(&dir, &p.id));
                if let Some(s) = saved {
                    let text = as_text(&s.title, &s.items);
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.status = Some("Conversation copied.".into());
                }
            }
            ChatOp::Rename(i) => self.pending_name = Some(Name::Chat(i)),
            ChatOp::Delete(i) => {
                let Some(p) = self.chat.past.get(i).cloned() else {
                    return;
                };
                if std::fs::remove_file(file(&dir, &p.id)).is_err() {
                    self.status = Some("That conversation couldn't be deleted.".into());
                }
                self.chat.past.remove(i);
                // The one showing goes with it.
                if self.chat.id.as_deref() == Some(&p.id) {
                    self.clear_chat();
                }
                self.status = Some(format!("“{}” deleted.", p.title));
            }
        }
        cx.notify();
    }

    /// A name typed for a conversation in the list.
    pub(crate) fn name_chat(&mut self, i: usize, title: String) {
        let dir = self.session.dir.clone();
        let Some(p) = self.chat.past.get_mut(i) else {
            return;
        };
        p.title = title.clone();
        let id = p.id.clone();
        if self.chat.id.as_deref() == Some(&id) {
            self.chat.title = title.clone();
            self.chat.named = true;
        }
        if let Some(mut saved) = read(&dir, &id) {
            saved.title = title;
            saved.named = true;
            if let Err(e) = write(&dir, &id, &saved) {
                self.status = Some(format!("Couldn't rename it: {e}"));
            }
        }
    }

    /// The menu of the conversation showing.
    pub(crate) fn this_chat_menu(&self, at: Point<gpui::Pixels>) -> Menu {
        let id = self.chat.id.as_deref();
        match self.chat.past.iter().position(|p| Some(&p.id[..]) == id) {
            Some(i) => {
                let mut menu = self.chat_menu(i, at);
                menu.items[1].1 = MenuAction::Chat(ChatOp::CopyThis);
                menu
            }
            None => Menu {
                at,
                items: vec![(
                    "Copy conversation".to_string(),
                    MenuAction::Chat(ChatOp::CopyThis),
                )],
                ..Default::default()
            },
        }
    }

    fn chat_menu(&self, i: usize, at: Point<gpui::Pixels>) -> Menu {
        let item = |label: &str, op: ChatOp| (label.to_string(), MenuAction::Chat(op));
        Menu {
            at,
            items: vec![
                item("Rename", ChatOp::Rename(i)),
                item("Copy conversation", ChatOp::Copy(i)),
                (String::new(), MenuAction::Nothing),
                item("Delete", ChatOp::Delete(i)),
            ],
            ..Default::default()
        }
    }

    fn past_row(&self, i: usize, p: &Past, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let open = self.chat.id.as_deref() == Some(&p.id);
        let id = p.id.clone();
        let questions = match p.asked {
            1 => "1 question".to_string(),
            n => format!("{n} questions"),
        };
        let group_name = format!("past-{i}");
        div()
            .id(("past", i))
            .group(group_name.clone())
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(9.))
            .cursor_pointer()
            .when(open, |d| d.bg(rgb(RAISED)))
            .when(!open, |d| d.hover(|d| d.bg(rgb(CARD))))
            .debug_selector(move || format!("past-{i}"))
            .child(logo(p.agent, 13.).flex_none().opacity(0.8))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(match self.name_field(Name::Chat(i)) {
                        Some(field) => div().child(field),
                        None => div()
                            .text_size(px(12.5))
                            .line_height(px(17.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if open { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(p.title.clone()),
                    })
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .whitespace_nowrap()
                            .child(format!("{questions} · {}", ago(p.updated))),
                    ),
            )
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .invisible()
                    .group_hover(group_name, |d| d.visible())
                    .hover(|d| d.bg(rgb(LINE_STRONG)))
                    .debug_selector(move || format!("past-menu-{i}"))
                    .child(icon(Icon::More, 12.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(ws.chat_menu(i, e.position));
                            cx.notify();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.menu = Some(ws.chat_menu(i, e.position));
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.open_chat(&id, cx)),
            )
    }

    /// The last few, for the page a new conversation starts on.
    pub(crate) fn recent_chats(&self, cx: &mut Context<Self>) -> Option<Div> {
        let rows: Vec<_> = self
            .chat
            .past
            .iter()
            .take(3)
            .enumerate()
            .map(|(i, p)| self.past_row(i, p, cx))
            .collect();
        (!rows.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(section_title("Pick up where you left off"))
                .children(rows)
        })
    }

    /// Every conversation with this song, the latest first, under when they were; a search.
    pub(crate) fn agent_history(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let c = &self.chat;
        let words = c.find.read(cx).text().trim().to_lowercase();
        let shown: Vec<(usize, &Past)> = c
            .past
            .iter()
            .enumerate()
            .filter(|(_, p)| words.split_whitespace().all(|w| p.said.contains(w)))
            .collect();
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        let mut heading = "";
        for (i, p) in &shown {
            let now = group(p.updated);
            if now != heading {
                heading = now;
                rows.push(section_title(now).into_any_element());
            }
            rows.push(self.past_row(*i, p, cx).into_any_element());
        }
        let empty = match (c.past.is_empty(), rows.is_empty()) {
            (true, _) => Some("Conversations about this song stay here."),
            (_, true) => Some("No conversation says that."),
            _ => None,
        };
        div()
            .id("agent-history")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(8.))
            .pb(px(10.))
            .child(
                div()
                    .mx(px(2.))
                    .mb(px(6.))
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(8.))
                    .bg(rgb(WELL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .debug_selector(|| "agent-find".into())
                    .child(icon(Icon::Search, 13.).text_color(rgb(TEXT_3)))
                    .child(div().flex_1().min_w(px(0.)).child(c.find.clone())),
            )
            .children(rows)
            .children(empty.map(|text| {
                div()
                    .py(px(28.))
                    .flex()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_4))
                    .child(text)
            }))
    }
}

/// A small heading in the panel.
pub(crate) fn section_title(text: &'static str) -> Div {
    div()
        .px(px(10.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_4))
        .child(text)
}

/// The search field, as the panel makes it.
pub(crate) fn find_field(cx: &mut Context<Workspace>) -> Entity<TextInput> {
    cx.new(|cx| TextInput::search("Search conversations", cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pina-chats-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn saved(title: &str, updated: u64, items: Vec<Item>) -> Saved {
        Saved {
            title: title.into(),
            named: false,
            agent: Kind::ClaudeCode,
            session: Some("abc".into()),
            started: updated,
            updated,
            items,
        }
    }

    #[test]
    fn a_conversation_comes_back_as_it_was_and_what_was_running_is_over() {
        let d = dir("back");
        let items = vec![
            Item::You("Make it darker".into()),
            Item::Tool {
                id: "1".into(),
                name: "mcp__pina__edit".into(),
                input: serde_json::json!({ "command": { "op": "set_tempo", "bpm": 90 } }),
                step: Step::Running,
            },
            Item::Words("Done.".into()),
            Item::Thinking,
        ];
        write(&d, "a", &saved("Darker", 100, items)).unwrap();
        let back = read(&d, "a").unwrap();
        assert_eq!(back.session.as_deref(), Some("abc"));
        assert_eq!(back.items.len(), 3, "thinking wasn't kept");
        assert!(matches!(
            back.items[1],
            Item::Tool {
                step: Step::Done,
                ..
            }
        ));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn the_list_is_the_latest_first_and_a_bad_file_loses_nothing() {
        let d = dir("list");
        let you = |t: &str| vec![Item::You(t.into())];
        write(&d, "old", &saved("Old", 100, you("first"))).unwrap();
        write(&d, "new", &saved("New", 200, you("second"))).unwrap();
        std::fs::write(file(&d, "torn"), "{ not json").unwrap();
        let all = list(&d);
        let ids: Vec<&str> = all.iter().map(|p| &p.id[..]).collect();
        assert_eq!(ids, ["new", "old"]);
        assert_eq!(all[0].asked, 1);
        assert!(all[0].said.contains("second"));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_title_is_what_was_asked_first_cut_at_a_word() {
        let long =
            "Make a very long intro that builds for sixteen bars and then drops into a huge chorus";
        let title = title_of(&[Item::You(format!("\n{long}\nand more"))]);
        assert!(
            title.ends_with('…') && title.chars().count() <= TITLE_CHARS + 1,
            "{title}"
        );
        assert!(long.starts_with(title.trim_end_matches('…')));
        assert_eq!(title_of(&[]), "New conversation");
    }

    #[test]
    fn what_an_agent_is_told_is_the_latest_of_what_was_said() {
        let words = |n: usize| "x".repeat(n);
        let items = [
            Item::You(words(RECAP_CHARS)),
            Item::Words("kept 1".into()),
            Item::You("kept 2".into()),
        ];
        let mut lines = Vec::new();
        let mut room = RECAP_CHARS;
        for i in items.iter().rev() {
            let line = match i {
                Item::You(t) | Item::Words(t) => t.clone(),
                _ => continue,
            };
            if line.len() > room {
                break;
            }
            room -= line.len();
            lines.push(line);
        }
        assert_eq!(lines, ["kept 2", "kept 1"]);
    }

    #[test]
    fn a_conversation_copies_as_text_without_its_steps() {
        let items = vec![
            Item::You("Add a bass".into()),
            Item::Tool {
                id: "1".into(),
                name: "edit".into(),
                input: Value::Null,
                step: Step::Done,
            },
            Item::Words("Added a bass.".into()),
        ];
        let text = as_text("Bass", &items);
        assert_eq!(text, "# Bass\n\n**You:** Add a bass\n\nAdded a bass.\n");
    }
}
