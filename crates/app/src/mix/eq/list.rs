//! The instance list: every channel's EQ in the display's place, each with what
//! it's playing (its spectrum, and where it collides with the reference's) and its curve; zoomed
//! in (or under the pointer, with auto-zoom), its bands to drag and a click to make one, as on
//! the display. Filter by name, pin some and show only those; bypass one, make it the
//! reference, compare with it, match to it, or open it.

use std::{cell::RefCell, rc::Rc};

use gpui::{
    Bounds, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    ScrollWheelEvent, Stateful, Subscription, canvas, div, fill, linear_color_stop,
    linear_gradient, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    eq::{LANE_LEN, eq_response, smooth_third, spectrum_db},
    project::{Eq, EqField},
};

use super::{EqOp, Geo, band_at, band_color, dot_db};
use crate::{
    mix::strip::Channel,
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{Drag, Name, Workspace},
};

/// The frequencies a row shows, 20 Hz to 20 kHz.
const LOW: f64 = 20.0;
const HIGH: f64 = 20_000.0;
const COLUMNS: usize = 160;
/// ± dB a row's curve shows.
const RANGE: f32 = 18.0;
/// A row's spectrum: its floor (dB).
const FLOOR: f32 = -84.0;
/// A row's height at each zoom: its spectrum, with its curve, to edit.
const HEIGHTS: [f32; 3] = [46.0, 92.0, 176.0];
const NAME_W: f32 = 196.0;

/// The instance list's settings, and what its rows have heard.
#[derive(Default)]
pub struct Listing {
    /// How big its rows are: 0 their spectrum, 1 with their curves, 2 their bands to edit.
    pub zoom: u8,
    /// The row under the pointer grows to edit.
    pub auto_zoom: bool,
    /// A row clicked: shown to edit, whatever the zoom.
    pub expanded: Option<Channel>,
    pub hover: Option<Channel>,
    /// Channels pinned (a track's or aux's id; none, the master), and whether only those show.
    pub pins: Vec<Option<u64>>,
    pub pinned_only: bool,
    /// What collisions are against: this channel (none, the one the window's on).
    pub reference: Option<Channel>,
    /// Each channel's spectrum lately, at `COLUMNS` frequencies.
    spectra: Vec<(Channel, Vec<f32>)>,
    buf: Option<Box<[f32; LANE_LEN]>>,
    /// The name filter, and its watch.
    pub filter: Option<Entity<TextInput>>,
    _typed: Option<[Subscription; 2]>,
    /// Each row's display, where it was last drawn.
    boxes: Boxes,
}

/// Where each row's display was drawn.
type Boxes = Rc<RefCell<Vec<(Channel, Bounds<Pixels>)>>>;

/// What a row's button does.
type Act = Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>;

/// One row: a channel with an EQ.
struct Row {
    ch: Channel,
    name: String,
    color: u32,
    id: Option<u64>,
    eq: Eq,
}

/// Where a band's dot is on a row's display.
fn dot(g: &Geo, b: &pina_engine::project::EqBand, scale: f32) -> (f32, f32) {
    (
        g.x(b.freq as f64),
        g.y(dot_db(b, scale).clamp(-RANGE, RANGE)),
    )
}

fn columns() -> Vec<f64> {
    (0..COLUMNS)
        .map(|i| LOW * (HIGH / LOW).powf(i as f64 / (COLUMNS - 1) as f64))
        .collect()
}

/// A row's display, `w` by `h`: frequencies across, ±18 dB up and down.
fn geo(w: f32, h: f32) -> Geo {
    Geo {
        w: w.max(10.0),
        h: h.max(10.0),
        lo: LOW,
        hi: HIGH,
        range: RANGE,
    }
}

impl Workspace {
    /// Every channel with an EQ, in the mixer's order.
    fn list_rows(&self) -> Vec<Row> {
        let p = &self.session.project;
        let tracks = p.tracks.iter().enumerate().filter_map(|(i, t)| {
            Some(Row {
                ch: Channel::Track(i),
                name: t.name.clone(),
                color: self.color(i),
                id: Some(t.id),
                eq: t.eq?,
            })
        });
        let auxes = p.auxes.iter().enumerate().filter_map(|(i, a)| {
            Some(Row {
                ch: Channel::Aux(i),
                name: a.name.clone(),
                color: BUS,
                id: Some(a.id),
                eq: a.eq?,
            })
        });
        let master = p.master_eq.map(|eq| Row {
            ch: Channel::Master,
            name: "Stereo Out".into(),
            color: MASTER,
            id: None,
            eq,
        });
        tracks.chain(auxes).chain(master).collect()
    }

    /// The list opened: its filter made.
    pub(crate) fn open_list(&mut self, cx: &mut Context<Self>) {
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        if v.listing.filter.is_none() {
            let input = cx.new(|cx| TextInput::search("Filter tracks", cx));
            let typed = cx.observe(&input, |_, _, cx| cx.notify());
            let done = cx.subscribe(&input, |_, input, e: &TextEvent, cx| {
                if let TextEvent::Cancel = e {
                    input.update(cx, |i, cx| i.set_text("", cx));
                }
                cx.notify();
            });
            v.listing.filter = Some(input);
            v.listing._typed = Some([typed, done]);
        }
    }

    /// Each frame the list's open: every row's channel heard, its spectrum moved on.
    pub(super) fn list_frame(&mut self) {
        let open = self.eq_view.as_ref().is_some_and(|v| v.list);
        self.player.watch_lanes(open);
        if !open {
            return;
        }
        let rate = self.session.project.sample_rate as f64;
        let rows: Vec<(Channel, Option<u64>)> =
            self.list_rows().iter().map(|r| (r.ch, r.id)).collect();
        let freqs = columns();
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let tilt = view.analysis.tilt;
        let listing = &mut view.listing;
        let buf = listing.buf.get_or_insert_with(|| Box::new([0.0; LANE_LEN]));
        listing
            .spectra
            .retain(|(ch, _)| rows.iter().any(|r| r.0 == *ch));
        for (ch, id) in rows {
            let written = self.player.lane_sound(id, buf);
            let now = match written > 0 && buf.iter().any(|x| x.abs() > 1e-5) {
                true => smooth_third(&spectrum_db(&buf[..], rate, &freqs), &freqs),
                false => vec![FLOOR - 20.0; COLUMNS],
            };
            let at = match listing.spectra.iter().position(|(c, _)| *c == ch) {
                Some(i) => i,
                None => {
                    listing.spectra.push((ch, vec![FLOOR; COLUMNS]));
                    listing.spectra.len() - 1
                }
            };
            let smooth = &mut listing.spectra[at].1;
            for (i, (s, v)) in smooth.iter_mut().zip(now).enumerate() {
                let v = (v + tilt * (freqs[i] / 1_000.0).log2() as f32).max(FLOOR);
                // Up quickly, down slowly.
                *s = if v > *s {
                    *s + (v - *s) * 0.6
                } else {
                    (*s - 1.2).max(v)
                };
            }
        }
    }

    /// A row's channel made the one the window's on (the list stays), a band of it picked.
    fn activate(&mut self, ch: Channel, band: Option<u16>, cx: &mut Context<Self>) {
        self.open_eq(ch, cx);
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = band.into_iter().collect();
        }
    }

    /// A band of a row pressed: picked, and dragged.
    fn press_list_band(&mut self, ch: Channel, id: u16, cx: &mut Context<Self>) {
        self.activate(ch, Some(id), cx);
        let Some(frame) = self.list_box(ch) else {
            return;
        };
        self.begin_drag(Drag::EqListBand { id, frame }, cx);
    }

    /// The band of a row's whose dot is at `at` (within a few pixels), if any.
    fn list_band_at(&self, ch: Channel, at: gpui::Point<Pixels>) -> Option<u16> {
        let frame = self.list_box(ch)?;
        let eq = self.strip_dsp(ch).0?;
        let g = geo(f32::from(frame.size.width), f32::from(frame.size.height));
        let (x, y) = (
            f32::from(at.x - frame.origin.x),
            f32::from(at.y - frame.origin.y),
        );
        eq.bands
            .iter()
            .map(|b| {
                let (bx, by) = dot(&g, b, eq.scale);
                (b.id, (bx - x).hypot(by - y))
            })
            .filter(|&(_, d)| d < 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// A row's display pressed where there's no band: one made there, and dragged.
    fn press_list(&mut self, ch: Channel, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(frame) = self.list_box(ch) else {
            return;
        };
        self.activate(ch, None, cx);
        let g = geo(f32::from(frame.size.width), f32::from(frame.size.height));
        let (x, y) = (
            f32::from(e.position.x - frame.origin.x),
            f32::from(e.position.y - frame.origin.y),
        );
        if let Some(id) = self.make_band(band_at(&g, x, y, false), e.modifiers.alt, cx) {
            self.begin_drag(Drag::EqListBand { id, frame }, cx);
        }
    }

    fn list_box(&self, ch: Channel) -> Option<Bounds<Pixels>> {
        let v = self.eq_view.as_ref()?;
        let boxes = v.listing.boxes.borrow();
        boxes.iter().find(|(c, _)| *c == ch).map(|&(_, b)| b)
    }

    /// A row's band dragged: to where the pointer is on its display.
    pub fn drag_list_band(&mut self, id: u16, frame: Bounds<Pixels>, p: gpui::Point<f32>) {
        let g = geo(f32::from(frame.size.width), f32::from(frame.size.height));
        let (x, y) = (
            p.x - f32::from(frame.origin.x),
            p.y - f32::from(frame.origin.y),
        );
        let Some(eq) = self.eq_mut() else { return };
        let Some(i) = eq.bands.find(id) else { return };
        let b = &mut eq.bands[i];
        b.freq = (g.freq(x.clamp(0.0, g.w)) as f32)
            .clamp(10.0, 30_000.0)
            .round();
        if b.shape.has_gain() {
            b.gain_db = ((g.db(y) * 2.0).round() / 2.0).clamp(-30.0, 30.0);
        }
        self.eq_moved(&[id], &[EqField::Freq, EqField::Gain]);
    }

    /// The instance list, in the display's place, `w` by `h`.
    pub(super) fn eq_list(&self, w: f32, h: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let rate = self.session.project.sample_rate as f64;
        let view = self.eq_view.as_ref().expect("the window's open");
        let (now, ext) = (view.ch, view.ext);
        let l = &view.listing;
        let filter = l
            .filter
            .as_ref()
            .map(|f| f.read(cx).text().trim().to_lowercase())
            .unwrap_or_default();
        let reference = l.reference.unwrap_or(now);
        let spectrum_of = |ch: Channel| {
            l.spectra
                .iter()
                .find(|(c, _)| *c == ch)
                .map(|(_, s)| s.clone())
        };
        let ref_spectrum = spectrum_of(reference);
        l.boxes.borrow_mut().clear();
        let freqs = columns();
        let rows: Vec<Row> = self
            .list_rows()
            .into_iter()
            .filter(|r| filter.is_empty() || r.name.to_lowercase().contains(&filter))
            .filter(|r| !l.pinned_only || l.pins.contains(&r.id))
            .collect();
        let count = rows.len();
        let items = rows
            .into_iter()
            .enumerate()
            .map(|(k, row)| {
                let Row {
                    ch,
                    name,
                    color,
                    id,
                    eq,
                } = row;
                let editing =
                    l.zoom == 2 || l.expanded == Some(ch) || (l.auto_zoom && l.hover == Some(ch));
                let zoom = if editing { 2 } else { l.zoom as usize };
                let row_h = HEIGHTS[zoom];
                let spectrum = spectrum_of(ch);
                let against = (ch != reference).then(|| ref_spectrum.clone()).flatten();
                let curve: Vec<f32> = match zoom {
                    0 => Vec::new(),
                    _ => freqs
                        .iter()
                        .map(|&f| eq_response(&eq, f, rate) as f32)
                        .collect(),
                };
                let pinned = l.pins.contains(&id);
                let compared = id.is_some() && id == ext;
                let boxes = l.boxes.clone();
                let bypass = eq.bypass;
                let picked: Vec<u16> = match ch == now {
                    true => view.picked.clone(),
                    false => Vec::new(),
                };
                let name_field = match ch {
                    Channel::Track(i) => self.name_field(Name::Track(i)),
                    Channel::Aux(i) => self.name_field(Name::Aux(i)),
                    Channel::Master => None,
                };
                let chip = |label: &'static str,
                            on: bool,
                            on_color: u32,
                            sel: String,
                            cx: &mut Context<Self>,
                            act: Act| {
                    div()
                        .h(px(20.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .text_size(px(10.))
                        .whitespace_nowrap()
                        .when(on, |d| d.bg(alpha(on_color, 0.2)).text_color(rgb(on_color)))
                        .when(!on, |d| {
                            d.bg(rgb(FIELD))
                                .text_color(rgb(TEXT_3))
                                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                        })
                        .debug_selector(move || sel.clone())
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                act(ws, cx);
                                cx.notify();
                            }),
                        )
                };
                // Its name (double-click to rename), what it is, and what can be done to it.
                let info = div()
                    .w(px(NAME_W))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(div().size(px(8.)).rounded_full().bg(rgb(color)))
                            .child(match name_field {
                                Some(f) => div().flex_1().child(f),
                                None => div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .debug_selector(move || format!("eq-instance-name-{k}"))
                                    .child(name)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                            let what = match ch {
                                                Channel::Track(i) => Some(Name::Track(i)),
                                                Channel::Aux(i) => Some(Name::Aux(i)),
                                                Channel::Master => None,
                                            };
                                            if e.click_count == 2
                                                && let Some(what) = what
                                            {
                                                cx.stop_propagation();
                                                ws.start_naming(what, window, cx);
                                            }
                                        }),
                                    ),
                            })
                            .when(ch == reference, |d| {
                                d.child(
                                    div()
                                        .px(px(5.))
                                        .rounded(px(4.))
                                        .bg(alpha(RECORD, 0.2))
                                        .text_size(px(9.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(RECORD))
                                        .child("REF"),
                                )
                            }),
                    )
                    .child(div().text_size(px(10.5)).text_color(rgb(TEXT_3)).child(
                        match (eq.bypass, eq.bands.len()) {
                            (true, _) => "bypassed".to_string(),
                            (_, 1) => "1 band".into(),
                            (_, n) => format!("{n} bands"),
                        },
                    ))
                    .when(zoom > 0 || l.hover == Some(ch), |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(3.))
                                .child(chip(
                                    "Pin",
                                    pinned,
                                    ACCENT_TEXT,
                                    format!("eq-instance-pin-{k}"),
                                    cx,
                                    Box::new(move |ws, _| {
                                        if let Some(v) = ws.eq_view.as_mut() {
                                            let pins = &mut v.listing.pins;
                                            match pins.iter().position(|p| *p == id) {
                                                Some(i) => {
                                                    pins.remove(i);
                                                }
                                                None => pins.push(id),
                                            }
                                        }
                                    }),
                                ))
                                .child(chip(
                                    "Bypass",
                                    bypass,
                                    AMBER,
                                    format!("eq-instance-bypass-{k}"),
                                    cx,
                                    Box::new(move |ws, cx| {
                                        ws.edit(cx, |ws| {
                                            if let Some(e) = ws.strip_dsp_mut(ch).0.as_mut() {
                                                e.bypass = !e.bypass;
                                            }
                                        })
                                    }),
                                ))
                                .when(ch != reference, |d| {
                                    d.child(chip(
                                        "Ref",
                                        false,
                                        RECORD,
                                        format!("eq-instance-ref-{k}"),
                                        cx,
                                        Box::new(move |ws, _| {
                                            if let Some(v) = ws.eq_view.as_mut() {
                                                v.listing.reference = (ch != v.ch).then_some(ch);
                                            }
                                        }),
                                    ))
                                })
                                .when(ch != now && id.is_some(), |d| {
                                    let id = id.unwrap_or_default();
                                    d.child(chip(
                                        "Compare",
                                        compared,
                                        RECORD,
                                        format!("eq-compare-{k}"),
                                        cx,
                                        Box::new(move |ws, cx| {
                                            let to = if compared { None } else { Some(id) };
                                            ws.eq_op(EqOp::Ext(to), cx)
                                        }),
                                    ))
                                    .child(chip(
                                        "Match",
                                        false,
                                        RECORD,
                                        format!("eq-match-{k}"),
                                        cx,
                                        Box::new(move |ws, cx| ws.eq_op(EqOp::Match(Some(id)), cx)),
                                    ))
                                })
                                .child(chip(
                                    "Open",
                                    false,
                                    ACCENT_TEXT,
                                    format!("eq-instance-open-{k}"),
                                    cx,
                                    Box::new(move |ws, cx| {
                                        ws.open_eq(ch, cx);
                                        if let Some(v) = ws.eq_view.as_mut() {
                                            v.list = false;
                                        }
                                    }),
                                )),
                        )
                    });
                // What it's playing, where it collides with the reference, and its curve.
                let picture = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        boxes.borrow_mut().push((ch, b));
                        let (cw, chh) = (f32::from(b.size.width), f32::from(b.size.height));
                        let o = b.origin;
                        let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
                        let n = COLUMNS;
                        let xs: Vec<f32> = (0..n).map(|i| i as f32 / (n - 1) as f32 * cw).collect();
                        let level = move |db: f32| ((db - FLOOR) / -FLOOR).clamp(0.0, 1.0) * chh;
                        if let Some(s) = &spectrum {
                            let mut area = PathBuilder::fill();
                            area.move_to(at(0.0, chh));
                            for i in 0..n {
                                area.line_to(at(xs[i], chh - level(s[i])));
                            }
                            area.line_to(at(cw, chh));
                            area.close();
                            if let Ok(p) = area.build() {
                                window.paint_path(p, alpha(0x9aa3b8, 0.16));
                            }
                            let mut top = PathBuilder::stroke(px(1.));
                            for i in 0..n {
                                let p = at(xs[i], chh - level(s[i]));
                                if i == 0 {
                                    top.move_to(p);
                                } else {
                                    top.line_to(p);
                                }
                            }
                            if let Ok(p) = top.build() {
                                window.paint_path(p, alpha(color, 0.55));
                            }
                            // Where both are loud and close, red up to the quieter.
                            if let Some(r) = &against {
                                for i in 0..n {
                                    let (a, e) = (s[i], r[i]);
                                    let loud = ((a.min(e) - (FLOOR + 30.0)) / 18.0).clamp(0.0, 1.0);
                                    let close = (1.0 - (a - e).abs() / 9.0).clamp(0.0, 1.0);
                                    let c = loud * close;
                                    if c > 0.05 {
                                        let top = chh - level(a.min(e));
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                at(xs[i] - 1.0, top),
                                                size(px(2.5), px(chh - top)),
                                            ),
                                            linear_gradient(
                                                180.,
                                                linear_color_stop(alpha(RECORD, 0.75 * c), 0.),
                                                linear_color_stop(alpha(RECORD, 0.1 * c), 1.),
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                        if !curve.is_empty() {
                            let g = geo(cw, chh);
                            window.paint_quad(fill(
                                Bounds::new(at(0.0, g.y(0.0)), size(b.size.width, px(1.))),
                                alpha(0xffffff, 0.08),
                            ));
                            let mut line = PathBuilder::stroke(px(1.6));
                            for (i, &d) in curve.iter().enumerate() {
                                let p = at(xs[i], g.y(d.clamp(-RANGE, RANGE)));
                                if i == 0 {
                                    line.move_to(p);
                                } else {
                                    line.line_to(p);
                                }
                            }
                            if let Ok(p) = line.build() {
                                let c = if bypass { 0x6b6f7a } else { super::CURVE };
                                window.paint_path(p, rgb(c));
                            }
                        }
                        // Its bands' dots, the picked ringed.
                        if editing {
                            let g = geo(cw, chh);
                            for (i, b) in eq.bands.iter().enumerate() {
                                let (x, y) = dot(&g, b, eq.scale);
                                let ring = if picked.contains(&b.id) { 8.0 } else { 0.0 };
                                if ring > 0.0 {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                at(x - ring, y - ring),
                                                size(px(2.0 * ring), px(2.0 * ring)),
                                            ),
                                            rgb(0xffffff),
                                        )
                                        .corner_radii(px(ring)),
                                    );
                                }
                                let c = if b.off {
                                    alpha(band_color(i), 0.35)
                                } else {
                                    rgb(band_color(i)).into()
                                };
                                window.paint_quad(
                                    fill(
                                        Bounds::new(at(x - 6.0, y - 6.0), size(px(12.), px(12.))),
                                        c,
                                    )
                                    .corner_radii(px(6.)),
                                );
                            }
                        }
                    },
                )
                .absolute()
                .inset_0();
                // Its bands, to drag (right-click for a band's menu, the wheel its width); a click
                // where there's none makes one.
                let display = div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .rounded(px(6.))
                    .overflow_hidden()
                    .bg(rgb(0x0b0c0f))
                    .when(editing, |d| d.cursor_crosshair())
                    .debug_selector(move || format!("eq-instance-display-{k}"))
                    .child(picture)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            match (editing, ws.list_band_at(ch, e.position)) {
                                (true, Some(id)) => ws.press_list_band(ch, id, cx),
                                (true, None) => ws.press_list(ch, e, cx),
                                // Small, a click opens it out to edit, and makes it the one.
                                (false, _) => {
                                    ws.activate(ch, None, cx);
                                    if let Some(v) = ws.eq_view.as_mut() {
                                        v.listing.expanded = Some(ch);
                                    }
                                }
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            if let Some(id) = ws.list_band_at(ch, e.position).filter(|_| editing) {
                                cx.stop_propagation();
                                ws.activate(ch, Some(id), cx);
                                ws.band_menu(id, e.position, cx);
                            }
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                        if let Some(id) = ws.list_band_at(ch, e.position).filter(|_| editing) {
                            cx.stop_propagation();
                            ws.activate(ch, Some(id), cx);
                            ws.wheel_band(id, e, cx);
                        }
                    }));
                div()
                    .id(("eq-instance", k))
                    .h(px(row_h))
                    .flex_none()
                    .px(px(12.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .rounded(px(8.))
                    .when(ch == now, |d| d.bg(alpha(ACCENT, 0.12)))
                    .when(ch != now, |d| d.hover(|d| d.bg(rgb(0x1a1b21))))
                    .debug_selector(move || format!("eq-instance-{k}"))
                    .child(div().w(px(3.)).h_full().rounded(px(2.)).bg(rgb(color)))
                    .child(info)
                    .child(display)
                    .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                        if let Some(v) = ws.eq_view.as_mut() {
                            let hover = &mut v.listing.hover;
                            match *over {
                                true => *hover = Some(ch),
                                false if *hover == Some(ch) => *hover = None,
                                false => return,
                            }
                            cx.notify();
                        }
                    }))
            })
            .collect::<Vec<_>>();
        let seg = |label: &'static str,
                   on: bool,
                   sel: &'static str,
                   cx: &mut Context<Self>,
                   act: fn(&mut super::EqView)| {
            div()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(10.5))
                .when(on, |d| {
                    d.bg(alpha(ACCENT, 0.2)).text_color(rgb(ACCENT_TEXT))
                })
                .when(!on, |d| {
                    d.text_color(rgb(TEXT_3))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                })
                .debug_selector(move || sel.into())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.eq_view.as_mut() {
                            act(v);
                        }
                        cx.notify();
                    }),
                )
        };
        let zoom = l.zoom;
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pb(px(6.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .child("Instances"),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(format!("{count} shown")),
            )
            .when_some(l.filter.clone(), |d, f| {
                d.child(
                    div()
                        .w(px(180.))
                        .debug_selector(|| "eq-instance-filter".into())
                        .child(f),
                )
            })
            .child(div().flex_1())
            .child(seg(
                "Pinned",
                l.pinned_only,
                "eq-instance-pinned",
                cx,
                |v| v.listing.pinned_only = !v.listing.pinned_only,
            ))
            .child(
                div()
                    .flex()
                    .p(px(2.))
                    .gap(px(2.))
                    .rounded(px(6.))
                    .bg(rgb(FIELD))
                    .child(seg("Spectrum", zoom == 0, "eq-instance-zoom-0", cx, |v| {
                        (v.listing.zoom, v.listing.expanded) = (0, None)
                    }))
                    .child(seg("Curves", zoom == 1, "eq-instance-zoom-1", cx, |v| {
                        (v.listing.zoom, v.listing.expanded) = (1, None)
                    }))
                    .child(seg("Edit", zoom == 2, "eq-instance-zoom-2", cx, |v| {
                        (v.listing.zoom, v.listing.expanded) = (2, None)
                    })),
            )
            .child(seg(
                "Auto-zoom",
                l.auto_zoom,
                "eq-instance-auto-zoom",
                cx,
                |v| v.listing.auto_zoom = !v.listing.auto_zoom,
            ));
        div()
            .id("eq-list")
            .w(px(w))
            .h(px(h))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .overflow_y_scroll()
            .bg(rgb(0x0e0f13))
            .debug_selector(|| "eq-list".into())
            .child(header)
            .children(items)
            // The space beside the rows: whatever was opened out goes back.
            .child(div().flex_1().min_h(px(24.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    if let Some(v) = ws.eq_view.as_mut() {
                        v.listing.expanded = None;
                    }
                    cx.notify();
                }),
            ))
    }
}

#[cfg(test)]
impl Workspace {
    /// Every row's spectrum as if the song were playing, each its own sound: for pictures.
    pub fn pretend_list(&mut self) {
        let rows: Vec<Channel> = self.list_rows().iter().map(|r| r.ch).collect();
        let freqs = columns();
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        v.listing.spectra = rows
            .iter()
            .enumerate()
            .map(|(k, &ch)| {
                // Each a hump somewhere of its own, on a falling slope.
                let centre = [180.0, 2_400.0, 90.0, 900.0, 5_000.0][k % 5];
                let s = freqs
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| {
                        let o = (f / centre).log2() as f32;
                        let ripple = ((i * 37 + k * 11) % 13) as f32 * 0.6;
                        (-30.0 - 3.0 * o.abs() * o.abs() - ripple).max(FLOOR)
                    })
                    .collect();
                (ch, s)
            })
            .collect();
    }
}
