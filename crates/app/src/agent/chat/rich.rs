//! What the agent says, set as a person would set it: headings, lists, tables and code, bold and
//! code within a line. All of it can be selected across lines and copied, as text on a page can;
//! each piece of text is told apart by its place in the conversation as it was last drawn.

use std::{ops::Range, time::Instant};

use gpui::{ClipboardItem, HighlightStyle, MouseDownEvent, Pixels, Point, StyledText, TextLayout};

use super::*;

/// A block of what was said.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Heading(String),
    Para(String),
    Bullet(String),
    /// Its number as written, and what it says.
    Number(String, String),
    Quote(String),
    Code(String),
    /// Rows of cells, the first its heads.
    Table(Vec<Vec<String>>),
    Rule,
}

/// What's said, in blocks. A code block still being written runs to the end.
pub(crate) fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut lines = text.lines().peekable();
    let end = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(para.join(" ")));
            para.clear();
        }
    };
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.starts_with("```") {
            end(&mut para, &mut out);
            let mut code = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push(l);
            }
            out.push(Block::Code(code.join("\n")));
            continue;
        }
        if t.starts_with('|') {
            end(&mut para, &mut out);
            let mut rows = vec![cells(t)];
            while let Some(next) = lines.peek().map(|l| l.trim()) {
                if !next.starts_with('|') {
                    break;
                }
                let is_rule = next.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '));
                if !is_rule {
                    rows.push(cells(next));
                }
                lines.next();
            }
            out.push(Block::Table(rows));
            continue;
        }
        let done = if t.is_empty() {
            None
        } else if t.len() >= 3 && t.chars().all(|c| matches!(c, '-' | '*' | '_')) {
            Some(Block::Rule)
        } else if t.starts_with('#') {
            Some(Block::Heading(t.trim_start_matches('#').trim().to_string()))
        } else if let Some(rest) = t.strip_prefix("> ") {
            Some(Block::Quote(rest.to_string()))
        } else if let Some(rest) = ["- ", "* ", "• "].iter().find_map(|m| t.strip_prefix(m)) {
            Some(Block::Bullet(rest.to_string()))
        } else if let Some((n, rest)) = numbered(t) {
            Some(Block::Number(n.to_string(), rest.to_string()))
        } else {
            para.push(t);
            continue;
        };
        end(&mut para, &mut out);
        out.extend(done);
    }
    end(&mut para, &mut out);
    out
}

fn cells(row: &str) -> Vec<String> {
    let row = row.trim().trim_matches('|');
    row.split('|').map(|c| c.trim().to_string()).collect()
}

/// "12. what it says" as "12." and what it says.
fn numbered(t: &str) -> Option<(&str, &str)> {
    let digits = t.find(|c: char| !c.is_ascii_digit())?;
    let rest = t[digits..].strip_prefix(". ")?;
    (digits > 0).then(|| (&t[..=digits], rest))
}

/// How part of a line is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Bold,
    Italic,
    Code,
}

/// A line without its marks, and where they fell. A link keeps its words.
pub(crate) fn inline(s: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut out = String::new();
    let mut marks = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let closed = |open: &str, close: &str| {
            let body = rest.strip_prefix(open)?;
            let to = body.find(close)?;
            (to > 0).then(|| (&body[..to], &body[to + close.len()..]))
        };
        let found = [
            ("**", "**", Mark::Bold),
            ("__", "__", Mark::Bold),
            ("`", "`", Mark::Code),
            ("*", "*", Mark::Italic),
        ]
        .iter()
        .find_map(|&(open, close, mark)| Some((closed(open, close)?, mark)));
        if let Some(((body, after), mark)) = found
            && !body.starts_with(' ')
        {
            let at = out.len();
            // Code inside bold reads as bold.
            out.push_str(&body.replace('`', ""));
            marks.push((at..out.len(), mark));
            rest = after;
            continue;
        }
        if c == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close..].find(')')
        {
            out.push_str(&rest[1..close]);
            rest = &rest[close + end + 1..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    (out, marks)
}

/// A place in the conversation's text: which piece, and how far in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spot {
    pub block: usize,
    pub at: usize,
}

/// A piece of text as last drawn, and what comes between it and the next when copied.
#[derive(Clone)]
pub struct Marked {
    layout: TextLayout,
    pub text: String,
    gap: &'static str,
}

impl Marked {
    #[cfg(test)]
    pub(crate) fn bounds(&self) -> gpui::Bounds<Pixels> {
        self.layout.bounds()
    }
}

/// Sets the conversation's text, noting each piece for selecting.
pub(crate) struct Painter {
    pub shown: Vec<Marked>,
    picked: Option<(Spot, Spot)>,
}

fn in_order((a, b): (Spot, Spot)) -> (Spot, Spot) {
    if a <= b { (a, b) } else { (b, a) }
}

impl Painter {
    pub(crate) fn new(pick: Option<(Spot, Spot)>) -> Self {
        Painter {
            shown: Vec::new(),
            picked: pick.map(in_order),
        }
    }

    /// A piece of text, its marks set and what's selected of it lit.
    pub(crate) fn text(
        &mut self,
        text: String,
        marks: Vec<(Range<usize>, Mark)>,
        gap: &'static str,
    ) -> StyledText {
        let k = self.shown.len();
        let picked = self.picked.and_then(|(a, b)| {
            let from = if k == a.block { a.at } else { 0 };
            let to = if k == b.block { b.at } else { text.len() };
            ((a.block..=b.block).contains(&k) && from < to).then_some(from..to)
        });
        let t = styled(&text, &marks, picked);
        self.shown.push(Marked {
            layout: t.layout().clone(),
            text,
            gap,
        });
        t
    }

    /// A line with marks in it.
    pub(crate) fn line(&mut self, s: &str, gap: &'static str) -> StyledText {
        let (text, marks) = inline(s);
        self.text(text, marks, gap)
    }
}

fn styled(text: &str, marks: &[(Range<usize>, Mark)], picked: Option<Range<usize>>) -> StyledText {
    // Where a style starts or stops, so what's selected can run across what's marked.
    let mut cuts = vec![0, text.len()];
    cuts.extend(marks.iter().flat_map(|(r, _)| [r.start, r.end]));
    cuts.extend(picked.iter().flat_map(|r| [r.start, r.end]));
    cuts.sort_unstable();
    cuts.dedup();
    let spans = cuts.windows(2).filter_map(|w| {
        let r = w[0]..w[1];
        let mark = marks
            .iter()
            .find(|(m, _)| m.start <= r.start && r.end <= m.end)
            .map(|m| m.1);
        let chosen = picked
            .as_ref()
            .is_some_and(|p| p.start <= r.start && r.end <= p.end);
        (mark.is_some() || chosen).then(|| {
            let mut style = HighlightStyle::default();
            match mark {
                Some(Mark::Bold) => style.font_weight = Some(FontWeight::SEMIBOLD),
                Some(Mark::Italic) => style.font_style = Some(gpui::FontStyle::Italic),
                Some(Mark::Code) => {
                    style.color = Some(rgb(AGENT_TEXT).into());
                    style.background_color = Some(alpha(0xffffff, 0.07));
                }
                None => {}
            }
            if chosen {
                style.background_color = Some(alpha(ACCENT, 0.42));
            }
            (r, style)
        })
    });
    let code = marks
        .iter()
        .filter(|m| m.1 == Mark::Code)
        .map(|(r, _)| (r.clone(), gpui::SharedString::from(MONO)));
    StyledText::new(text.to_string())
        .with_highlights(spans.collect::<Vec<_>>())
        .with_font_family_overrides(code.collect::<Vec<_>>())
}

/// The word at `at`.
fn word(text: &str, at: usize) -> Range<usize> {
    let wordy = |c: char| c.is_alphanumeric() || c == '\'' || c == '-';
    let before = text[..at].rfind(|c| !wordy(c));
    let start = before.map_or(0, |i| {
        i + text[i..].chars().next().map_or(1, char::len_utf8)
    });
    let end = text[at..]
        .find(|c| !wordy(c))
        .map_or(text.len(), |i| at + i);
    start..end
}

impl Chat {
    /// The text under a point; `near`, the nearest there is to it.
    fn spot_at(&self, p: Point<Pixels>, near: bool) -> Option<Spot> {
        let shown = self.shown.borrow();
        let off = |m: &Marked| {
            let b = m.layout.bounds();
            let (top, bottom) = (b.origin.y, b.origin.y + b.size.height);
            let y = f32::from((top - p.y).max(p.y - bottom));
            let x = f32::from((b.origin.x - p.x).max(p.x - (b.origin.x + b.size.width)));
            (y.max(0.0), x.max(0.0))
        };
        let (block, m) = shown
            .iter()
            .enumerate()
            .min_by(|a, b| off(a.1).0.total_cmp(&off(b.1).0))?;
        let (y, x) = off(m);
        if !near && (y > 0.0 || x > 0.0) {
            return None;
        }
        let at = match m.layout.index_for_position(p) {
            Ok(i) | Err(i) => i,
        };
        Some(Spot { block, at })
    }

    /// What's selected, as text.
    pub(crate) fn picked_text(&self) -> Option<String> {
        let (a, b) = in_order(self.pick?);
        let shown = self.shown.borrow();
        let mut out = String::new();
        for k in a.block..=b.block {
            let m = shown.get(k)?;
            let from = if k == a.block { a.at } else { 0 };
            let to = if k == b.block { b.at } else { m.text.len() };
            out.push_str(m.text.get(from..to)?);
            if k != b.block {
                out.push_str(m.gap);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

impl Workspace {
    /// Pressed on the conversation: on its text, selecting starts (a double-click, a word);
    /// anywhere, what was selected lets go. The keyboard comes here, for ⌘C.
    pub(crate) fn chat_press(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        self.chat.pick = None;
        if let Some(spot) = self.chat.spot_at(e.position, false) {
            self.refocus = true;
            match e.click_count {
                1 => {
                    self.chat.pick = Some((spot, spot));
                    self.begin_drag(crate::workspace::Drag::ChatText, cx);
                }
                _ => {
                    let shown = self.chat.shown.borrow();
                    let r = shown.get(spot.block).map(|m| word(&m.text, spot.at));
                    drop(shown);
                    self.chat.pick = r.map(|r| {
                        let of = |at| Spot {
                            block: spot.block,
                            at,
                        };
                        (of(r.start), of(r.end))
                    });
                }
            }
        }
        cx.notify();
    }

    /// The pointer dragged while selecting.
    pub(crate) fn drag_chat_text(&mut self, p: Point<f32>) {
        let at = gpui::point(px(p.x), px(p.y));
        if let (Some(head), Some(pick)) = (self.chat.spot_at(at, true), &mut self.chat.pick) {
            pick.1 = head;
        }
    }

    /// ⌘C: what's selected in the conversation, if anything is.
    pub(crate) fn copy_chat_pick(&mut self, cx: &mut Context<Self>) -> bool {
        match self.chat.picked_text() {
            Some(text) => {
                self.copy_text(text, "picked", cx);
                true
            }
            None => false,
        }
    }

    /// Text on the clipboard, and the button that put it there ticked for a moment.
    pub(crate) fn copy_text(&mut self, text: String, key: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = Some("Copied.".into());
        self.chat.copied = Some((key.to_string(), Instant::now()));
        let wait = cx.background_executor().timer(COPIED_FOR);
        cx.spawn(async move |ws, cx| {
            wait.await;
            ws.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
        cx.notify();
    }

    /// A little button that copies `text`, ticked once it has.
    pub(crate) fn copy_button(&self, key: String, text: String, cx: &mut Context<Self>) -> Div {
        let done = self
            .chat
            .copied
            .as_ref()
            .is_some_and(|(k, at)| *k == key && at.elapsed() < COPIED_FOR);
        let selector = format!("copy-{key}");
        div()
            .size(px(22.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(move || selector)
            .child(
                icon(if done { Icon::Check } else { Icon::Copy }, 12.).text_color(rgb(if done {
                    METER_LOW
                } else {
                    TEXT_3
                })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.copy_text(text.clone(), &key, cx);
                }),
            )
    }

    /// What the agent said.
    pub(crate) fn said(
        &self,
        paint: &mut Painter,
        text: &str,
        n: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut out = div()
            .flex()
            .flex_col()
            .gap(px(7.))
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(rgb(TEXT));
        for (k, block) in blocks(text).into_iter().enumerate() {
            let marker = |c: gpui::SharedString| {
                div()
                    .flex_none()
                    .min_w(px(14.))
                    .text_color(rgb(TEXT_3))
                    .child(c)
            };
            out = out.child(match block {
                Block::Heading(t) => div()
                    .pt(px(4.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.5))
                    .child(paint.line(&t, "\n"))
                    .into_any_element(),
                Block::Para(t) => div().child(paint.line(&t, "\n\n")).into_any_element(),
                Block::Bullet(t) => div()
                    .flex()
                    .gap(px(4.))
                    .child(marker("•".into()))
                    .child(div().flex_1().min_w(px(0.)).child(paint.line(&t, "\n")))
                    .into_any_element(),
                Block::Number(n, t) => div()
                    .flex()
                    .gap(px(4.))
                    .child(marker(n.into()))
                    .child(div().flex_1().min_w(px(0.)).child(paint.line(&t, "\n")))
                    .into_any_element(),
                Block::Quote(t) => div()
                    .pl(px(10.))
                    .border_l_2()
                    .border_color(rgb(LINE_STRONG))
                    .text_color(rgb(TEXT_2))
                    .child(paint.line(&t, "\n"))
                    .into_any_element(),
                Block::Code(code) => {
                    let group: gpui::SharedString = format!("code-{n}-{k}").into();
                    div()
                        .group(group.clone())
                        .relative()
                        .p(px(10.))
                        .rounded(px(8.))
                        .bg(rgb(WELL))
                        .border_1()
                        .border_color(rgb(LINE_SOFT))
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .line_height(px(17.))
                        .text_color(rgb(TEXT_2))
                        .child(paint.text(code.clone(), Vec::new(), "\n"))
                        .child(
                            div()
                                .absolute()
                                .top(px(4.))
                                .right(px(4.))
                                .invisible()
                                .group_hover(group, |d| d.visible())
                                .child(self.copy_button(format!("code-{n}-{k}"), code, cx)),
                        )
                        .into_any_element()
                }
                Block::Table(rows) => {
                    let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
                    div()
                        .flex()
                        .flex_col()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .overflow_hidden()
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .children(rows.iter().enumerate().map(|(r, row)| {
                            div()
                                .flex()
                                .when(r == 0, |d| {
                                    d.bg(rgb(CARD)).font_weight(FontWeight::SEMIBOLD)
                                })
                                .when(r > 0, |d| d.border_t_1().border_color(rgb(LINE_SOFT)))
                                .children((0..columns).map(|c| {
                                    let cell = row.get(c).map_or("", String::as_str);
                                    let gap = if c + 1 == columns { "\n" } else { "\t" };
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .px(px(8.))
                                        .py(px(5.))
                                        .child(paint.line(cell, gap))
                                }))
                        }))
                        .into_any_element()
                }
                Block::Rule => div().h(px(1.)).my(px(4.)).bg(rgb(LINE)).into_any_element(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_agent_writes_reads_as_it_was_meant() {
        let text = "## The plan\n\nFirst line\nsame paragraph.\n\n- one\n* two\n1. three\n\n> so\n\n---\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n| Part | Bars |\n|---|---|\n| Drums | 8 |";
        assert_eq!(
            blocks(text),
            [
                Block::Heading("The plan".into()),
                Block::Para("First line same paragraph.".into()),
                Block::Bullet("one".into()),
                Block::Bullet("two".into()),
                Block::Number("1.".into(), "three".into()),
                Block::Quote("so".into()),
                Block::Rule,
                Block::Code("let a = 1;\n\nlet b = 2;".into()),
                Block::Table(vec![
                    vec!["Part".to_string(), "Bars".to_string()],
                    vec!["Drums".to_string(), "8".to_string()],
                ]),
            ]
        );
    }

    #[test]
    fn a_code_block_still_being_written_shows_what_it_has() {
        assert_eq!(blocks("```\nhalf"), [Block::Code("half".into())]);
    }

    #[test]
    fn marks_come_off_and_are_kept_where_they_fell() {
        let (text, marks) = inline("Set **tempo** to `120`, *about*, see [the docs](http://x).");
        assert_eq!(text, "Set tempo to 120, about, see the docs.");
        assert_eq!(&text[marks[0].0.clone()], "tempo");
        assert_eq!(marks[0].1, Mark::Bold);
        assert_eq!(&text[marks[1].0.clone()], "120");
        assert_eq!(marks[1].1, Mark::Code);
        assert_eq!(marks[2].1, Mark::Italic);
    }

    #[test]
    fn a_star_that_opens_nothing_stays_a_star() {
        assert_eq!(inline("2 * 3 = 6").0, "2 * 3 = 6");
        assert_eq!(inline("a ** b").0, "a ** b");
    }

    #[test]
    fn a_double_click_takes_the_word_it_lands_on() {
        let t = "the kick's punch";
        assert_eq!(&t[word(t, 6)], "kick's");
        assert_eq!(&t[word(t, t.len())], "punch");
    }

    #[test]
    fn styles_never_overlap_however_a_selection_falls_across_them() {
        let (text, marks) = inline("one **two** three");
        // Across the end of the bold: a piece each side of it, in order.
        let cut = marks[0].0.start + 1..text.len() - 2;
        let _ = styled(&text, &marks, Some(cut));
    }
}
