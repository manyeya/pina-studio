//! What the EQ's keys and menus do.

use super::*;

/// What a click in the EQ window or one of its menus does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EqOp {
    /// The bands picked made this shape, or this slope, or put on this part of the sound.
    Shape(Shape),
    Slope(f32),
    Placement(Placement),
    /// The bands picked split in two: left and right, or mid and side.
    Split,
    /// The bands picked made dynamic, or still.
    Dynamic,
    /// The bands picked switched off, or on.
    Off,
    Delete,
    /// A band picked (alone, or added to those picked).
    Pick(u16, bool),
    SelectAll,
    Copy,
    /// Every band copied.
    CopyAll,
    Paste,
    /// Every band taken out.
    Clear,
    /// The analyzer: pre or post on and off, its range, resolution, speed, tilt; freeze.
    Pre,
    Post,
    AnalyzerRange(u8),
    Resolution(u8),
    Speed(u8),
    Tilt(u8),
    Freeze,
    Range(u8),
    Bypass,
    Popup(Popup),
    Piano,
    /// Shows A or B; copies the one shown to the other.
    Ab,
    CopyAb,
    FullScreen,
    /// A preset: one of Pina Studio's (by its place), or one saved (by its place in the list).
    Factory(usize),
    Preset(usize),
    SavePreset,
    /// The bands picked: gain-Q interaction on or off.
    GainQ,
    /// The EQ's character, auto gain, phase, and whether its pan is mid against side.
    Character(Character),
    AutoGain,
    Invert,
    PanMs(bool),
    /// The dynamics' settings shown or hidden; the bands picked: dynamics bypassed, cleared,
    /// listening to the sidechain, or between two frequencies (or to what they cover);
    /// their threshold their own.
    DynExpand,
    DynBypass,
    DynClear,
    External,
    Trigger,
    AutoThreshold,
    /// The bands picked: spectral dynamics on or off; their tilt on or off.
    Spectral,
    SpectralTilt,
    /// How the EQ treats phase; how fine its linear phase and spectral dynamics work.
    Processing(Processing),
    PhaseResolution(u8),
    /// The track the EQ's sidechain is (by id), or none.
    Sidechain(Option<u64>),
    /// A track to compare with (by id), or none; collisions shown or not; Spectrum Grab on
    /// or off.
    Ext(Option<u64>),
    Collisions,
    Grab,
    /// The instance list shown or not; EQ Sketch on or off.
    List,
    Sketch,
    /// EQ Match: opened (comparing with a track, if given), its reference a track or a file
    /// asked for, matched, its bands fewer or more, finished, or closed.
    Match(Option<u64>),
    MatchTrack(u64),
    MatchFile,
    MatchRun,
    MatchBands(i8),
    MatchFinish,
    MatchCancel,
}

impl Workspace {
    pub fn eq_op(&mut self, op: EqOp, cx: &mut Context<Self>) {
        let Some(ch) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        match op {
            EqOp::Shape(shape) => self.edit_picked(cx, |b| {
                if b.shape != shape {
                    b.q = shape.default_q();
                    // Its slope as near as the new shape takes (a brickwall's no bell's).
                    b.slope = match b.slope >= Shape::BRICKWALL {
                        true => 12.0,
                        false => shape.fit_slope(b.slope),
                    };
                    if !shape.has_gain() {
                        b.dynamic = None;
                    }
                }
                b.shape = shape;
            }),
            EqOp::Slope(slope) => self.edit_picked(cx, |b| {
                if b.shape.slope_range().is_some() {
                    b.slope = b.shape.fit_slope(slope);
                }
            }),
            EqOp::Placement(place) => self.edit_picked(cx, |b| b.placement = place),
            EqOp::Split => {
                let picked = self
                    .eq_view
                    .as_ref()
                    .map(|v| v.picked.clone())
                    .unwrap_or_default();
                let mut made = Vec::new();
                self.edit(cx, |ws| {
                    let Some(eq) = ws.eq_mut() else { return };
                    for id in &picked {
                        let Some(i) = eq.bands.find(*id) else {
                            continue;
                        };
                        // Left and right from a band on both; mid and side from one on either.
                        let (a, b) = match eq.bands[i].placement {
                            Placement::Mid | Placement::Side => (Placement::Mid, Placement::Side),
                            _ => (Placement::Left, Placement::Right),
                        };
                        eq.bands[i].placement = a;
                        let mut twin = eq.bands[i];
                        (twin.placement, twin.id) = (b, 0);
                        if let Some(j) = eq.bands.insert(i + 1, twin) {
                            made.push(eq.bands[j].id);
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.extend(made);
                }
            }
            EqOp::Dynamic => {
                let all = self.picked_bands().iter().all(|b| b.dynamic.is_some());
                self.edit_picked(cx, |b| {
                    b.dynamic = match (all, b.shape.has_gain()) {
                        (false, true) => Some(b.dynamic.unwrap_or_default()),
                        _ => None,
                    }
                })
            }
            EqOp::Off => {
                let all_off = self.picked_bands().iter().all(|b| b.off);
                self.edit_picked(cx, |b| b.off = !all_off)
            }
            EqOp::Delete => {
                let picked = self
                    .eq_view
                    .as_ref()
                    .map(|v| v.picked.clone())
                    .unwrap_or_default();
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        for id in &picked {
                            if let Some(i) = eq.bands.find(*id) {
                                eq.bands.remove(i);
                            }
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.clear();
                    v.hover = None;
                    v.solo = v.solo.filter(|s| !picked.contains(&s.id));
                }
            }
            EqOp::Pick(id, add) => {
                if let Some(v) = self.eq_view.as_mut() {
                    match add {
                        true if !v.picked.contains(&id) => v.picked.push(id),
                        true => {}
                        false => v.picked = vec![id],
                    }
                }
            }
            EqOp::SelectAll => {
                let ids = self.eq_order();
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = ids;
                }
            }
            EqOp::Copy | EqOp::CopyAll => {
                let bands = match op {
                    EqOp::Copy => self.picked_bands(),
                    _ => self
                        .shown_eq()
                        .map(|(_, e)| e.bands.to_vec())
                        .unwrap_or_default(),
                };
                if !bands.is_empty() {
                    let n = bands.len();
                    self.eq_clipboard = bands;
                    self.status = Some(format!("{n} band{} copied", if n == 1 { "" } else { "s" }));
                }
            }
            EqOp::Paste => {
                let bands = self.eq_clipboard.clone();
                let mut made = Vec::new();
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        for b in &bands {
                            if let Some(i) = eq.bands.push(EqBand { id: 0, ..*b }) {
                                made.push(eq.bands[i].id);
                            }
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = made;
                }
            }
            EqOp::Clear => {
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        eq.bands = Bands::default();
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.clear();
                    v.solo = None;
                }
            }
            EqOp::Pre => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.pre = !v.analysis.pre;
                }
            }
            EqOp::Post => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.post = !v.analysis.post;
                }
            }
            EqOp::AnalyzerRange(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.range = r;
                    v.columns.clear();
                }
            }
            EqOp::Resolution(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.resolution = r;
                }
            }
            EqOp::Speed(s) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.speed = s;
                }
            }
            EqOp::Tilt(t) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.tilt = t as f32 * 1.5;
                }
            }
            EqOp::Freeze => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.freeze = !v.analysis.freeze;
                }
            }
            EqOp::Range(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.range = r;
                }
            }
            EqOp::Bypass => self.edit(cx, |ws| {
                if let Some(eq) = ws.strip_dsp_mut(ch).0.as_mut() {
                    eq.bypass = !eq.bypass;
                }
            }),
            EqOp::Popup(p) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.popup = if v.popup == Some(p) { None } else { Some(p) };
                }
            }
            EqOp::Piano => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.piano = !v.piano;
                }
            }
            EqOp::Ab => {
                let Some((_, now)) = self.shown_eq() else {
                    return;
                };
                let other = self.eq_view.as_ref().and_then(|v| v.other).unwrap_or(now);
                self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(other));
                if let Some(v) = self.eq_view.as_mut() {
                    v.other = Some(now);
                    v.on_b = !v.on_b;
                    v.picked.clear();
                }
            }
            EqOp::CopyAb => {
                let now = self.shown_eq().map(|(_, e)| e);
                if let Some(v) = self.eq_view.as_mut() {
                    v.other = now;
                    self.status = Some(format!(
                        "{} copied to {}",
                        if v.on_b { "B" } else { "A" },
                        if v.on_b { "A" } else { "B" }
                    ));
                }
            }
            EqOp::FullScreen => {
                let (vw, vh) = (self.viewport_w, self.viewport_h);
                if let Some(v) = self.eq_view.as_mut() {
                    match v.restore.take() {
                        Some((at, size)) => (v.at, v.size) = (at, size),
                        None => {
                            v.restore = Some((v.at, v.size));
                            v.at = (8.0, CONTROL_BAR_H + 4.0);
                            v.size = (vw - 16.0, vh - CONTROL_BAR_H - STATUS_H - 8.0);
                        }
                    }
                }
            }
            EqOp::Factory(i) => {
                if let Some((name, eq)) = factory().into_iter().nth(i) {
                    self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(eq));
                    self.status = Some(format!("Loaded {name}"));
                }
            }
            EqOp::Preset(i) => {
                if let Some((name, path)) = saved_presets().into_iter().nth(i) {
                    let read = std::fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|b| serde_json::from_slice::<Eq>(&b).map_err(|e| e.to_string()));
                    match read {
                        Ok(eq) => {
                            self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(eq));
                            self.status = Some(format!("Loaded {name}"));
                        }
                        Err(e) => self.status = Some(format!("{name}: {e}")),
                    }
                }
            }
            EqOp::GainQ => {
                let all = self.picked_bands().iter().all(|b| b.gain_q);
                self.edit_picked(cx, |b| b.gain_q = !all)
            }
            EqOp::Character(c) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.character = c;
                }
            }),
            EqOp::AutoGain => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.auto_gain = !eq.auto_gain;
                }
            }),
            EqOp::Invert => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.invert = !eq.invert;
                }
            }),
            EqOp::PanMs(ms) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.pan_ms = ms;
                }
            }),
            EqOp::DynExpand => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.dyn_open = !v.dyn_open;
                }
            }
            EqOp::DynBypass => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.bypass));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.bypass = !all;
                    }
                })
            }
            EqOp::DynClear => self.edit_picked(cx, |b| b.dynamic = None),
            EqOp::Spectral => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.spectral));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.spectral = !all;
                    }
                })
            }
            EqOp::SpectralTilt => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.tilt));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.tilt = !all;
                    }
                })
            }
            EqOp::Processing(p) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.processing = p;
                }
            }),
            EqOp::PhaseResolution(r) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.resolution = r.min(3);
                }
            }),
            EqOp::External => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.external));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.external = !all;
                    }
                })
            }
            EqOp::Trigger => {
                let free = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.trigger.is_some()));
                self.edit_picked(cx, |b| {
                    let freq = b.freq;
                    if let Some(d) = b.dynamic.as_mut() {
                        d.trigger = match free {
                            true => None,
                            false => Some([(freq / 2.0).max(10.0), (freq * 2.0).min(30_000.0)]),
                        };
                    }
                })
            }
            EqOp::AutoThreshold => self.edit_picked(cx, |b| {
                if let Some(d) = b.dynamic.as_mut() {
                    d.threshold_db = None;
                }
            }),
            EqOp::Sidechain(source) => {
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        eq.sidechain = source;
                    }
                });
                // Picked from a band's dynamics: that band listens to it.
                if source.is_some() {
                    self.edit_picked(cx, |b| {
                        if let Some(d) = b.dynamic.as_mut() {
                            d.external = true;
                        }
                    });
                }
            }
            EqOp::Ext(source) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.ext = source;
                    v.ext_spec.clear();
                }
            }
            EqOp::Collisions => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.collisions = !v.analysis.collisions;
                }
            }
            EqOp::Grab => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.grab = !v.analysis.grab;
                }
            }
            EqOp::List => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.list = !v.list;
                }
                self.open_list(cx);
            }
            EqOp::Sketch => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.sketching = !v.sketching;
                    v.stroke.clear();
                    v.picked.clear();
                }
            }
            EqOp::Match(..)
            | EqOp::MatchTrack(_)
            | EqOp::MatchFile
            | EqOp::MatchRun
            | EqOp::MatchBands(_)
            | EqOp::MatchFinish
            | EqOp::MatchCancel => self.match_op(op, cx),
            EqOp::SavePreset => {
                let Some((_, eq)) = self.shown_eq() else {
                    return;
                };
                let name = format!("{} EQ", self.channel_look(ch).0);
                let dir = presets_dir();
                let mut path = dir.join(format!("{name}.json"));
                let mut n = 2;
                while path.exists() {
                    path = dir.join(format!("{name} {n}.json"));
                    n += 1;
                }
                let saved = std::fs::create_dir_all(&dir).and_then(|_| {
                    std::fs::write(&path, serde_json::to_vec_pretty(&eq).unwrap_or_default())
                });
                self.status = Some(match saved {
                    Ok(()) => format!(
                        "Saved the preset {}",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    ),
                    Err(e) => format!("Couldn't save the preset: {e}"),
                });
            }
        }
        cx.notify();
    }

    pub(crate) fn picked_bands(&self) -> Vec<EqBand> {
        let Some((_, eq)) = self.shown_eq() else {
            return Vec::new();
        };
        let picked = self
            .eq_view
            .as_ref()
            .map(|v| v.picked.clone())
            .unwrap_or_default();
        picked
            .iter()
            .filter_map(|&id| eq.bands.find(id).map(|i| eq.bands[i]))
            .collect()
    }

    /// Delete, with the window last clicked in and bands picked: they go.
    pub fn delete_eq_band(&mut self, cx: &mut Context<Self>) -> bool {
        if !self
            .eq_view
            .as_ref()
            .is_some_and(|v| v.focused && !v.picked.is_empty())
        {
            return false;
        }
        self.eq_op(EqOp::Delete, cx);
        true
    }

    /// ⌘C and ⌘V with the window last clicked in: its bands.
    pub fn eq_key(&mut self, op: EqOp, cx: &mut Context<Self>) -> bool {
        if !self.eq_view.as_ref().is_some_and(|v| v.focused) {
            return false;
        }
        self.eq_op(op, cx);
        true
    }
}
