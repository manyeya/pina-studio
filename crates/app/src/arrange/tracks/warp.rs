//! Warp markers on a selected audio region.

use super::*;

/// Where an audio region's file plays: its frames (at the project's rate) in the stretched,
/// warped audio, and those in ticks from where the region starts.
pub struct Warping {
    pub map: Map,
    pub hits: Arc<Vec<usize>>,
    pub warps: Vec<[u64; 2]>,
    /// The stretched frame the region starts at, and frames a tick.
    start: f64,
    per_tick: f64,
}

impl Warping {
    /// Ticks from the region's start where file frame `f` plays.
    pub fn tick(&self, f: usize) -> f64 {
        (self.map.at(f) as f64 - self.start) / self.per_tick
    }
}

impl Workspace {
    /// An audio region's warp geometry, once its file is loaded.
    pub fn warping(&self, at: At) -> Option<Warping> {
        let p = &self.session.project;
        let Content::Audio {
            file,
            offset,
            stretch,
            warps,
            ..
        } = &p.region(at).content
        else {
            return None;
        };
        let (hits, n) = self.player.hits(file, p.sample_rate as f64)?;
        let len = (n as f64 * stretch).round() as usize;
        let pins: Vec<(usize, usize)> = warps
            .iter()
            .map(|w| (w[0] as usize, w[1] as usize))
            .collect();
        let map = Map::new(n, len, &pins);
        let start = match pins.is_empty() {
            true => *offset as f64 * stretch,
            false => map.at(*offset as usize) as f64,
        };
        Some(Warping {
            map,
            hits,
            warps: warps.clone(),
            start,
            per_tick: p.sample_rate as f64 * 60.0 / (p.tempo * PPQ as f64),
        })
    }

    /// A hit (or a warp marker) of an audio region dragged to `x`: a warp marker there, on the
    /// sixteenth, the hits either side pinned where they play (flex markers), and
    /// kept between its neighbours so the audio stays in order.
    pub fn drag_warp(&mut self, at: At, from: u64, x: f32) {
        let Some(w) = self.warping(at) else {
            return;
        };
        let r = self.session.project.region(at);
        let (start, length) = (r.start as f64, r.length as f64);
        let g = self.grid().min(PPQ / 4) as f64;
        let tick = (self.tick_at(x) - start).clamp(0.0, length);
        let to = (w.start + (tick / g).round() * g * w.per_tick).round() as u64;
        let i = w.hits.iter().position(|&h| h as u64 == from);
        let pins: Vec<[u64; 2]> = [i.and_then(|i| i.checked_sub(1)), i.map(|i| i + 1)]
            .into_iter()
            .flatten()
            .filter_map(|j| w.hits.get(j))
            .map(|&h| [h as u64, w.map.at(h) as u64])
            .collect();
        let Content::Audio { warps, .. } =
            &mut self.session.project.tracks[at.0].regions[at.1].content
        else {
            return;
        };
        for p in pins {
            if !warps.iter().any(|m| m[0] == p[0]) {
                warps.push(p);
            }
        }
        warps.retain(|m| m[0] != from);
        let below = warps.iter().filter(|m| m[0] < from).map(|m| m[1] + 1).max();
        let above = warps
            .iter()
            .filter(|m| m[0] > from)
            .map(|m| m[1].saturating_sub(1))
            .min();
        let to = to.max(below.unwrap_or(0)).min(above.unwrap_or(u64::MAX));
        warps.push([from, to]);
        warps.sort();
    }

    /// The selected audio region's hits, as handles in its title strip to drag, and its warp
    /// markers, as lines down it (a double-click takes one out).
    pub(crate) fn warp_handles(&self, at: At, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(w) = self.warping(at) else {
            return Vec::new();
        };
        let r = self.session.project.region(at);
        let ppt = self.px_per_tick();
        let width = r.length as f64;
        let mut out = Vec::new();
        let marked: Vec<u64> = w.warps.iter().map(|m| m[0]).collect();
        let handle = |from: u64, marker: bool, x: f32, cx: &mut Context<Self>| {
            div()
                .absolute()
                .left(px(x - 5.))
                .top_0()
                .w(px(11.))
                .h(px(17.))
                .flex()
                .justify_center()
                .cursor_ew_resize()
                .debug_selector(move || format!("warp-{}-{}-{from}", at.0, at.1))
                .child(match marker {
                    true => div()
                        .mt(px(3.))
                        .size(px(9.))
                        .rounded(px(2.))
                        .bg(rgb(ACCENT)),
                    false => div()
                        .mt(px(5.))
                        .w(px(1.5))
                        .h(px(9.))
                        .bg(alpha(0xffffff, 0.6)),
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if marker && e.click_count == 2 {
                            ws.edit(cx, |ws| {
                                let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                                if let Content::Audio { warps, .. } = &mut r.content {
                                    warps.retain(|m| m[0] != from);
                                }
                            });
                            return;
                        }
                        ws.begin_drag(Drag::Warp { at, from }, cx);
                    }),
                )
                .into_any_element()
        };
        for m in &w.warps {
            let tick = w.tick(m[0] as usize);
            if (0.0..=width).contains(&tick) {
                let x = (tick * ppt as f64) as f32;
                out.push(
                    div()
                        .absolute()
                        .left(px(x - 1.))
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(alpha(ACCENT, 0.8))
                        .into_any_element(),
                );
                out.push(handle(m[0], true, x, cx));
            }
        }
        for &h in w.hits.iter().filter(|&&h| !marked.contains(&(h as u64))) {
            let tick = w.tick(h);
            if (0.0..=width).contains(&tick) {
                out.push(handle(h as u64, false, (tick * ppt as f64) as f32, cx));
            }
        }
        out
    }
}
