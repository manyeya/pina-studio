//! What a reference is made of, drawn, and what's done with it: its spectrum over time (its held
//! lines over it), its sections and where things start; its findings in three columns (measured,
//! estimated, unknown) and what's needed to say more; a picture's reading, its plot dragged out;
//! Create a song like this; and the song against its reference, with revisions to ask for.

use gpui::{ObjectFit, StyledImage, img, relative};
use pina_engine::study::reference::{
    BANDS, Basis, Blueprint, Closeness, Colors, FLOOR, FreqScale, HIGH_HZ, LOW_HZ, Source,
    compare::{self, Measure},
    image::table,
};

use super::like::*;
use super::*;
use crate::cli::reference;

/// Columns the spectrum is drawn in, at most.
const COLUMNS: usize = 320;

fn basis_color(b: Basis) -> u32 {
    match b {
        Basis::Measured => MINT,
        Basis::Estimated => AMBER,
        Basis::Unknown => TEXT_4,
    }
}

fn pct(v: Option<f32>) -> String {
    v.map_or("–".into(), |v| format!("{:.0}%", v * 100.0))
}

/// A blueprint drawn: its spectrum over time (bands low to high, loud bright) with its held lines
/// over it, its sections (their letters' colours, how hard each plays), and where things start,
/// low, mid and high.
pub(crate) fn blueprint_graphic(bp: &Blueprint) -> Div {
    let n = bp.frames.max(1);
    let cols = n.min(COLUMNS);
    let span = |c: usize| (c * n / cols, ((c + 1) * n / cols).max(c * n / cols + 1));
    let heat: Vec<[f32; BANDS]> = (0..cols)
        .map(|c| {
            let (a, z) = span(c);
            std::array::from_fn(|b| {
                bp.energy[a.min(bp.energy.len())..z.min(bp.energy.len())]
                    .iter()
                    .map(|r| r[b])
                    .fold(FLOOR, f32::max)
            })
        })
        .collect();
    let starts: Vec<[f32; 3]> = (0..cols)
        .map(|c| {
            let (a, z) = span(c);
            let mut o = [0f32; 3];
            for row in &bp.onsets[a.min(bp.onsets.len())..z.min(bp.onsets.len())] {
                for k in 0..3 {
                    o[k] = o[k].max(row[k]);
                }
            }
            o
        })
        .collect();
    let lines: Vec<(f32, f32, f32, f32)> = bp
        .contours
        .iter()
        .map(|c| {
            let up = match c.hz {
                Some(hz) => ((hz / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln()).clamp(0.0, 1.0),
                None => c.up,
            };
            (
                c.start as f32 / n as f32,
                c.end as f32 / n as f32,
                up,
                c.strength,
            )
        })
        .collect();
    let palette = table(Colors::Magma);
    let spectrum = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let cw = w / heat.len().max(1) as f32;
            let rh = h / BANDS as f32;
            for (c, col) in heat.iter().enumerate() {
                for (k, &v) in col.iter().enumerate() {
                    let t = ((v - FLOOR) / -FLOOR).clamp(0.0, 1.0);
                    let [r, g, bl] = palette[(t * 63.0) as usize];
                    let color = ((r as u32) << 16) | ((g as u32) << 8) | bl as u32;
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                b.origin.x + px(c as f32 * cw),
                                b.origin.y + px(h - (k + 1) as f32 * rh),
                            ),
                            gpui::size(px(cw + 0.5), px(rh + 0.5)),
                        ),
                        rgb(color),
                    ));
                }
            }
            for &(a, z, up, s) in &lines {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            b.origin.x + px(a * w),
                            b.origin.y + px((1.0 - up) * h - 1.0),
                        ),
                        gpui::size(px(((z - a) * w).max(2.0)), px(2.)),
                    ),
                    alpha(0xffffff, 0.35 + 0.5 * s),
                ));
            }
        },
    )
    .size_full();
    let lanes = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let cw = w / starts.len().max(1) as f32;
            let rh = h / 3.0;
            let colors = [CORAL, SKY, MINT];
            for (c, o) in starts.iter().enumerate() {
                for k in 0..3 {
                    if o[k] > 0.15 {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    b.origin.x + px(c as f32 * cw),
                                    b.origin.y + px(k as f32 * rh + 1.0),
                                ),
                                gpui::size(px(cw.max(1.0)), px(rh - 2.0)),
                            ),
                            alpha(colors[k], 0.25 + 0.75 * o[k]),
                        ));
                    }
                }
            }
        },
    )
    .size_full();
    let axis_label = |s: String| {
        div()
            .font_family(MONO)
            .text_size(px(9.5))
            .text_color(rgb(TEXT_4))
            .child(s)
    };
    let hz_labels: Vec<(f32, String)> = match bp.bands_hz.is_empty() {
        true => vec![(0.97, "top".into()), (0.03, "bottom".into())],
        false => [(10_000.0, "10k"), (1_000.0, "1k"), (100.0, "100")]
            .into_iter()
            .map(|(hz, s)| {
                (
                    ((hz / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln()),
                    s.to_string(),
                )
            })
            .collect(),
    };
    let sections = div()
        .h(px(30.))
        .flex()
        .gap(px(2.))
        .children(bp.sections.iter().map(|p| {
            let c = letter_color(&p.label);
            let frac = (p.end - p.start) as f32 / n as f32;
            div()
                .w(relative(frac))
                .min_w(px(2.))
                .h_full()
                .relative()
                .rounded(px(4.))
                .overflow_hidden()
                .bg(alpha(c, 0.14))
                .border_l_2()
                .border_color(rgb(c))
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(relative(p.energy.clamp(0.05, 1.0)))
                        .bg(alpha(c, 0.22)),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .left(px(6.))
                        .right(px(2.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(match p.bars {
                            Some(b) => format!("{} · {b}", p.name),
                            None => p.name.clone(),
                        }),
                )
        }));
    let start = match bp.seconds {
        Some(_) => "0:00".to_string(),
        None => "0%".to_string(),
    };
    let end = match bp.seconds {
        Some(s) => {
            let s = s.round() as u32;
            format!("{}:{:02}", s / 60, s % 60)
        }
        None => "100%".into(),
    };
    div()
        .flex()
        .gap(px(8.))
        .child(
            div()
                .w(px(34.))
                .flex_none()
                .relative()
                .h(px(140.))
                .children(hz_labels.into_iter().map(|(up, s)| {
                    div()
                        .absolute()
                        .right_0()
                        .top(px((1.0 - up) * 140.0 - 6.0))
                        .child(axis_label(s))
                })),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .h(px(140.))
                        .rounded(px(8.))
                        .overflow_hidden()
                        .bg(rgb(0x000004))
                        .debug_selector(|| "like-spectrum".into())
                        .child(spectrum),
                )
                .child(sections)
                .child(
                    div()
                        .h(px(30.))
                        .rounded(px(5.))
                        .overflow_hidden()
                        .bg(rgb(0x101015))
                        .child(lanes),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(axis_label(start))
                        .child(axis_label(
                            "starts: low (coral) · mid (sky) · high (mint)".into(),
                        ))
                        .child(axis_label(end)),
                ),
        )
}

/// A blueprint's findings in three columns: what's measured, estimated and unknown, each with how
/// sure and from what.
pub(crate) fn facts_view(bp: &Blueprint) -> Div {
    let column = |basis: Basis, title: &'static str, about: &'static str| {
        let facts: Vec<_> = bp.facts.iter().filter(|f| f.basis == basis).collect();
        let c = basis_color(basis);
        div()
            .flex_1()
            .min_w(px(0.))
            .p(px(14.))
            .rounded(px(12.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap(px(8.))
            .debug_selector(move || format!("like-facts-{}", title.to_lowercase()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(c)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(format!("{}", facts.len())),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(about),
            )
            .children(facts.into_iter().map(move |f| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(f.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.))
                            .text_color(rgb(if basis == Basis::Unknown {
                                TEXT_3
                            } else {
                                TEXT
                            }))
                            .child(f.value.clone()),
                    )
                    .when(basis != Basis::Unknown, |d| {
                        d.child(
                            div()
                                .w(px(34.))
                                .h(px(4.))
                                .flex_none()
                                .rounded(px(2.))
                                .bg(alpha(0xffffff, 0.08))
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(f.confidence.clamp(0.02, 1.0)))
                                        .rounded(px(2.))
                                        .bg(rgb(c)),
                                ),
                        )
                    })
                    .child(
                        div()
                            .w(px(40.))
                            .flex_none()
                            .text_size(px(9.5))
                            .text_color(rgb(TEXT_4))
                            .child(f.from.name()),
                    )
            }))
    };
    div()
        .flex()
        .gap(px(12.))
        .items_start()
        .child(column(
            Basis::Measured,
            "Measured",
            "Read from the audio, or given.",
        ))
        .child(column(
            Basis::Estimated,
            "Estimated",
            "Inferred from too little to be sure: guides, not facts.",
        ))
        .child(column(
            Basis::Unknown,
            "Unknown",
            "Not there to find: the song's writer chooses.",
        ))
}

/// What's needed to say more, if anything.
fn needs_view(bp: &Blueprint) -> Option<Div> {
    (!bp.needs.is_empty()).then(|| {
        div()
            .p(px(14.))
            .rounded(px(12.))
            .bg(alpha(AMBER, 0.08))
            .border_1()
            .border_color(alpha(AMBER, 0.4))
            .flex()
            .flex_col()
            .gap(px(5.))
            .debug_selector(|| "like-needs".into())
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(AMBER))
                    .child("To say more, Pina needs"),
            )
            .children(bp.needs.iter().map(|n| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("· {n}"))
            }))
            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(
                "Until then, times are places through the plot and nothing about pitch is claimed.",
            ))
    })
}

/// A primary or quiet button.
pub(crate) fn act(label: &'static str, sel: &'static str, primary: bool) -> Div {
    div()
        .h(px(32.))
        .px(px(13.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(9.))
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .debug_selector(move || sel.into())
        .when(primary, |d| {
            d.bg(rgb(TEXT))
                .text_color(rgb(BG))
                .hover(|d| d.bg(rgb(0xffffff)))
        })
        .when(!primary, |d| {
            d.bg(alpha(0xffffff, 0.04))
                .border_1()
                .border_color(alpha(0xffffff, 0.12))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(alpha(0xffffff, 0.08)).text_color(rgb(TEXT)))
        })
        .child(label)
}

impl Workspace {
    /// The sheet for a picture of a spectrogram: the picture and its reading, what it says, and
    /// the song made like it.
    pub(crate) fn picture_sheet(
        &mut self,
        bp: &Blueprint,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self
            .studies
            .like
            .reading
            .as_ref()
            .is_none_or(|r| r.file != bp.file)
        {
            self.set_reading(bp, cx);
        }
        let file = bp.file.clone();
        let (create, forget) = (file.clone(), file.clone());
        let sections = bp.sections.len();
        div()
            .id("picture-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.studies.scroll)
            .child(
                div()
                    .px(px(36.))
                    .pt(px(30.))
                    .pb(px(56.))
                    .flex()
                    .flex_col()
                    .gap(px(26.))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(chip("Spectrogram picture", AMBER))
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(rgb(TEXT_3))
                                                    .child("Everything from it is relative, and estimated unless given"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(26.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(file_name(&bp.file)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .text_color(rgb(TEXT_3))
                                            .child(format!(
                                                "{} · {sections} section{} · {} held line{}",
                                                match bp.seconds {
                                                    Some(s) => format!("{s:.0} s"),
                                                    None => "length unknown".into(),
                                                },
                                                if sections == 1 { "" } else { "s" },
                                                bp.contours.len(),
                                                if bp.contours.len() == 1 { "" } else { "s" },
                                            )),
                                    ),
                            )
                            .child(
                                act("Create a song like this", "like-create", true).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.open_create(create.clone(), cx)),
                                ),
                            )
                            .child(act("Forget", "like-forget", false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.forget_picture(forget.clone(), cx)),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(18.))
                            .items_start()
                            .child(self.picture_panel(cx))
                            .child(self.reading_panel(cx)),
                    )
                    .children(needs_view(bp))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(heading(
                                "Blueprint",
                                "Its spectrum over time, held lines in white; its sections; where things start",
                            ))
                            .child(blueprint_graphic(bp)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(heading("What's known", "Each finding, how it was come by and how sure"))
                            .child(facts_view(bp)),
                    )
                    .children(self.compare_card(&file, cx)),
            )
    }

    /// The picture, its plot framed: drag over it to set the plot's edges.
    fn picture_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(r) = &self.studies.like.reading else {
            return div();
        };
        let (w, h) = (r.size.0.max(1) as f32, r.size.1.max(1) as f32);
        let mut dw = 560.0f32;
        let mut dh = dw * h / w;
        if dh > 340.0 {
            dh = 340.0;
            dw = dh * w / h;
        }
        let k = dw / w;
        let crop = r.crop.unwrap_or([0, 0, r.size.0, r.size.1]);
        let [x, y, cw, ch] = crop.map(|v| v as f32 * k);
        let drawn = r.drawn.clone();
        let dim = |l: f32, t: f32, ww: f32, hh: f32| {
            div()
                .absolute()
                .left(px(l))
                .top(px(t))
                .w(px(ww.max(0.0)))
                .h(px(hh.max(0.0)))
                .bg(alpha(0x000000, 0.55))
        };
        let to_px = move |ws: &Workspace, p: gpui::Point<Pixels>| -> Option<(f32, f32)> {
            let r = ws.studies.like.reading.as_ref()?;
            let b = r.drawn.get()?;
            let sx = r.size.0 as f32 / f32::from(b.size.width).max(1.0);
            let sy = r.size.1 as f32 / f32::from(b.size.height).max(1.0);
            Some((
                ((f32::from(p.x - b.origin.x)) * sx).clamp(0.0, r.size.0 as f32),
                ((f32::from(p.y - b.origin.y)) * sy).clamp(0.0, r.size.1 as f32),
            ))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .relative()
                    .w(px(dw))
                    .h(px(dh))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_crosshair()
                    .debug_selector(|| "like-picture".into())
                    .child(img(r.file.clone()).size_full().object_fit(ObjectFit::Fill))
                    .child(
                        canvas(move |b, _, _| drawn.set(Some(b)), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(dim(0.0, 0.0, dw, y))
                    .child(dim(0.0, y + ch, dw, dh - y - ch))
                    .child(dim(0.0, y, x, ch))
                    .child(dim(x + cw, y, dw - x - cw, ch))
                    .child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .w(px(cw))
                            .h(px(ch))
                            .border_2()
                            .border_color(rgb(ACCENT)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if let Some(at) = to_px(ws, e.position)
                                && let Some(r) = &mut ws.studies.like.reading
                            {
                                r.dragging = Some(at);
                                r.crop = Some([at.0 as u32, at.1 as u32, 1, 1]);
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(move |ws, e: &gpui::MouseMoveEvent, _, cx| {
                        let Some(at) = to_px(ws, e.position) else {
                            return;
                        };
                        let Some(r) = &mut ws.studies.like.reading else {
                            return;
                        };
                        let Some(from) = r.dragging else {
                            return;
                        };
                        if e.pressed_button != Some(MouseButton::Left) {
                            r.dragging = None;
                            return;
                        }
                        let (x0, y0) = (from.0.min(at.0), from.1.min(at.1));
                        let (x1, y1) = (from.0.max(at.0), from.1.max(at.1));
                        r.crop = Some([
                            x0 as u32,
                            y0 as u32,
                            ((x1 - x0) as u32).max(1),
                            ((y1 - y0) as u32).max(1),
                        ]);
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Some(r) = &mut ws.studies.like.reading {
                                r.dragging = None;
                                // A click, not a drag: the plot's found again.
                                if r.crop.is_some_and(|c| c[2] < 8 || c[3] < 8) {
                                    r.crop = None;
                                }
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(format!(
                        "Plot: {}×{} px at {}, {} · drag over the picture to set its edges",
                        crop[2], crop[3], crop[0], crop[1]
                    )),
            )
    }

    /// The picture's reading: how long it runs, its frequencies and their scale, its colours and
    /// levels; read again with them.
    fn reading_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(r) = &self.studies.like.reading else {
            return div();
        };
        let row = |name: &'static str, body: Div| {
            div()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(label(name))
                .child(body)
        };
        let small =
            |e: &Entity<TextInput>, sel: &'static str| field_box(Some(e.clone()), sel).w(px(96.));
        let unit = |s: &'static str| div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(s);
        let scale = r.scale;
        let scales = tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(FreqScale::ALL.map(|s| {
                seg(capital(s.name()), scale == Some(s), 24.)
                    .debug_selector(move || format!("like-scale-{}", s.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if let Some(r) = &mut ws.studies.like.reading {
                                r.scale = (r.scale != Some(s)).then_some(s);
                            }
                            cx.notify();
                        }),
                    )
            }));
        let colors = r.colors;
        let swatches = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(Colors::ALL.map(|c| {
                let t = table(c);
                let on = colors == Some(c);
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(3.))
                    .cursor_pointer()
                    .debug_selector(move || format!("like-colors-{}", c.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if let Some(r) = &mut ws.studies.like.reading {
                                r.colors = Some(c);
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(46.))
                            .h(px(12.))
                            .rounded(px(3.))
                            .overflow_hidden()
                            .flex()
                            .border_1()
                            .border_color(if on {
                                rgb(TEXT).into()
                            } else {
                                alpha(0xffffff, 0.1)
                            })
                            .children((0..8).map(|i| {
                                let [r, g, b] = t[i * 9];
                                div()
                                    .flex_1()
                                    .h_full()
                                    .bg(rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32))
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(rgb(if on { TEXT } else { TEXT_4 }))
                            .child(c.name().replace('_', " ")),
                    )
            }));
        div()
            .flex_1()
            .min_w(px(280.))
            .p(px(16.))
            .rounded(px(12.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Reading"))
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                        "What its axes' labels say: Pina can't read text off a picture. Left empty, it stays unknown.",
                    )),
            )
            .child(row(
                "How long it runs",
                div().flex().items_center().gap(px(6.)).child(small(&r.seconds, "like-seconds")).child(unit("seconds")),
            ))
            .child(row(
                "Frequencies",
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(small(&r.low, "like-low"))
                    .child(unit("to"))
                    .child(small(&r.high, "like-high"))
                    .child(unit("Hz")),
            ))
            .child(row("How they run", scales))
            .child(row("Colours, quiet to loud", swatches))
            .child(row(
                "Level span (optional)",
                div().flex().items_center().gap(px(6.)).child(small(&r.db, "like-db")).child(unit("dB, if its colour bar says")),
            ))
            .child(
                act("Read again", "like-reread", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.reread_picture(cx)),
                ),
            )
    }

    /// The song against its reference, if it's being made like this one: compared, section by
    /// section, the differences to revise, and revisions to ask for.
    pub(crate) fn compare_card(&mut self, file: &Path, cx: &mut Context<Self>) -> Option<Div> {
        let like = self.session.project.like.as_ref()?;
        if like.file != file {
            return None;
        }
        let song = self
            .session
            .dir
            .file_name()
            .map_or("This song".into(), |n| n.to_string_lossy().into_owned());
        if self.studies.like.ask.is_none() {
            let ask = cx.new(|cx| {
                TextInput::form("", "Ask for a change: closer drums, brighter chorus…", cx)
            });
            let sent = cx.subscribe(&ask, |ws, _, e: &crate::ui::text::TextEvent, cx| {
                if let crate::ui::text::TextEvent::Confirm(text) = e {
                    ws.revise_like(text.clone(), None, cx);
                }
            });
            sent.detach();
            self.studies.like.ask = Some(ask);
        }
        let comparing = self.studies.like.comparing;
        let compared = self.song_compared();
        let meter = |name: &'static str, v: Option<f32>| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name))
                        .child(div().font_family(MONO).text_size(px(13.)).child(pct(v))),
                )
                .child(
                    div()
                        .h(px(5.))
                        .rounded(px(3.))
                        .bg(alpha(0xffffff, 0.07))
                        .child(
                            div()
                                .h_full()
                                .rounded(px(3.))
                                .w(relative(v.unwrap_or(0.0).clamp(0.0, 1.0)))
                                .bg(rgb(match v {
                                    Some(v) if v >= 0.85 => MINT,
                                    Some(v) if v >= 0.6 => AMBER,
                                    _ => CORAL,
                                })),
                        ),
                )
        };
        let mini = |v: Option<f32>| {
            div()
                .w(px(54.))
                .h(px(4.))
                .rounded(px(2.))
                .bg(alpha(0xffffff, 0.07))
                .child(
                    div()
                        .h_full()
                        .rounded(px(2.))
                        .w(relative(v.unwrap_or(0.0).clamp(0.0, 1.0)))
                        .bg(rgb(match v {
                            Some(v) if v >= 0.85 => MINT,
                            Some(v) if v >= 0.6 => AMBER,
                            None => TEXT_4,
                            _ => CORAL,
                        })),
                )
        };
        let body = match compared {
            None => div()
                .text_size(px(12.))
                .text_color(rgb(TEXT_3))
                .child(match comparing {
                    true => "Rendering and comparing…",
                    false => "Compare to see, section by section, how its groove, notes, balance and energy differ from the reference's.",
                }),
            Some(c) => {
                let o = c.report.overall;
                let sections: Vec<Div> = c
                    .report
                    .sections
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let bars = c.json["sections"][i]["bars"].as_str().unwrap_or("").to_string();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .py(px(4.))
                            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(letter_color(&s.label))))
                            .child(
                                div()
                                    .w(px(110.))
                                    .text_size(px(12.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.name.clone()),
                            )
                            .child(
                                div()
                                    .w(px(60.))
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(if bars.is_empty() { String::new() } else { format!("bars {bars}") }),
                            )
                            .child(mini(s.scores.groove))
                            .child(mini(s.scores.tonal))
                            .child(mini(s.scores.balance))
                            .child(mini(s.scores.energy))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{:+.1} dB", s.energy_db)),
                            )
                    })
                    .collect();
                let differences: Vec<Div> = c
                    .report
                    .differences
                    .iter()
                    .take(5)
                    .enumerate()
                    .map(|(i, d)| {
                        let s = &c.report.sections[d.section];
                        // Aimed at this section and measure, whatever else shares its name.
                        let aim = compare::Target {
                            measures: vec![d.measure],
                            sections: vec![d.section],
                        };
                        let request = format!(
                            "closer {} in {}",
                            reference::measure_word(d.measure),
                            s.name.to_lowercase()
                        );
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(9.))
                            .bg(alpha(0xffffff, 0.025))
                            .border_1()
                            .border_color(rgb(LINE_SOFT))
                            .child(chip(reference::measure_word(d.measure), match d.measure {
                                Measure::Groove => CORAL,
                                Measure::Tonal => VIOLET,
                                Measure::Balance => SKY,
                                Measure::Energy => AMBER,
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(div().text_size(px(12.)).child(d.says.clone()))
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(rgb(TEXT_4))
                                            .child(format!("parts: {}", d.parts.join(", "))),
                                    ),
                            )
                            .child(
                                act("Revise", "", false)
                                    .h(px(26.))
                                    .debug_selector(move || format!("like-revise-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.revise_like(request.clone(), Some(aim.clone()), cx)
                                        }),
                                    ),
                            )
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.))
                            .child(meter("Groove", o.groove))
                            .child(meter("Notes and lines", o.tonal))
                            .child(meter("Balance", o.balance))
                            .child(meter("Energy", o.energy)),
                    )
                    .when(c.report.relative, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(AMBER))
                                .child("The reference is a picture with no level scale: every measure is relative, and balance needs its frequency axis."),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.))
                                    .pl(px(198.))
                                    .children(["groove", "notes", "balance", "energy"].map(|h| {
                                        div().w(px(54.)).text_size(px(9.5)).text_color(rgb(TEXT_4)).child(h)
                                    })),
                            )
                            .children(sections),
                    )
                    .when(!differences.is_empty(), |d| {
                        d.child(label("Biggest differences")).children(differences)
                    })
                    .when(c.report.differences.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(MINT))
                                .child("No big differences: it follows the reference's shape."),
                        )
                    })
            }
        };
        let revising = self
            .studies
            .like
            .revising
            .as_ref()
            .filter(|r| r.dir == self.session.dir)
            .map(|r| {
                let line = match (r.pass > PASSES, &r.stopped) {
                    (true, Some(why)) => format!("\"{}\": stopped: {why}", r.request),
                    (true, None) => {
                        let last = r.history.last();
                        format!(
                            "\"{}\": {} ({} → {})",
                            r.request,
                            match last.map(|p| p.verdict) {
                                Some(compare::Verdict::Reached) => "done",
                                Some(compare::Verdict::Better) => "better",
                                _ => "no better: stopped",
                            },
                            pct(r.history.first().and_then(|p| p.before)),
                            pct(last.and_then(|p| p.after)),
                        )
                    }
                    (false, _) => format!(
                        "Revising \"{}\": pass {} of {PASSES}{}",
                        r.request,
                        r.pass,
                        match r.waiting {
                            true => " · the Studio Agent is at work",
                            false => " · comparing",
                        }
                    ),
                };
                let undo = r.pass > PASSES
                    && r.history
                        .last()
                        .is_some_and(|p| p.verdict == compare::Verdict::Stalled);
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(9.))
                    .bg(alpha(AGENT, 0.08))
                    .border_1()
                    .border_color(alpha(AGENT, 0.3))
                    .debug_selector(|| "like-revising".into())
                    .child(icon(Icon::Agent, 13.).text_color(rgb(AGENT_TEXT)))
                    .child(div().flex_1().text_size(px(12.)).child(line))
                    .children(r.history.iter().enumerate().map(|(i, p)| {
                        div()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{}: {}", i + 1, pct(p.after)))
                    }))
                    .when(undo, |d| {
                        d.child(
                            act("Undo the last pass", "like-undo", false)
                                .h(px(26.))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.undo_turn(cx)),
                                ),
                        )
                    })
            });
        let ask = self.studies.like.ask.clone();
        let quick = [
            "Closer drums",
            "Closer harmony",
            "Closer balance",
            "Closer energy",
        ];
        Some(
            div()
                .p(px(18.))
                .rounded(px(14.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(alpha(AGENT, 0.35))
                .flex()
                .flex_col()
                .gap(px(14.))
                .debug_selector(|| "like-compare-card".into())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(format!("{song} against it")))
                                .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                                    "Your song's render, measured the same way, section by section: as music, not pixels.",
                                )),
                        )
                        .child(
                            act(
                                match (comparing, compared.is_some()) {
                                    (true, _) => "Comparing…",
                                    (false, true) => "Compare again",
                                    (false, false) => "Compare",
                                },
                                "like-compare",
                                compared.is_none(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.compare_like(cx)),
                            ),
                        ),
                )
                .child(body)
                .children(revising)
                .when(compared.is_some(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(label("Ask for a change"))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(field_box(ask.clone(), "like-ask").flex_1())
                                    .child(act("Revise", "like-ask-send", true).on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            let text = ws
                                                .studies
                                                .like
                                                .ask
                                                .as_ref()
                                                .map(|a| a.read(cx).text().to_string())
                                                .unwrap_or_default();
                                            ws.revise_like(text, None, cx);
                                        }),
                                    )),
                            )
                            .child(div().flex().gap(px(6.)).children(quick.map(|q| {
                                div()
                                    .h(px(24.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(12.))
                                    .bg(alpha(0xffffff, 0.05))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .cursor_pointer()
                                    .hover(|d| d.bg(alpha(0xffffff, 0.1)).text_color(rgb(TEXT)))
                                    .debug_selector(move || format!("like-quick-{}", q.to_lowercase().replace(' ', "-")))
                                    .child(q)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.revise_like(q.to_lowercase(), None, cx)
                                        }),
                                    )
                            })))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_4))
                                    .child(format!(
                                        "The agent changes only what it aims at, then Pina compares again: it stops once it's reached, or when a pass stops helping ({PASSES} at most). Each pass is one turn undo takes back."
                                    )),
                            ),
                    )
                }),
        )
    }

    /// Create a song like this: its choices, over the page.
    pub fn like_create_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let c = self.studies.like.create.as_ref()?;
        let file = c.file.clone();
        let bp = self
            .studies
            .like
            .pictures
            .iter()
            .find(|b| b.file == file)
            .or_else(|| self.studies.like.blueprints.get(&file))
            .cloned();
        let picture = bp.as_ref().is_some_and(|b| b.source == Source::Image);
        let fact = |name: &str| {
            bp.as_ref()
                .and_then(|b| b.facts.iter().find(|f| f.name == name).cloned())
        };
        let takes: Vec<(String, String, Basis)> = ["Tempo", "Key", "Meter", "Form", "Length"]
            .into_iter()
            .map(|n| match fact(n) {
                Some(f) => (n.to_string(), f.value, f.basis),
                None => (n.to_string(), "the agent chooses".into(), Basis::Unknown),
            })
            .collect();
        let choose =
            |what: &'static str, about: &'static str, now: Closeness, cx: &mut Context<Self>| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .w(px(150.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(what),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(about),
                            ),
                    )
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(7.))
                            .children(Closeness::ALL.map(|c| {
                                seg(capital(c.name()), now == c, 26.)
                                    .debug_selector(move || {
                                        format!("like-{}-{}", what.to_lowercase(), c.name())
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            if let Some(f) = &mut ws.studies.like.create {
                                                match what {
                                                    "Rhythm" => f.rhythm = c,
                                                    "Form" => f.form = c,
                                                    _ => f.sound = c,
                                                }
                                            }
                                            cx.notify();
                                        }),
                                    )
                            })),
                    )
            };
        let (rhythm, form, sound, style) = (c.rhythm, c.form, c.sound, c.style.clone());
        let sheet = div()
            .w(px(560.))
            .p(px(24.))
            .rounded(px(16.))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(18.))
            .debug_selector(|| "like-create-sheet".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(19.)).font_weight(FontWeight::BOLD).child("Create a song like this"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(chip(if picture { "Spectrogram picture" } else { "Recording" }, if picture { AMBER } else { MINT }))
                            .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child(file_name(&file))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("From the reference"))
                    .children(takes.into_iter().map(|(n, v, b)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(basis_color(b))))
                            .child(div().w(px(60.)).text_size(px(11.5)).text_color(rgb(TEXT_3)).child(n))
                            .child(div().flex_1().text_size(px(12.)).child(v))
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(match b {
                                        Basis::Measured => "measured",
                                        Basis::Estimated => "estimated",
                                        Basis::Unknown => "a creative choice",
                                    }),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("Style"))
                    .child(field_box(Some(style), "like-style")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label("How closely to follow it"))
                    .child(choose("Rhythm", "where its parts hit", rhythm, cx))
                    .child(choose("Form", "its sections and their energy", form, cx))
                    .child(choose("Sound", "its balance and held lines", sound, cx)),
            )
            .child(
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(alpha(AGENT, 0.07))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_2))
                    .child(
                        "The agent writes every note: new parts and sounds, not a copy. Then Pina renders it and compares it with the reference, section by section.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(act("Cancel", "like-create-cancel", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.studies.like.create = None;
                            cx.notify();
                        }),
                    ))
                    .child(act("Create and ask the agent", "like-create-go", true).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, window, cx| ws.create_like(window, cx)),
                    )),
            );
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(alpha(0x000000, 0.55))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.studies.like.create = None;
                            cx.notify();
                        }),
                    )
                    .child(sheet),
            )
            .with_priority(3),
        )
    }
}
