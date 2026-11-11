//! Settings' studio pages. Audio: what's playing now (through what, how fast, how loaded, late
//! buffers, a test tone each side), the outputs and inputs with what each can do and the inputs'
//! levels live, the rate and the buffer with what they cost in time, recordings moved back for
//! the input's lateness, the cores making sound. Recording: how deep takes are, counting in, the
//! click. Plugins: where they're looked for and what's found. Sounds & packs: who makes them,
//! where they're kept. Saving & files: how often a recovery copy's kept, where songs are.

use gpui::{Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use pina_engine::audio::{
    engine::{ClickStyle, DeviceInfo, RATES, buffer_sizes},
    record::Depth,
};

use crate::{
    ui::theme::*,
    workspace::{Name, Workspace},
};

/// A card: its title, then its rows.
fn card(title: &'static str, note: Option<&'static str>, rows: Vec<Div>) -> Div {
    div()
        .rounded(px(12.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(8.))
                .px(px(14.))
                .pt(px(12.))
                .pb(px(8.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .children(note.map(|n| div().text_size(px(12.)).text_color(rgb(TEXT_3)).child(n))),
        )
        .children(rows)
}

/// A row: its title and what it does, then `right`.
fn row(
    title: impl Into<gpui::SharedString>,
    what: impl Into<gpui::SharedString>,
    right: impl IntoElement,
) -> Div {
    let what = what.into();
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .border_t_1()
        .border_color(rgb(LINE_SOFT))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.into()),
                )
                .when(!what.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(what),
                    )
                }),
        )
        .child(right)
}

/// A device as its row says it: its channels and the rates it can run at.
fn device_line(d: &DeviceInfo, dir: &str) -> String {
    let rates = match (d.rates.first(), d.rates.last()) {
        (Some(lo), Some(hi)) if lo == hi => format!("{} kHz", khz(*lo)),
        (Some(lo), Some(hi)) => format!("{}–{} kHz", khz(*lo), khz(*hi)),
        _ => match d.own_rate {
            Some(r) => format!("{} kHz only", khz(r)),
            None => String::new(),
        },
    };
    format!("{} {dir} · {rates}", d.channels)
}

/// A Bluetooth headset on a call: its microphone open, it plays at a phone's rate.
fn call(d: &DeviceInfo) -> bool {
    d.own_rate.is_some_and(|r| r < 32_000) && (d.bluetooth || d.rates.is_empty())
}

fn khz(r: u32) -> String {
    let k = r as f32 / 1000.0;
    match k.fract() == 0.0 {
        true => format!("{k:.0}"),
        false => format!("{k:.1}"),
    }
}

/// A round button picked or not, as a radio's.
fn radio(on: bool) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .rounded(px(8.))
        .when(on, |d| {
            d.border_4().border_color(rgb(ACCENT)).bg(rgb(0xffffff))
        })
        .when(!on, |d| d.border_1().border_color(rgb(TEXT_4)))
}

impl Workspace {
    /// What can be played through and recorded from, read afresh.
    pub(crate) fn read_devices(&mut self) {
        self.outputs_info = pina_engine::audio::engine::output_info();
        self.inputs_info = pina_engine::audio::record::input_info();
    }

    /// Settings' Audio page.
    pub(crate) fn audio_page(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        if self.outputs_info.is_empty() {
            self.read_devices();
        }
        // The inputs' levels, live while this page shows; not a Bluetooth headset's, which
        // opened turns its headphones down to call quality. Tests never open the microphone.
        let listened = self
            .settings
            .audio
            .input
            .clone()
            .or_else(|| pina_engine::audio::record::input_now(None));
        let headset_in = self
            .inputs_info
            .iter()
            .find(|d| Some(&d.name) == listened.as_ref())
            .is_some_and(|d| d.bluetooth || call(d));
        if headset_in {
            self.input_meter = None;
            self.in_levels.clear();
        }
        if self.input_meter.is_none() && !headset_in && !cfg!(test) {
            self.input_meter =
                pina_engine::audio::record::InputMeter::start(self.settings.audio.input.as_deref())
                    .ok();
        }
        if let Some(m) = &self.input_meter {
            let now = m.read();
            self.in_levels.resize(now.len(), 0.0);
            for (l, n) in self.in_levels.iter_mut().zip(now) {
                *l = n.max(*l * 0.85);
            }
            cx.notify();
        }
        let audio = self.settings.audio.clone();
        let playing_through = self.player.output_device();
        let engine_rate = self.player.rate() as u32;
        let song_rate = self.session.project.sample_rate;
        let buffer = self.player.buffer();
        let load = self.load;
        let lates = self
            .lates
            .iter()
            .filter(|t| t.elapsed().as_secs() < 600)
            .count();
        let out_now = self
            .outputs_info
            .iter()
            .find(|d| Some(&d.name) == playing_through.as_ref())
            .cloned();
        let ms = |frames: u32| frames as f32 / engine_rate.max(1) as f32 * 1000.0;
        // What's playing now.
        let stat = |k: &'static str, v: String, sub: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .pl(px(18.))
                .border_l_1()
                .border_color(rgb(LINE))
                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(k))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(17.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(v),
                )
                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(sub))
        };
        let test = |label: &'static str, sides: u8, cx: &mut Context<Self>| {
            button(label, false)
                .h(px(28.))
                .px(px(10.))
                .font_family(MONO)
                .debug_selector(move || format!("setting-test-{}", label.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.player.test_tone(sides);
                        cx.notify();
                    }),
                )
        };
        let live = div()
            .flex()
            .items_center()
            .gap(px(18.))
            .px(px(16.))
            .py(px(14.))
            .rounded(px(12.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(rgb(FIELD))
                    .child(icon(Icon::Headphones, 20.).text_color(rgb(TEXT))),
            )
            .child(
                div()
                    .min_w(px(200.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_3))
                            .child("PLAYING THROUGH"),
                    )
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(playing_through.clone().unwrap_or_else(|| "Nothing".into())),
                    )
                    .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child(
                        match (&audio.device, &out_now) {
                            (None, Some(d)) => format!("{} out · following the system", d.channels),
                            (Some(_), Some(d)) => format!("{} out · picked here", d.channels),
                            _ => String::new(),
                        },
                    )),
            )
            .child(stat(
                "Rate",
                format!("{} kHz", khz(engine_rate)),
                format!("song: {} kHz", khz(song_rate)),
            ))
            .child(stat(
                "Buffer",
                if buffer > 0 {
                    buffer.to_string()
                } else {
                    "–".into()
                },
                if buffer > 0 {
                    format!("{:.1} ms out", ms(buffer))
                } else {
                    "as the system has it".into()
                },
            ))
            .child(stat(
                "Engine",
                format!("{:.0}%", load * 100.0),
                "of each buffer's time".into(),
            ))
            .child(stat(
                "Late buffers",
                lates.to_string(),
                "in the last 10 min".into(),
            ))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(6.))
                    .child(label("Test"))
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(test("L", 1, cx))
                            .child(test("R", 2, cx))
                            .child(test("Both", 3, cx)),
                    ),
            );
        let on_call = out_now.as_ref().filter(|d| call(d)).map(|d| d.name.clone());
        let banner = (engine_rate != song_rate).then(|| {
            div()
                .flex()
                .gap(px(10.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(10.))
                .bg(alpha(ACCENT, 0.08))
                .border_1()
                .border_color(alpha(ACCENT, 0.35))
                .text_size(px(12.5))
                .text_color(rgb(TEXT_2))
                .when_some(on_call.clone(), |d, name| d.border_color(alpha(AMBER, 0.45)).bg(alpha(AMBER, 0.07)).child(format!(
                    "{name} is on a call: something has its microphone open ({}another app), so it plays at {} kHz on one channel, phone quality, highs and lows gone. Pina plays through it as it is and goes back to {} kHz by itself when the microphone's let go.",
                    match listened.as_ref() == Some(&name) { true => "Pina's input is set to it, or ", false => "" },
                    khz(engine_rate),
                    khz(song_rate.min(48_000)),
                )))
                .when(on_call.is_none(), |d| d.child(format!(
                    "The song is {} kHz and Pina runs at {} kHz{}, converting the song's audio as it loads. What you bounce is the song's rate.",
                    khz(song_rate),
                    khz(engine_rate),
                    match audio.rate {
                        Some(_) => " as picked below".to_string(),
                        None => format!(" because {} can't play {} kHz", playing_through.clone().unwrap_or_default(), khz(song_rate)),
                    }
                )))
        });
        // The outputs.
        let out_row =
            |name: Option<String>, line: String, off: bool, warn: bool, cx: &mut Context<Self>| {
                let on = name == audio.device;
                let shown = name.clone().unwrap_or_else(|| "System output".into());
                let sel = format!("setting-output-{shown}");
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .mx(px(6.))
                    .px(px(10.))
                    .py(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(0x171a24)))
                    .when(off, |d| d.opacity(0.45))
                    .hover(|d| d.bg(rgb(OFF)))
                    .debug_selector(move || sel.clone())
                    .child(radio(on))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(shown),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(if warn { AMBER } else { TEXT_3 }))
                                    .child(line),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.set_output_device(name.clone(), cx);
                        }),
                    )
            };
        let system_out = pina_engine::audio::engine::output_now(
            &pina_engine::audio::engine::AudioSetup::default(),
        );
        let mut outs = vec![out_row(
            None,
            format!(
                "Follows what macOS plays through: now {}",
                system_out.unwrap_or_else(|| "nothing".into())
            ),
            false,
            false,
            cx,
        )];
        for d in self.outputs_info.clone() {
            let line = match (call(&d), d.bluetooth) {
                (true, _) => format!(
                    "On a call: {} kHz, {} channel, phone quality, while something has its microphone",
                    khz(d.own_rate.unwrap_or(0)),
                    d.channels
                ),
                (false, true) => format!(
                    "{} · Bluetooth, heard a moment late",
                    device_line(&d, "out")
                ),
                (false, false) => device_line(&d, "out"),
            };
            outs.push(out_row(Some(d.name.clone()), line, false, call(&d), cx));
        }
        if let Some(gone) = audio
            .device
            .clone()
            .filter(|n| !self.outputs_info.iter().any(|d| d.name == *n))
        {
            outs.push(out_row(
                Some(gone),
                "Not plugged in: Pina plays through the system's until it is, then goes back to it"
                    .into(),
                true,
                false,
                cx,
            ));
        }
        let pairs = out_now.as_ref().map_or(1, |d| (d.channels / 2).clamp(1, 8));
        let pair_segs: Vec<Div> = (0..pairs)
            .map(|k| {
                seg(
                    format!("{} + {}", k * 2 + 1, k * 2 + 2),
                    audio.out_pair == k,
                    26.,
                )
                .debug_selector(move || format!("setting-out-pair-{k}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let mut a = ws.settings.audio.clone();
                        a.out_pair = k;
                        ws.set_audio(a, cx);
                    }),
                )
            })
            .collect();
        let output = card("Output", None, outs).pb(px(6.)).when(pairs > 1, |d| {
            d.child(row(
                "The song plays out of",
                "Its channels",
                tray().p(px(2.)).rounded(px(8.)).children(pair_segs),
            ))
        });
        // The inputs, and their levels.
        let in_now = pina_engine::audio::record::input_now(audio.input.as_deref());
        let in_row = |name: Option<String>, line: String, warn: bool, cx: &mut Context<Self>| {
            let on = name == audio.input;
            let shown = name.clone().unwrap_or_else(|| "System input".into());
            let sel = format!("setting-input-{shown}");
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .mx(px(6.))
                .px(px(10.))
                .py(px(10.))
                .rounded(px(8.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(0x171a24)))
                .hover(|d| d.bg(rgb(OFF)))
                .debug_selector(move || sel.clone())
                .child(radio(on))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(shown),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(if warn { AMBER } else { TEXT_3 }))
                                .child(line),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.settings.audio.input = name.clone();
                        ws.settings.save();
                        // Listened to afresh.
                        ws.input_meter = None;
                        cx.notify();
                    }),
                )
        };
        let mut ins = vec![in_row(
            None,
            format!(
                "Follows macOS: now {}",
                in_now.clone().unwrap_or_else(|| "nothing".into())
            ),
            false,
            cx,
        )];
        for d in self.inputs_info.clone() {
            // A headset's microphone: running it turns the headphones down to call quality.
            let low = call(&d) || d.bluetooth;
            let line = match low {
                true => format!(
                    "{} in · Bluetooth: recording from it turns its headphones down to call quality for everything",
                    d.channels,
                ),
                false => device_line(&d, "in"),
            };
            ins.push(in_row(Some(d.name.clone()), line, low, cx));
        }
        let meters: Vec<Div> = self
            .in_levels
            .iter()
            .enumerate()
            .take(8)
            .map(|(k, &l)| {
                let db = 20.0 * l.max(1e-6).log10();
                let w = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(40.))
                            .font_family(MONO)
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .child(format!("In {}", k + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded(px(3.))
                            .bg(rgb(FIELD))
                            .relative()
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(gpui::relative(w))
                                    .bg(rgb(if db > -3.0 {
                                        CORAL
                                    } else if db > -12.0 {
                                        AMBER
                                    } else {
                                        MINT
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .w(px(56.))
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(match db < -59.0 {
                                true => "–".to_string(),
                                false => format!("{db:.0} dB"),
                            }),
                    )
            })
            .collect();
        let input = card("Input", Some("what recording hears"), ins).child(
            div()
                .px(px(14.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_t_1()
                .border_color(rgb(LINE_SOFT))
                .when(meters.is_empty() && headset_in, |d| {
                    d.child(label(format!(
                        "{}'s level isn't shown here: opening a Bluetooth headset's microphone turns its headphones down to call quality. It's heard while recording.",
                        listened.clone().unwrap_or_default()
                    )))
                })
                .when(meters.is_empty() && !headset_in, |d| {
                    d.child(label("No input to listen to"))
                })
                .children(meters),
        );
        // Rate and buffer.
        let can_rates = out_now
            .as_ref()
            .map(|d| d.rates.clone())
            .unwrap_or_default();
        let mut rate_segs = vec![
            seg(
                format!("Song's ({} k)", khz(song_rate)),
                audio.rate.is_none(),
                26.,
            )
            .debug_selector(|| "setting-rate-song".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    let mut a = ws.settings.audio.clone();
                    a.rate = None;
                    ws.set_audio(a, cx);
                }),
            ),
        ];
        for r in RATES {
            let can = can_rates.contains(&r);
            rate_segs.push(
                seg(format!("{}k", khz(r)), audio.rate == Some(r), 26.)
                    .when(!can, |d| d.opacity(0.3).cursor_default())
                    .debug_selector(move || format!("setting-rate-{r}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if can {
                                let mut a = ws.settings.audio.clone();
                                a.rate = Some(r);
                                ws.set_audio(a, cx);
                            }
                        }),
                    ),
            );
        }
        let buffers: Vec<Div> = std::iter::once(None)
            .chain(buffer_sizes(audio.device.as_deref()).into_iter().map(Some))
            .map(|size| {
                let label = size.map_or("Auto".to_string(), |b| b.to_string());
                let sel = format!("setting-buffer-{label}");
                seg(label, size == audio.buffer, 26.)
                    .debug_selector(move || sel.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let mut a = ws.settings.audio.clone();
                            a.buffer = size;
                            ws.set_audio(a, cx);
                        }),
                    )
            })
            .collect();
        let lat = |k: &'static str, v: String| {
            div()
                .flex_1()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(k))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(v),
                )
        };
        let b = buffer.max(audio.buffer.unwrap_or(0));
        let rate_card = card(
            "Rate and buffer",
            None,
            vec![
                div()
                    .px(px(14.))
                    .pb(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label(format!("Sample rate · what {} can do", playing_through.clone().unwrap_or_else(|| "the output".into()))))
                    .child(tray().p(px(2.)).rounded(px(8.)).flex().children(rate_segs))
                    .child(label("Buffer, in samples: smaller answers your keys sooner, bigger gives a heavy song room"))
                    .child(tray().p(px(2.)).rounded(px(8.)).flex().children(buffers))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(lat("In", if b > 0 { format!("{:.1} ms", ms(b)) } else { "–".into() }))
                            .child(lat("Out", if b > 0 { format!("{:.1} ms", ms(b)) } else { "–".into() }))
                            .child(lat("Round trip", if b > 0 { format!("{:.1} ms", 2.0 * ms(b) + self.settings.record_offset_ms) } else { "–".into() })),
                    ),
            ],
        );
        // The playhead in time with what's heard.
        let later = self.heard_later_ms();
        let bt = out_now.as_ref().is_some_and(|d| d.bluetooth);
        let out_name = playing_through
            .clone()
            .unwrap_or_else(|| "the output".into());
        let later_field = match self.naming.as_ref().and_then(|n| match n.what {
            Name::HeardLater => Some(n.input.clone()),
            _ => None,
        }) {
            Some(input) => div().w(px(90.)).child(crate::pages::genres::field_box(
                Some(input),
                "setting-later-field",
            )),
            None => button(format!("{later:.0} ms"), false)
                .h(px(28.))
                .font_family(MONO)
                .debug_selector(|| "setting-later".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.start_naming(Name::HeardLater, window, cx);
                    }),
                ),
        };
        let later_quick: Vec<Div> = [0.0f32, 50.0, 100.0, 150.0, 200.0, 300.0]
            .into_iter()
            .map(|v| {
                seg(format!("{v:.0}"), (later - v).abs() < 0.5, 26.)
                    .debug_selector(move || format!("setting-later-{v:.0}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_heard_later(v, cx)),
                    )
            })
            .collect();
        let taps = self.tapping.as_ref().map(|t| t.offs.len());
        let tap_row =
            match taps {
                Some(n) => div()
                    .mx(px(14.))
                    .mb(px(14.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("setting-tap-pad")
                            .debug_selector(|| "setting-tap-pad".into())
                            .flex_1()
                            .h(px(64.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(3.))
                            .rounded(px(10.))
                            .bg(alpha(ACCENT, 0.14))
                            .border_1()
                            .border_color(alpha(ACCENT, 0.6))
                            .cursor_pointer()
                            .active(|d| d.bg(alpha(ACCENT, 0.32)))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Tap on each click: Space, or here"),
                            )
                            .child(div().text_size(px(11.5)).text_color(rgb(TEXT_2)).child(
                                format!("{n} of 10 · steady, with what you hear, not what you see"),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.tap(cx);
                                }),
                            ),
                    )
                    .child(
                        button("Stop", false)
                            .h(px(64.))
                            .px(px(14.))
                            .debug_selector(|| "setting-tap-stop".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.stop_tapping(cx)),
                            ),
                    ),
                None => div(),
            };
        let in_time = card(
            "Playhead in time",
            None,
            vec![
                div()
                    .px(px(14.))
                    .pb(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child(match bt {
                        true => format!("The playhead shows what's heard as it's heard, from how late {out_name} says it is. Bluetooth says less than it really is: tap along once and the playhead's on what you hear. Kept for each output."),
                        false => format!("The playhead shows what's heard as it's heard, from how late {out_name} says it is. If it still runs ahead of the sound, tap along once. Kept for each output."),
                    }),
                div()
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(12.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("Heard later than {out_name} says, by")),
                    )
                    .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(tray().p(px(2.)).rounded(px(8.)).children(later_quick))
                        .child(later_field)
                        .child(div().flex_1())
                        .when(taps.is_none(), |d| {
                            d.child(
                                button("Tap along", false)
                                    .h(px(28.))
                                    .debug_selector(|| "setting-tap-along".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.start_tapping(cx)),
                                    ),
                            )
                        }),
                    ),
                tap_row,
            ],
        );
        // Recordings in time.
        let naming = self.naming.as_ref().and_then(|n| match n.what {
            Name::RecordOffset => Some(n.input.clone()),
            _ => None,
        });
        let offset = self.settings.record_offset_ms;
        let offset_field = match naming {
            Some(input) => div().w(px(90.)).child(crate::pages::genres::field_box(
                Some(input),
                "setting-offset-field",
            )),
            None => button(format!("{offset:.1} ms"), false)
                .h(px(28.))
                .font_family(MONO)
                .debug_selector(|| "setting-offset".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.start_naming(Name::RecordOffset, window, cx);
                    }),
                ),
        };
        let quick: Vec<Div> = [0.0f32, 3.0, 6.0, 12.0, 24.0]
            .into_iter()
            .map(|v| {
                seg(format!("{v:.0}"), (offset - v).abs() < 0.05, 26.)
                    .debug_selector(move || format!("setting-offset-{v:.0}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.settings.record_offset_ms = v;
                            ws.settings.save();
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let timing = card(
            "Recordings in time",
            None,
            vec![
                div()
                    .px(px(14.))
                    .pb(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child("Every interface hears a little late. Pina moves each take back by this much, so it lands where you played it."),
                row("Moved back by", "ms; type any, or pick", div().flex().items_center().gap(px(8.)).child(tray().p(px(2.)).rounded(px(8.)).children(quick)).child(offset_field)),
            ],
        );
        // The engine.
        let most = std::thread::available_parallelism().map_or(4, |n| n.get());
        let cores: Vec<Div> = std::iter::once(None)
            .chain([2usize, 4, 6, 8, 12, 16].into_iter().filter(|&c| c <= most).map(Some))
            .map(|c| {
                let label = c.map_or("Auto".to_string(), |c| c.to_string());
                let sel = format!("setting-cores-{label}");
                seg(label, audio.cores == c, 26.).debug_selector(move || sel.clone()).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let mut a = ws.settings.audio.clone();
                        a.cores = c;
                        // Taken up when the engine next starts.
                        ws.settings.audio = a;
                        ws.settings.save();
                        ws.status = Some("The engine uses that many cores when it next starts: change the output or reopen the song".into());
                        cx.notify();
                    }),
                )
            })
            .collect();
        let late = self.settings.late_warning;
        let engine = card(
            "Engine",
            None,
            vec![
                row(
                    "Cores making sound",
                    format!("of this computer's {most}"),
                    tray().p(px(2.)).rounded(px(8.)).children(cores),
                ),
                row(
                    "Say so when a buffer comes late",
                    "The status bar's load turns red: a dropout",
                    switch(late)
                        .debug_selector(|| "setting-late".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.settings.late_warning ^= true;
                                ws.settings.save();
                                cx.notify();
                            }),
                        ),
                ),
            ],
        );
        let rescan = button("Look again", false)
            .h(px(26.))
            .debug_selector(|| "setting-devices-rescan".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.read_devices();
                    ws.input_meter = None;
                    cx.notify();
                }),
            );
        let mut all = vec![live];
        all.extend(banner);
        all.push(
            div()
                .flex()
                .gap(px(16.))
                .items_start()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(output)
                        .child(input)
                        .child(div().flex().justify_end().child(rescan)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(rate_card)
                        .child(in_time)
                        .child(timing)
                        .child(engine),
                ),
        );
        all
    }

    /// Settings' Recording page.
    pub(crate) fn recording_page(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let s = self.settings.clone();
        let choice = |segs: Vec<Div>| tray().p(px(2.)).rounded(px(8.)).children(segs);
        let pick = |label: &'static str,
                    on: bool,
                    sel: String,
                    f: fn(&mut Workspace),
                    cx: &mut Context<Self>| {
            seg(label, on, 26.)
                .debug_selector(move || sel.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        f(ws);
                        ws.settings.save();
                        ws.player.set_click(ws.settings.click);
                        ws.gate_click();
                        cx.notify();
                    }),
                )
        };
        let depth = choice(vec![
            pick(
                "16-bit",
                s.record_depth == Depth::Int16,
                "setting-depth-16".into(),
                |ws| ws.settings.record_depth = Depth::Int16,
                cx,
            ),
            pick(
                "24-bit",
                s.record_depth == Depth::Int24,
                "setting-depth-24".into(),
                |ws| ws.settings.record_depth = Depth::Int24,
                cx,
            ),
            pick(
                "32-bit float",
                s.record_depth == Depth::Float32,
                "setting-depth-32".into(),
                |ws| ws.settings.record_depth = Depth::Float32,
                cx,
            ),
        ]);
        let count = choice(vec![
            pick(
                "Off",
                s.count_in == 0,
                "setting-count-0".into(),
                |ws| ws.settings.count_in = 0,
                cx,
            ),
            pick(
                "1 bar",
                s.count_in == 1,
                "setting-count-1".into(),
                |ws| ws.settings.count_in = 1,
                cx,
            ),
            pick(
                "2 bars",
                s.count_in == 2,
                "setting-count-2".into(),
                |ws| ws.settings.count_in = 2,
                cx,
            ),
            pick(
                "4 bars",
                s.count_in == 4,
                "setting-count-4".into(),
                |ws| ws.settings.count_in = 4,
                cx,
            ),
        ]);
        let c = s.click;
        let sound = choice(vec![
            pick(
                "Click",
                c.sound == 0,
                "setting-click-click".into(),
                |ws| ws.settings.click.sound = 0,
                cx,
            ),
            pick(
                "Wood",
                c.sound == 1,
                "setting-click-wood".into(),
                |ws| ws.settings.click.sound = 1,
                cx,
            ),
            pick(
                "Beep",
                c.sound == 2,
                "setting-click-beep".into(),
                |ws| ws.settings.click.sound = 2,
                cx,
            ),
            pick(
                "Tick",
                c.sound == 3,
                "setting-click-tick".into(),
                |ws| ws.settings.click.sound = 3,
                cx,
            ),
        ]);
        let level = choice(vec![
            pick(
                "-18",
                c.level_db == -18.0,
                "setting-click-level--18".into(),
                |ws| ws.settings.click.level_db = -18.0,
                cx,
            ),
            pick(
                "-12",
                c.level_db == -12.0,
                "setting-click-level--12".into(),
                |ws| ws.settings.click.level_db = -12.0,
                cx,
            ),
            pick(
                "-6",
                c.level_db == -6.0,
                "setting-click-level--6".into(),
                |ws| ws.settings.click.level_db = -6.0,
                cx,
            ),
            pick(
                "0 dB",
                c.level_db == 0.0,
                "setting-click-level-0".into(),
                |ws| ws.settings.click.level_db = 0.0,
                cx,
            ),
            pick(
                "+6",
                c.level_db == 6.0,
                "setting-click-level-6".into(),
                |ws| ws.settings.click.level_db = 6.0,
                cx,
            ),
        ]);
        let every = choice(vec![
            pick(
                "Beats",
                c.every <= 1,
                "setting-click-every-1".into(),
                |ws| ws.settings.click.every = 1,
                cx,
            ),
            pick(
                "8ths",
                c.every == 2,
                "setting-click-every-2".into(),
                |ws| ws.settings.click.every = 2,
                cx,
            ),
            pick(
                "16ths",
                c.every == 4,
                "setting-click-every-4".into(),
                |ws| ws.settings.click.every = 4,
                cx,
            ),
        ]);
        let flag = |on: bool, sel: &'static str, f: fn(&mut Workspace), cx: &mut Context<Self>| {
            switch(on).debug_selector(move || sel.into()).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    f(ws);
                    ws.settings.save();
                    ws.player.set_click(ws.settings.click);
                    ws.gate_click();
                    cx.notify();
                }),
            )
        };
        let hear = button("Hear it", false)
            .h(px(26.))
            .debug_selector(|| "setting-click-hear".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    // A bar of it, as it sounds now, without playing.
                    ws.player.hear_click();
                    ws.status =
                        Some("That's the click as it's set: counting in sounds the same".into());
                    cx.notify();
                }),
            );
        let _ = ClickStyle::default();
        vec![
            card(
                "Files",
                None,
                vec![
                    row(
                        "Bit depth",
                        "The rate is the engine's, or the input's own if it can't",
                        depth,
                    ),
                    row(
                        "Goes into",
                        "The song's Audio Files folder, a file a take",
                        label("WAV"),
                    ),
                ],
            ),
            card(
                "Counting in and the click",
                None,
                vec![
                    row(
                        "Count in",
                        "Recording starts after the click's counted these in",
                        count,
                    ),
                    row(
                        "Click sound",
                        "",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(sound)
                            .child(hear),
                    ),
                    row("Level", "dB from as it comes", level),
                    row("Clicks on", "", every),
                    row(
                        "Only while recording",
                        "The click button still turns it on; it's heard only while a take records",
                        flag(
                            s.click_only_recording,
                            "setting-click-only-recording",
                            |ws| ws.settings.click_only_recording ^= true,
                            cx,
                        ),
                    ),
                    row(
                        "Louder on the bar",
                        "The bar's first click higher and louder",
                        flag(
                            c.accent,
                            "setting-click-accent",
                            |ws| ws.settings.click.accent ^= true,
                            cx,
                        ),
                    ),
                ],
            ),
        ]
    }

    /// Settings' Plugins page.
    pub(crate) fn plugin_settings(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let found = self.plugins.clone().unwrap_or_default();
        let instruments = found.iter().filter(|p| p.instrument).count();
        let in_folder =
            |dir: &std::path::Path| found.iter().filter(|p| p.path.starts_with(dir)).count();
        let added = self.settings.plugin_folders.clone();
        let mut rows: Vec<Div> = pina_engine::plugin::scan::folders()
            .into_iter()
            .filter(|d| d.is_dir())
            .map(|d| {
                let n = in_folder(&d);
                let show = d.clone();
                row(
                    d.display().to_string(),
                    format!("{n} found"),
                    button("Show", false).h(px(26.)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, _| crate::platform::reveal(&show)),
                    ),
                )
            })
            .collect();
        for (k, d) in added.iter().enumerate() {
            let n = in_folder(d);
            rows.push(row(
                d.display().to_string(),
                format!("Added here · {n} found"),
                button("Take out", false)
                    .h(px(26.))
                    .debug_selector(move || format!("setting-plugin-folder-out-{k}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.settings.plugin_folders.remove(k);
                            ws.settings.save();
                            ws.start_scan(cx);
                            cx.notify();
                        }),
                    ),
            ));
        }
        rows.push(row(
            "Add a folder…",
            "Anywhere else you keep plugins",
            button("+", false)
                .h(px(26.))
                .debug_selector(|| "setting-plugin-folder-add".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| {
                        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
                            files: false,
                            directories: true,
                            multiple: true,
                            prompt: Some("Add".into()),
                        });
                        cx.spawn(async move |this, cx| {
                            let Ok(Ok(Some(paths))) = picked.await else {
                                return;
                            };
                            this.update(cx, |ws, cx| {
                                ws.settings.plugin_folders.extend(paths);
                                ws.settings.save();
                                ws.start_scan(cx);
                            })
                            .ok();
                        })
                        .detach();
                    }),
                ),
        ));
        let scanning = self.plugins.is_none();
        vec![
            card(
                "Folders",
                Some("where Pina looks for VST3, CLAP, VST and Audio Units"),
                rows,
            ),
            card(
                "Found",
                None,
                vec![row(
                    match scanning {
                        true => "Looking…".to_string(),
                        false => format!("{} plugins", found.len()),
                    },
                    format!(
                        "{} effects · {instruments} instruments",
                        found.len() - instruments
                    ),
                    button("Look again", false)
                        .h(px(26.))
                        .debug_selector(|| "setting-plugin-rescan".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.plugins = None;
                                ws.start_scan(cx);
                                cx.notify();
                            }),
                        ),
                )],
            ),
        ]
    }

    /// Settings' Sounds & packs page.
    pub(crate) fn sound_settings(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let lib = self.library();
        let (sounds, packs) = (lib.sounds.len(), lib.packs.len());
        let dir = lib.dir.clone();
        let naming = self.naming.as_ref().and_then(|n| match n.what {
            Name::Maker => Some(n.input.clone()),
            _ => None,
        });
        let maker = match self.settings.maker.as_str() {
            "" => "you".to_string(),
            m => m.to_string(),
        };
        let field = match naming {
            Some(input) => div().w(px(200.)).child(crate::pages::genres::field_box(
                Some(input),
                "setting-maker-field",
            )),
            None => button(maker, false)
                .h(px(28.))
                .debug_selector(|| "setting-maker".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.start_naming(Name::Maker, window, cx);
                    }),
                ),
        };
        let step = self.settings.load_as_step;
        vec![card(
            "Sounds & packs",
            None,
            vec![
                row(
                    "Made by",
                    "The name on the sounds and packs you share",
                    field,
                ),
                row(
                    "Load as I click",
                    "In the library, a click loads a sound (else a double-click does)",
                    switch(step)
                        .debug_selector(|| "setting-load-as-step".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.settings.load_as_step ^= true;
                                ws.settings.save();
                                cx.notify();
                            }),
                        ),
                ),
                row(
                    "Sounds folder",
                    format!(
                        "{sounds} sounds · {packs} packs of your own and brought in: a folder a pack, a file a sound"
                    ),
                    button("Show", false).h(px(26.)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, _| {
                            let _ = std::fs::create_dir_all(&dir);
                            crate::platform::reveal(&dir);
                        }),
                    ),
                ),
            ],
        )]
    }

    /// Settings' Saving & files page.
    pub(crate) fn saving_page(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let every = self.settings.keep_every;
        let segs: Vec<Div> = [(30u32, "30 s"), (60, "1 min"), (300, "5 min")]
            .into_iter()
            .map(|(secs, label)| {
                seg(label, every == secs, 26.)
                    .debug_selector(move || format!("setting-keep-{secs}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.settings.keep_every = secs;
                            ws.settings.save();
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let songs = crate::pages::projects::root();
        vec![card(
            "Songs",
            None,
            vec![
                row(
                    "Keep a recovery copy every",
                    "Of what's changed since the song was saved: brought back if Pina stops",
                    tray().p(px(2.)).rounded(px(8.)).children(segs),
                ),
                row(
                    "Songs folder",
                    songs.display().to_string(),
                    button("Show", false).h(px(26.)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, _| crate::platform::reveal(&songs)),
                    ),
                ),
            ],
        )]
    }
}

/// A small grey label.
fn label(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.5))
        .text_color(rgb(TEXT_3))
        .child(s.into())
}
