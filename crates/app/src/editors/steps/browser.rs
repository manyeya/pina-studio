//! The pattern browser: Pina Studio's patterns and templates and the user's own, in folders,
//! searched by name; a click loads one into the pattern region. Its action menu saves the
//! pattern (or a template of it), makes and removes folders, and puts patterns in the song's
//! key.

use std::path::PathBuf;

use crate::workspace::Name;

use super::{library::Entry, *};

/// One line of a browser column: a folder to open, or a pattern to load.
enum Item {
    Folder(Vec<String>),
    Pattern(Entry),
}

impl Workspace {
    /// The user folder a save or a new folder goes in: the one open, else User's Patterns (or
    /// Templates).
    fn save_dir(&self, template: bool) -> PathBuf {
        match self.steps.folder.split_first() {
            Some((top, rest)) if top == "User" && !rest.is_empty() => {
                rest.iter().fold(library::user_dir(), |d, f| d.join(f))
            }
            _ => library::user_dir().join(if template { "Templates" } else { "Patterns" }),
        }
    }

    /// What a folder holds: its folders and its patterns, by name.
    fn folder_items(&self, path: &[String]) -> Vec<(String, Item)> {
        match path {
            [] => library::FOLDERS
                .iter()
                .map(|f| (f.to_string(), Item::Folder(vec![f.to_string()])))
                .collect(),
            [top, rest @ ..] if top == "User" => {
                let dir = rest.iter().fold(library::user_dir(), |d, f| d.join(f));
                let (mut folders, files) = library::user_folder(&dir);
                if rest.is_empty() {
                    for f in ["Patterns", "Templates"] {
                        if !folders.iter().any(|x| x == f) {
                            folders.push(f.into());
                        }
                    }
                    folders.sort();
                }
                folders
                    .into_iter()
                    .map(|f| {
                        let mut p = path.to_vec();
                        p.push(f.clone());
                        (f, Item::Folder(p))
                    })
                    .chain(
                        files
                            .into_iter()
                            .map(|(n, p)| (n, Item::Pattern(Entry::User(p)))),
                    )
                    .collect()
            }
            [top] => {
                let folder = library::FOLDERS
                    .iter()
                    .copied()
                    .find(|f| f == top)
                    .unwrap_or("Drums");
                library::factory(folder)
                    .into_iter()
                    .map(|(n, _)| (n.clone(), Item::Pattern(Entry::Factory(folder, n))))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Every pattern whose name has every word typed.
    fn search_items(&self) -> Vec<(String, Item)> {
        let words: Vec<String> = self
            .steps
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let hit = |n: &str| words.iter().all(|w| n.to_lowercase().contains(w));
        let mut out = Vec::new();
        for folder in &library::FOLDERS[1..] {
            for (n, _) in library::factory(folder) {
                if hit(&n) {
                    out.push((
                        format!("{n}  ·  {folder}"),
                        Item::Pattern(Entry::Factory(folder, n)),
                    ));
                }
            }
        }
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, PathBuf)>) {
            let (folders, files) = library::user_folder(dir);
            out.extend(files);
            for f in folders {
                walk(&dir.join(f), out);
            }
        }
        let mut mine = Vec::new();
        walk(&library::user_dir(), &mut mine);
        for (n, p) in mine {
            if hit(&n) {
                out.push((format!("{n}  ·  User"), Item::Pattern(Entry::User(p))));
            }
        }
        out
    }

    pub(crate) fn pattern_browser(&mut self, cx: &mut Context<Self>) -> Div {
        let folder = self.steps.folder.clone();
        let query = self.steps.query.clone();
        let searching = !query.is_empty();
        let search_field = self.name_field(Name::PatternSearch);
        let save_field = [
            Name::PatternSave(false),
            Name::PatternSave(true),
            Name::PatternFolder,
        ]
        .into_iter()
        .find_map(|n| self.name_field(n).map(|f| (n, f)));
        let line = |label: String, item: Item, open: bool, k: usize, cx: &mut Context<Self>| {
            let is_folder = matches!(item, Item::Folder(_));
            div()
                .h(px(24.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(4.))
                .rounded(px(5.))
                .text_size(px(12.))
                .whitespace_nowrap()
                .overflow_hidden()
                .cursor_pointer()
                .bg(if open {
                    rgb(RAISED).into()
                } else {
                    alpha(0, 0.0)
                })
                .text_color(rgb(if open { TEXT } else { TEXT_2 }))
                .hover(|s| s.bg(rgb(OFF)))
                .debug_selector(move || format!("steps-browse-{k}"))
                .child(
                    div()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(label),
                )
                .when(is_folder, |d| {
                    d.child(icon(Icon::ChevronRight, 11.).text_color(rgb(TEXT_3)))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| match &item {
                        Item::Folder(p) => {
                            ws.steps.folder = p.clone();
                            cx.notify();
                        }
                        Item::Pattern(e) => ws.steps_op(StepsOp::Load(e.clone()), cx),
                    }),
                )
        };
        // Two columns: the folder open and what's in it (or, deeper, its parent and it).
        let columns: Vec<Div> = match searching {
            true => {
                let items = self.search_items();
                vec![
                    div().flex_1().flex().flex_col().gap(px(1.)).children(
                        items
                            .into_iter()
                            .enumerate()
                            .map(|(k, (label, item))| line(label, item, false, k, cx)),
                    ),
                ]
            }
            false => {
                let (left_path, right_path) = match folder.len() {
                    0 => (Vec::new(), None),
                    n => (folder[..n - 1].to_vec(), Some(folder.clone())),
                };
                let left = self.folder_items(&left_path);
                let right = right_path
                    .map(|p| self.folder_items(&p))
                    .unwrap_or_default();
                let open = folder.last().cloned();
                vec![
                    div()
                        .w(px(118.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .children(left.into_iter().enumerate().map(|(k, (label, item))| {
                            let on = open.as_deref() == Some(label.as_str());
                            line(label, item, on, k, cx)
                        })),
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .pl(px(4.))
                        .border_l_1()
                        .border_color(rgb(LINE))
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .children(
                            right
                                .into_iter()
                                .enumerate()
                                .map(|(k, (label, item))| line(label, item, false, 100 + k, cx)),
                        ),
                ]
            }
        };
        let crumbs = match folder.is_empty() {
            true => "Patterns".to_string(),
            false => format!("Patterns › {}", folder.join(" › ")),
        };
        div()
            .w(px(PANE_W + 50.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .p(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(match search_field {
                        Some(input) => div().flex_1().h(px(24.)).child(input),
                        None => div()
                            .flex_1()
                            .h(px(24.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(6.))
                            .bg(rgb(FIELD))
                            .text_size(px(11.5))
                            .cursor_text()
                            .debug_selector(|| "steps-search".into())
                            .child(icon(Icon::Search, 12.).text_color(rgb(TEXT_3)))
                            .child(
                                div()
                                    .text_color(rgb(if searching { TEXT } else { TEXT_4 }))
                                    .child(if searching {
                                        query.clone()
                                    } else {
                                        "Search Patterns".into()
                                    }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, window, cx| {
                                    ws.start_naming(Name::PatternSearch, window, cx);
                                }),
                            ),
                    })
                    .when(searching, |d| {
                        d.child(icon_seg(Icon::Close, false, 22.).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.steps.query.clear();
                                cx.notify();
                            }),
                        ))
                    })
                    .child(
                        icon_seg(Icon::More, false, 24.)
                            .debug_selector(|| "steps-browser-actions".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let user_folder =
                                        ws.steps.folder.len() > 2 && ws.steps.folder[0] == "User";
                                    let op = |l: &str, op: StepsOp| {
                                        (l.to_string(), MenuAction::Steps(op))
                                    };
                                    let mut items = vec![
                                        op("Save Pattern…\t⇧⌘P", StepsOp::SavePattern),
                                        op("Save Template…\t⇧⌘T", StepsOp::SaveTemplate),
                                        (String::new(), MenuAction::Nothing),
                                        op("New Folder…", StepsOp::NewFolder),
                                    ];
                                    items.push(match user_folder {
                                        true => op("Delete Folder", StepsOp::DeleteFolder),
                                        false => ("Delete Folder".into(), MenuAction::Nothing),
                                    });
                                    items.push((String::new(), MenuAction::Nothing));
                                    items.push(op("Use Project Key", StepsOp::UseProjectKey));
                                    ws.open_steps_menu(e.position, items, cx);
                                }),
                            ),
                    ),
            )
            .children(save_field.map(|(n, input)| {
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(match n {
                                Name::PatternFolder => "Folder",
                                Name::PatternSave(true) => "Template",
                                _ => "Pattern",
                            }),
                    )
                    .child(div().flex_1().h(px(24.)).child(input))
            }))
            .child(
                div()
                    .id("steps-browser-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(6.))
                    .flex()
                    .children(columns),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .when(!folder.is_empty(), |d| {
                        d.child(icon(Icon::Back, 11.).text_color(rgb(TEXT_2)))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.steps.folder.pop();
                                    cx.notify();
                                }),
                            )
                    })
                    .child(crumbs),
            )
    }

    /// A pattern from the browser into the shown region: a template's rows and settings, or the
    /// whole pattern; one of Pina Studio's melodic ones in the song's key. The region grows to
    /// hold it once through.
    pub(crate) fn load_pattern(&mut self, entry: &Entry, cx: &mut Context<Self>) {
        let Some((name, mut p, _template)) = library::read(entry) else {
            self.status = Some("That pattern couldn't be read.".into());
            cx.notify();
            return;
        };
        let Some(at) = self.pattern() else {
            return;
        };
        if let Entry::Factory(folder, _) = entry
            && matches!(*folder, "Bass" | "Melodic")
            && let Some(key) = self.session.project.song_key()
        {
            p.set_key(key.root, true);
        }
        let span = p.step * p.steps() as u64;
        self.edit(cx, |ws| {
            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
            r.content = Content::Pattern(p);
            r.length = r.length.max(span);
        });
        self.steps.row = 0;
        self.steps.picked.clear();
        self.status = Some(format!("Loaded {name}"));
    }

    /// Saves the shown pattern (or a template of it: its settings and rows, no steps on) in the
    /// open user folder, under `name`.
    pub(crate) fn write_pattern(&mut self, name: &str, template: bool) {
        let Some((_, p)) = self.shown_pattern() else {
            return;
        };
        let mut pattern = p.clone();
        if template {
            for row in &mut pattern.rows {
                let n = row.cells.len();
                row.clear();
                row.cells = vec![0; n];
            }
        }
        let dir = self.save_dir(template);
        let file = dir.join(format!("{}.json", name.replace(['/', '\\', ':'], "-")));
        let saved = library::Saved {
            name: name.to_string(),
            template,
            pattern,
        };
        let done = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::to_string_pretty(&saved).map_err(|e| e.to_string()))
            .and_then(|json| std::fs::write(&file, json).map_err(|e| e.to_string()));
        self.status = Some(match done {
            Ok(()) => format!("Saved {name}"),
            Err(e) => format!("Couldn't save {name}: {e}"),
        });
    }

    pub(crate) fn save_pattern(&mut self, template: bool, _cx: &mut Context<Self>) {
        self.steps.ask = Some(Name::PatternSave(template));
    }

    pub(crate) fn new_pattern_folder(&mut self, _cx: &mut Context<Self>) {
        self.steps.ask = Some(Name::PatternFolder);
    }

    /// A folder made in the open user folder.
    pub(crate) fn make_pattern_folder(&mut self, name: &str) {
        let dir = self
            .save_dir(false)
            .join(name.replace(['/', '\\', ':'], "-"));
        self.status = Some(match std::fs::create_dir_all(&dir) {
            Ok(()) => format!("Made folder {name}"),
            Err(e) => format!("Couldn't make {name}: {e}"),
        });
    }

    /// The open user folder, moved to the Trash.
    pub(crate) fn delete_pattern_folder(&mut self, cx: &mut Context<Self>) {
        let f = &self.steps.folder;
        if f.len() < 3 || f[0] != "User" {
            return;
        }
        let dir = f[1..].iter().fold(library::user_dir(), |d, x| d.join(x));
        self.status = Some(match crate::pages::projects::files::trash(&dir) {
            Ok(()) => {
                self.steps.folder.pop();
                "Folder moved to the Trash".into()
            }
            Err(e) => format!("Couldn't remove the folder: {e}"),
        });
        cx.notify();
    }
}
