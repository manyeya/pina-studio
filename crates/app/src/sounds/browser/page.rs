//! The sample browser as a page of its own, and what shows while a sample or file is dragged.

use super::*;

impl Workspace {
    /// The browser's own page, in the tracks' place as the mixer is: what to look for, what's
    /// found (a list, the map or a kit), and the selected sample in detail.
    pub fn samples_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Samples"),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(self.samples_detail()),
            )
            .child(div().flex_1())
            .child(self.samples_controls(cx))
            .child(
                button("Compact", false)
                    .child(icon(Icon::PanelRight, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "samples-compact".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.toggle_samples(cx)),
                    ),
            );
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(self.samples_body(cx))
    }

    pub fn samples_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let middle = match self.browser.view {
            Results::List => self.results_list(cx).into_any_element(),
            Results::Map => self.results_map(cx).into_any_element(),
            Results::Kit => self.kit_view(cx).into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(self.filters(cx))
            .child(div().flex_1().min_w(px(0.)).flex().flex_col().child(middle))
            .child(self.details(cx))
    }

    /// The sample being dragged, by the pointer, saying where it would go.
    pub fn sample_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Sample {
            index,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let s = self.browser.sample(index)?;
        let color = category_color(s.analysis.category);
        let target = self.drop_target(at);
        let (hint, ok) = match target {
            Some(Drop::Audio(t, _)) => (
                format!("Audio on {}", self.session.project.tracks[t].name),
                true,
            ),
            Some(Drop::Zone(t, _)) => (
                format!("A pad on {}", self.session.project.tracks[t].name),
                true,
            ),
            Some(Drop::New(_)) => ("Audio on a new track".to_string(), true),
            Some(Drop::Under(t, _)) => (
                format!(
                    "Audio on a new track under {}",
                    self.session.project.tracks[t].name
                ),
                true,
            ),
            None => ("Drop it on a track".to_string(), true),
        };
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .w(px(230.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1c1c22))
                .border_1()
                .border_color(alpha(color, 0.7))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().w(px(56.)).h(px(20.)).flex_none().child(wave(
                            s.analysis.waveform.clone(),
                            color,
                            0.9,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(if ok { TEXT_3 } else { RECORD }))
                        .child(hint),
                ),
        )
    }

    /// In the lanes, while a sample is dragged over them: where it would land.
    pub fn sample_drop_view(&self) -> Option<Div> {
        let Some(Drag::Sample {
            index,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let s = self.browser.sample(index)?;
        let ppt = self.px_per_tick();
        let color = category_color(s.analysis.category);
        let lane = |row: usize| {
            div()
                .absolute()
                .left_0()
                .top(px(self.row_top(row) + 2.))
                .w_full()
                .h(px(self.lane_h(row) - 4.))
                .rounded(px(6.))
        };
        let p = &self.session.project;
        // An audio region's outline, as long as it will be in the song.
        let region = |row: usize, tick: u64| {
            let length = loop_ticks(&s.analysis, p.tempo).unwrap_or_else(|| {
                p.ticks((s.analysis.seconds as f64 * p.sample_rate as f64) as u64)
                    .max(1)
            });
            div()
                .absolute()
                .left(px(tick as f32 * ppt - self.scroll_x))
                .top(px(self.row_top(row) + 4.))
                .w(px((length as f32 * ppt).max(6.)))
                .h(px(self.lane_h(row) - 8.))
                .rounded(px(6.))
                .bg(alpha(color, 0.18))
                .border_1()
                .border_color(alpha(color, 0.9))
        };
        Some(match self.drop_target(at)? {
            Drop::Audio(t, tick) => region(t, tick),
            Drop::New(tick) => region(p.tracks.len(), tick),
            // A new track goes in under it: the region shows on the line between.
            Drop::Under(t, tick) => region(t, tick).top(px(self.row_top(t + 1) - 6.)).h(px(12.)),
            Drop::Zone(t, _) => lane(t)
                .bg(alpha(color, 0.1))
                .border_1()
                .border_color(alpha(color, 0.8)),
        })
    }

    /// In the lanes, while files are dragged in from the Finder: where they'd land.
    pub fn file_drop_view(&self, cx: &App) -> Option<Div> {
        let at = self.file_drag.as_ref().filter(|_| cx.has_active_drag())?.0;
        let lane = |row: usize| {
            div()
                .absolute()
                .left_0()
                .top(px(self.row_top(row) + 2.))
                .w_full()
                .h(px(self.lane_h(row) - 4.))
                .rounded(px(6.))
                .bg(alpha(ACCENT, 0.08))
                .border_1()
                .border_color(alpha(ACCENT, 0.7))
        };
        let tick = |t: u64| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(t as f32 * self.px_per_tick() - self.scroll_x))
                .w(px(2.))
                .bg(rgb(ACCENT))
        };
        Some(match self.drop_target(at)? {
            Drop::Audio(t, at) => lane(t).child(tick(at)),
            Drop::New(at) => lane(self.session.project.tracks.len()).child(tick(at)),
            Drop::Zone(t, at) | Drop::Under(t, at) => lane(t + 1)
                .h(px(4.))
                .top(px(self.row_top(t + 1) - 2.))
                .child(tick(at)),
        })
    }
}
