//! A text field, for naming tracks, regions, markers and buses, for searching, and (wrapping
//! and growing a line at a time) for writing to an agent. It takes typing through the
//! platform's input handler, so accents and other input methods work; Enter confirms, Escape
//! cancels, and losing focus confirms too; in a field of several lines ⇧Enter starts a new one.
//! Adapted from gpui's input example.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, FontWeight, GlobalElementId,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle,
    Window, WrappedLine, actions, div, fill, point, prelude::*, px, relative, rgb, size,
};

use crate::theme::*;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Confirm,
        Cancel,
        Newline,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let ctx = Some("TextInput");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("cmd-left", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("cmd-right", End, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("enter", Confirm, ctx),
        KeyBinding::new("shift-enter", Newline, ctx),
        KeyBinding::new("escape", Cancel, ctx),
    ]);
}

/// What the field tells whoever opened it.
pub enum TextEvent {
    Confirm(String),
    Cancel,
}

pub struct TextInput {
    focus: FocusHandle,
    content: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    /// Confirmed or cancelled already, so losing focus afterwards does nothing.
    done: bool,
    /// A search field: it stays, so Enter and Escape tell every time; whoever holds it draws its
    /// box, and it says what to type while it's empty.
    search: Option<SharedString>,
    /// Losing focus confirms it, as naming a track does; a search, a message to an agent or a
    /// form's field wait for Enter.
    confirm_on_blur: bool,
    /// Its type: size, line height and weight; 12 on 16, regular, by default.
    type_: Option<(f32, f32, FontWeight)>,
    /// Wraps, growing to this many lines, then scrolls: 0 is one line that doesn't.
    rows: usize,
    /// The first line in view, of a field of several that's scrolled.
    scroll_row: usize,
    /// Its lines as last painted, for clicks to find their place in.
    last_lines: Vec<WrappedLine>,
}

impl EventEmitter<TextEvent> for TextInput {}

impl TextInput {
    /// A field holding `text`, all of it selected, so typing replaces it.
    pub fn new(text: &str, cx: &mut Context<Self>) -> Self {
        TextInput {
            focus: cx.focus_handle(),
            content: text.to_string().into(),
            selected: 0..text.len(),
            reversed: false,
            marked: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            done: false,
            search: None,
            confirm_on_blur: true,
            type_: None,
            rows: 0,
            scroll_row: 0,
            last_lines: Vec::new(),
        }
    }

    /// A field for writing at length: it wraps, grows to `rows` lines and then scrolls; Enter
    /// confirms (every time), ⇧Enter starts a new line, and `placeholder` shows while it's
    /// empty.
    pub fn writing(placeholder: &str, rows: usize, cx: &mut Context<Self>) -> Self {
        TextInput {
            search: Some(placeholder.to_string().into()),
            confirm_on_blur: false,
            rows: rows.max(2),
            ..TextInput::new("", cx)
        }
    }

    /// The same, in a larger (or bolder) type: a title edited in place.
    pub fn typed(mut self, size: f32, line: f32, weight: FontWeight) -> Self {
        self.type_ = Some((size, line, weight));
        self
    }

    /// An empty search field, saying what to type in it.
    pub fn search(placeholder: &str, cx: &mut Context<Self>) -> Self {
        TextInput {
            search: Some(placeholder.to_string().into()),
            confirm_on_blur: false,
            ..TextInput::new("", cx)
        }
    }

    /// A form's field: it stays, only Enter confirms it, and whoever holds it draws its box.
    pub fn field(text: &str, cx: &mut Context<Self>) -> Self {
        TextInput {
            search: Some("".into()),
            confirm_on_blur: false,
            ..TextInput::new(text, cx)
        }
    }

    /// A form's field that keeps itself: Enter or a click elsewhere confirms it, every time,
    /// and `placeholder` shows while it's empty.
    pub fn form(text: &str, placeholder: &str, cx: &mut Context<Self>) -> Self {
        TextInput {
            search: Some(placeholder.to_string().into()),
            confirm_on_blur: true,
            selected: text.len()..text.len(),
            ..TextInput::new(text, cx)
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = text.to_string().into();
        self.selected = text.len()..text.len();
        self.marked = None;
        cx.notify();
    }

    /// Confirms what's typed, once (a search field, every time).
    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        if !std::mem::replace(&mut self.done, true) || self.search.is_some() {
            cx.emit(TextEvent::Confirm(self.content.trim().to_string()));
        }
    }

    fn on_confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        self.confirm(cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        match self.rows {
            0 => self.confirm(cx),
            _ => self.replace_text_in_range(None, "\n", window, cx),
        }
    }

    fn on_cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::replace(&mut self.done, true) || self.search.is_some() {
            cx.emit(TextEvent::Cancel);
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        match self.selected.is_empty() {
            true => self.move_to(self.previous_boundary(self.cursor()), cx),
            false => self.move_to(self.selected.start, cx),
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        match self.selected.is_empty() {
            true => self.move_to(self.next_boundary(self.selected.end), cx),
            false => self.move_to(self.selected.end, cx),
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.select_to(self.previous_boundary(self.cursor()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.select_to(self.next_boundary(self.cursor()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        // Clicked, it takes the keyboard.
        window.focus(&self.focus, cx);
        self.selecting = true;
        let at = self.index_at(e.position);
        match e.modifiers.shift {
            true => self.select_to(at, cx),
            false => self.move_to(at, cx),
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.index_at(e.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = match self.rows {
                0 => text.replace('\n', " "),
                _ => text,
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.content[self.selected.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.content[self.selected.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        cx.notify()
    }

    fn cursor(&self) -> usize {
        match self.reversed {
            true => self.selected.start,
            false => self.selected.end,
        }
    }

    fn index_at(&self, position: Point<Pixels>) -> usize {
        if self.rows > 0 {
            return self.wrapped_index_at(position);
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if self.content.is_empty() || position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    /// Where a point falls in a field of several lines: the line it's on (as wrapped), then the
    /// character nearest it.
    fn wrapped_index_at(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        if self.content.is_empty() || self.last_lines.is_empty() {
            return 0;
        }
        let height = self.last_line_height();
        let mut y = position.y - bounds.top() + height * self.scroll_row as f32;
        let mut start = 0;
        for (i, line) in self.last_lines.iter().enumerate() {
            let rows = line.wrap_boundaries().len() + 1;
            let tall = height * rows as f32;
            if y < tall || i + 1 == self.last_lines.len() {
                let local = point(position.x - bounds.left(), y.max(px(0.)).min(tall - px(1.)));
                let at = match line.closest_index_for_position(local, height) {
                    Ok(i) | Err(i) => i,
                };
                return (start + at).min(self.content.len());
            }
            y -= tall;
            start += line.len() + 1;
        }
        self.content.len()
    }

    fn last_line_height(&self) -> Pixels {
        px(self.type_.map_or(16., |t| t.1))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        match self.reversed {
            true => self.selected.start = offset,
            false => self.selected.end = offset,
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = (0, 0);
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let (mut utf16, mut utf8) = (0, 0);
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }

    // ponytail: steps by character, not grapheme cluster; a combining accent takes two presses.
    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .chars()
            .next()
            .map_or(self.content.len(), |c| offset + c.len_utf8())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
        self.selected = range.start + text.len()..range.start + text.len();
        self.marked.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selected = new_selected
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.end)
            .unwrap_or_else(|| range.start + text.len()..range.start + text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range);
        Some(Bounds::from_corners(
            point(
                bounds.left() + layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let local = self.last_bounds?.localize(&p)?;
        let layout = self.last_layout.as_ref()?;
        let utf8 = layout.index_for_x(p.x - local.x)?;
        Some(self.offset_to_utf16(utf8))
    }
}

/// What the field shows (its text, or its placeholder faintly) as runs of type.
fn shown(input: &TextInput, window: &Window) -> (SharedString, Vec<TextRun>, Pixels) {
    let style = window.text_style();
    let (content, color) = match &input.search {
        Some(hint) if input.content.is_empty() => (hint.clone(), rgb(TEXT_4).into()),
        _ => (input.content.clone(), style.color),
    };
    let run = TextRun {
        len: content.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = match input.marked.as_ref().filter(|_| !input.content.is_empty()) {
        Some(m) => [
            TextRun {
                len: m.start,
                ..run.clone()
            },
            TextRun {
                len: m.end - m.start,
                underline: Some(UnderlineStyle {
                    color: Some(run.color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                ..run.clone()
            },
            TextRun {
                len: content.len() - m.end,
                ..run
            },
        ]
        .into_iter()
        .filter(|r| r.len > 0)
        .collect(),
        None => vec![run],
    };
    let font_size = style.font_size.to_pixels(window.rem_size());
    (content, runs, font_size)
}

/// The text, its selection and its cursor, painted.
struct TextElement {
    input: Entity<TextInput>,
}

struct Prepaint {
    line: Option<ShapedLine>,
    /// A field of several lines: its lines, each where it starts.
    lines: Vec<(WrappedLine, Point<Pixels>)>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let input = self.input.read(cx);
        if input.rows == 0 {
            style.size.height = window.line_height().into();
            return (window.request_layout(style, [], cx), ());
        }
        // As many lines as the text takes at its width, up to `rows`.
        let (content, runs, font_size) = shown(input, window);
        let (line_height, rows) = (window.line_height(), input.rows);
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.unwrap_or(match available.width {
                gpui::AvailableSpace::Definite(w) => w,
                _ => px(400.),
            });
            let taken = window
                .text_system()
                .shape_text(content.clone(), font_size, &runs, Some(width), None)
                .map_or(1, |lines| {
                    lines
                        .iter()
                        .map(|l| l.wrap_boundaries().len() + 1)
                        .sum::<usize>()
                });
            size(width, line_height * taken.clamp(1, rows) as f32)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let input = self.input.read(cx);
        let (selected, cursor) = (input.selected.clone(), input.cursor());
        let (content, runs, font_size) = shown(input, window);
        if input.rows > 0 {
            return self.prepaint_wrapped(bounds, content, &runs, font_size, window, cx);
        }
        let line = window
            .text_system()
            .shape_line(content, font_size, &runs, None);
        let (selection, cursor) = if selected.is_empty() {
            let x = bounds.left() + line.x_for_index(cursor);
            let caret = Bounds::new(point(x, bounds.top()), size(px(1.5), bounds.size.height));
            (None, Some(fill(caret, rgb(ACCENT))))
        } else {
            let area = Bounds::from_corners(
                point(
                    bounds.left() + line.x_for_index(selected.start),
                    bounds.top(),
                ),
                point(
                    bounds.left() + line.x_for_index(selected.end),
                    bounds.bottom(),
                ),
            );
            (Some(fill(area, alpha(ACCENT, 0.4))), None)
        };
        Prepaint {
            line: Some(line),
            lines: Vec::new(),
            cursor,
            selection: selection.into_iter().collect(),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection)
        }
        let height = window.line_height();
        if prepaint.line.is_none() {
            let lines = std::mem::take(&mut prepaint.lines);
            for (line, origin) in &lines {
                let _ = line.paint(*origin, height, gpui::TextAlign::Left, None, window, cx);
            }
            if focus.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
            self.input.update(cx, |input, _| {
                input.last_lines = lines.into_iter().map(|(l, _)| l).collect();
                input.last_bounds = Some(bounds);
            });
            return;
        }
        let line = prepaint.line.take().unwrap();
        let _ = line.paint(
            bounds.origin,
            height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
        if focus.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl TextElement {
    /// A field of several lines: the text wrapped at its width, scrolled to keep the cursor in
    /// view, and the cursor or the selection (a band a line) where they fall.
    fn prepaint_wrapped(
        &mut self,
        bounds: Bounds<Pixels>,
        content: SharedString,
        runs: &[TextRun],
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let height = window.line_height();
        let lines: Vec<WrappedLine> = window
            .text_system()
            .shape_text(content, font_size, runs, Some(bounds.size.width), None)
            .map(|l| l.into_iter().collect())
            .unwrap_or_default();
        let input = self.input.read(cx);
        let (selected, cursor, empty) = (
            input.selected.clone(),
            input.cursor(),
            input.content.is_empty(),
        );
        // Where a byte of the text is, in rows and x from the field's top left (unscrolled).
        let place = |index: usize| -> (usize, Pixels) {
            let (mut start, mut row) = (0, 0);
            for (i, line) in lines.iter().enumerate() {
                let end = start + line.len();
                if index <= end || i + 1 == lines.len() {
                    let p = line
                        .position_for_index(index.saturating_sub(start).min(line.len()), height)
                        .unwrap_or_default();
                    return (row + (p.y / height).round() as usize, p.x);
                }
                start = end + 1;
                row += line.wrap_boundaries().len() + 1;
            }
            (0, px(0.))
        };
        let total: usize = lines.iter().map(|l| l.wrap_boundaries().len() + 1).sum();
        let visible = ((bounds.size.height / height).floor() as usize).max(1);
        let (cursor_row, cursor_x) = if empty { (0, px(0.)) } else { place(cursor) };
        let scroll = self.input.update(cx, |input, _| {
            if cursor_row < input.scroll_row {
                input.scroll_row = cursor_row;
            } else if cursor_row >= input.scroll_row + visible {
                input.scroll_row = cursor_row + 1 - visible;
            }
            input.scroll_row = input.scroll_row.min(total.saturating_sub(visible));
            input.scroll_row
        });
        let top = bounds.top() - height * scroll as f32;
        let at = |row: usize, x: Pixels| point(bounds.left() + x, top + height * row as f32);
        let (cursor, selection) = match selected.is_empty() {
            true => {
                let caret = Bounds::new(at(cursor_row, cursor_x), size(px(1.5), height));
                (Some(fill(caret, rgb(ACCENT))), Vec::new())
            }
            false => {
                let (a, ax) = place(selected.start);
                let (b, bx) = place(selected.end);
                let band = |row: usize, from: Pixels, to: Pixels| {
                    fill(
                        Bounds::from_corners(at(row, from), at(row, to) + point(px(0.), height)),
                        alpha(ACCENT, 0.4),
                    )
                };
                let bands = match a == b {
                    true => vec![band(a, ax, bx)],
                    false => (a..=b)
                        .map(|r| match r {
                            r if r == a => band(r, ax, bounds.size.width),
                            r if r == b => band(r, px(0.), bx),
                            r => band(r, px(0.), bounds.size.width),
                        })
                        .collect(),
                };
                (None, bands)
            }
        };
        let mut placed = Vec::new();
        let mut row = 0;
        for line in lines {
            let rows = line.wrap_boundaries().len() + 1;
            placed.push((line, at(row, px(0.))));
            row += rows;
        }
        Prepaint {
            line: None,
            lines: placed,
            cursor,
            selection,
        }
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("text-input")
            .flex()
            .w_full()
            .when(self.rows == 0, |d| d.items_center().h(px(22.)))
            .when(self.rows > 0, |d| d.items_start())
            .when(self.search.is_none(), |d| {
                d.px(px(6.))
                    .rounded(px(5.))
                    .bg(rgb(FIELD))
                    .border_1()
                    .border_color(rgb(ACCENT))
            })
            .overflow_hidden()
            .key_context("TextInput")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.type_.map_or(12., |t| t.0)))
            .line_height(px(self.type_.map_or(16., |t| t.1)))
            .when_some(self.type_, |d, t| d.font_weight(t.2))
            // A one-line field in a larger type is that type's line tall; one of several grows.
            .when_some(self.type_.filter(|_| self.rows == 0), |d, t| d.h(px(t.1)))
            .text_color(rgb(TEXT))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::on_cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            // A click anywhere else keeps what's typed, whatever it lands on.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                if this.confirm_on_blur && this.focus.is_focused(window) {
                    this.confirm(cx)
                }
            }))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
