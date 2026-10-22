//! A song compiled into what plays: each track's events, clips, controls and automation in samples, its sends and output, delay compensation.

use super::*;

pub(crate) struct Clip {
    /// Timeline position and length, and where in the file it starts, all in samples.
    pub(crate) start: u64,
    pub(crate) len: u64,
    pub(crate) offset: u64,
    pub(crate) data: Arc<AudioData>,
    /// Its region's fades: where the region starts and how long it fades in; where it ends and
    /// how long it fades out (samples).
    pub(crate) fades: [u64; 4],
}

/// A clip's level at a sample, under its region's fades: an equal-power curve each end.
pub(crate) fn faded(c: &Clip, t: u64) -> f32 {
    let [from, fade_in, to, fade_out] = c.fades;
    let mut g = 1.0;
    if fade_in > 0 && t < from + fade_in {
        g *= ((t.saturating_sub(from) as f32 / fade_in as f32) * std::f32::consts::FRAC_PI_2).sin();
    }
    if fade_out > 0 && t + fade_out > to {
        g *= ((to.saturating_sub(t) as f32 / fade_out as f32) * std::f32::consts::FRAC_PI_2).sin();
    }
    g
}

#[derive(Clone, Copy)]
pub(crate) struct Event {
    pub(crate) at: u64,
    pub(crate) pitch: u8,
    /// 0 is a note off.
    pub(crate) velocity: f32,
}

/// A controller move, pitch bend, pressure or note expression, at a sample.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ControlEvent {
    pub(crate) at: u64,
    pub(crate) kind: ControlKind,
}

/// What every lane plays, and how loud.
pub struct Arrangement {
    pub(crate) tempo: f64,
    pub(crate) master: f32,
    pub(crate) plans: Vec<Plan>,
    pub(crate) auxes: Vec<AuxPlan>,
    /// Each stack's shared modulators' values this block, by aux.
    pub(crate) stack_mods: Vec<[f32; MAX_MODS]>,
    /// What tracks send to each aux, this block.
    pub(crate) buses: Vec<[Vec<f32>; 2]>,
    /// How far ahead of what's heard the engine reads, so plugins' latency doesn't make it late.
    pub(crate) lookahead: u64,
    /// The master's built-in EQ and compressor.
    pub(crate) master_strip: StripDsp,
    /// Where playback loops, in samples, while the cycle is on.
    pub(crate) cycle: Option<(u64, u64)>,
    /// The lane holding the master's plugins, and the track each of them hears as a sidechain.
    pub(crate) master_lane: Option<usize>,
    pub(crate) master_sidechains: Vec<Option<usize>>,
    /// The order tracks are worked out in: a sidechain's source before what hears it.
    pub(crate) order: Vec<usize>,
    /// Each track's output this block, kept for the plugins that hear it as a sidechain.
    pub(crate) sidechains: Vec<[Vec<f32>; 2]>,
    /// A beat, in quarter notes, and beats to a bar: for the metronome.
    pub(crate) beat: f64,
    pub(crate) beats_per_bar: u64,
}

impl Arrangement {
    pub(crate) fn empty() -> Self {
        Arrangement {
            tempo: 120.0,
            master: 1.0,
            plans: Vec::new(),
            auxes: Vec::new(),
            stack_mods: Vec::new(),
            buses: Vec::new(),
            lookahead: 0,
            master_strip: StripDsp::default(),
            cycle: None,
            master_lane: None,
            master_sidechains: Vec::new(),
            order: Vec::new(),
            sidechains: Vec::new(),
            beat: 1.0,
            beats_per_bar: 4,
        }
    }
}

/// A channel's built-in EQ, compressor and saturator, unless absent or bypassed; and what the
/// EQ's linear phase or spectral dynamics need, made here, off the audio thread.
pub(crate) fn strip_dsp(
    eq: Option<&Eq>,
    comp: Option<&Comp>,
    sat: Option<&Saturator>,
    rate: f64,
) -> StripDsp {
    let eq = eq.filter(|e| !e.bypass).copied();
    StripDsp {
        made: eq.as_ref().map(|e| Arc::new(Made::new(e, rate))),
        eq,
        comp: comp.filter(|c| !c.bypass).copied(),
        sat: sat.filter(|s| !s.bypass).copied(),
    }
}

/// What a channel's built-in EQ, compressor and saturator are set to; the lane's `Strip` (or the
/// engine's, for the master) runs them.
#[derive(Default)]
pub(crate) struct StripDsp {
    pub(crate) eq: Option<Eq>,
    pub(crate) made: Option<Arc<Made>>,
    pub(crate) comp: Option<Comp>,
    pub(crate) sat: Option<Saturator>,
}

/// What a Live Loops cell plays, from 0, over `length` samples.
pub(crate) struct CellPlan {
    pub(crate) events: Vec<Event>,
    pub(crate) controls: Vec<ControlEvent>,
    pub(crate) clips: Vec<Clip>,
    pub(crate) length: u64,
}

/// An aux: it plays its bus through its inserts into the master.
pub(crate) struct AuxPlan {
    pub(crate) slot: usize,
    /// A stack's shared modulators, the lane each follower hears, and their settings'
    /// automation (from the first track in it with a lane for one).
    pub(crate) mods: Vec<ModKind>,
    pub(crate) follow: [usize; MAX_MODS],
    pub(crate) mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)>,
    pub(crate) gain: [f32; 2],
    pub(crate) dsp: StripDsp,
    /// Delay compensation after its inserts.
    pub(crate) delay: usize,
    /// The track each insert hears as a sidechain, if any.
    pub(crate) sidechains: Vec<Option<usize>>,
}

/// The sidechain an insert hears: its source track's output this block.
pub(crate) fn sidechain<'a>(
    buffers: &'a [[Vec<f32>; 2]],
    source: Option<&Option<usize>>,
    frames: usize,
) -> Option<[&'a [f32]; 2]> {
    let [l, r] = buffers.get((*source?)?)?;
    Some([&l[..frames], &r[..frames]])
}

/// Plugin delay compensation: delays that line every track and aux up at the master, and how
/// far ahead the engine reads so that what's heard is on time.
#[derive(Debug, PartialEq)]
pub(crate) struct Compensation {
    pub(crate) lookahead: u64,
    /// Per track, before its sends.
    pub(crate) tracks: Vec<usize>,
    /// On every track's direct path, to wait for the slowest aux.
    pub(crate) direct: usize,
    /// Per aux, after its inserts.
    pub(crate) auxes: Vec<usize>,
}

pub(crate) fn compensate(tracks: &[u32], auxes: &[u32]) -> Compensation {
    let slowest = |l: &[u32]| {
        l.iter()
            .copied()
            .max()
            .unwrap_or(0)
            .min(MAX_DELAY as u32 - 1)
    };
    let (t, a) = (slowest(tracks), slowest(auxes));
    let behind = |max: u32| move |&l: &u32| (max - l.min(max)) as usize;
    Compensation {
        lookahead: (t + a) as u64,
        tracks: tracks.iter().map(behind(t)).collect(),
        direct: a as usize,
        auxes: auxes.iter().map(behind(a)).collect(),
    }
}

/// What one lane plays.
pub(crate) struct Plan {
    pub(crate) slot: usize,
    /// Sorted by time, offs before ons at the same sample.
    pub(crate) events: Vec<Event>,
    /// Sorted by start.
    pub(crate) clips: Vec<Clip>,
    /// 0 when muted or another track is soloed.
    pub(crate) audible: f32,
    pub(crate) volume_db: f32,
    pub(crate) pan: f32,
    pub(crate) pan_mode: PanMode,
    pub(crate) pan_law: PanLaw,
    pub(crate) spread: f32,
    /// Binaural: its height and distance (the way round follows the pan).
    pub(crate) place: Place,
    /// Its modulators (sends by their place in `sends`), the lane each follower hears, and the
    /// set values of the plugin parameters they move.
    pub(crate) mods: Vec<Modulator>,
    pub(crate) follow: [usize; MAX_MODS],
    pub(crate) param_base: Vec<(Which, u32, f32)>,
    /// Automation, with ticks turned to samples; empty follows the fader.
    pub(crate) volume: Vec<AutoPoint>,
    pub(crate) pans: Vec<AutoPoint>,
    /// 1 where automation mutes it.
    pub(crate) mutes: Vec<AutoPoint>,
    /// Each send's level automation (dB), as `sends`; empty follows its knob.
    pub(crate) send_levels: Vec<Vec<AutoPoint>>,
    /// Plugin parameters' automation, in samples, 0 to 1.
    pub(crate) params: Vec<(Which, u32, Vec<AutoPoint>)>,
    /// The EQ's bands' automation, in samples: Hz, dB, Q.
    pub(crate) eq_auto: Vec<(u8, EqField, Vec<AutoPoint>)>,
    /// Its modulators' settings' automation, in samples: (modulator, setting, points).
    pub(crate) mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)>,
    /// Aux index, level, and whether it's taken before the fader.
    pub(crate) sends: Vec<(usize, f32, bool)>,
    /// Controller moves, sorted by time.
    pub(crate) controls: Vec<ControlEvent>,
    /// Live Loops cells, in the track's order.
    pub(crate) cells: Vec<CellPlan>,
    /// The track each insert hears as a sidechain, if any; and whether any plugin hears this
    /// one.
    pub(crate) sidechains: Vec<Option<usize>>,
    pub(crate) feeds: bool,
    /// The aux it plays into instead of the master.
    pub(crate) output: Option<usize>,
    /// The sampler's settings, which it follows as they change.
    pub(crate) sampler: Option<Arc<crate::song::project::Sampler>>,
    /// Delay compensation before the sends, and on the direct path after them.
    pub(crate) delay: usize,
    pub(crate) direct: usize,
    pub(crate) dsp: StripDsp,
}

impl Plan {
    /// Moves a plugin's automated parameters across a block, `from` to `to` (samples read), and
    /// its modulated ones about where they're automated or set.
    pub(crate) fn automate(
        &self,
        which: Which,
        p: &mut Processor,
        (from, to): (u64, u64),
        o: &Offsets,
    ) {
        for (_, id, points) in self.params.iter().filter(|a| a.0 == which) {
            let off = o.plugin(which, *id).unwrap_or(0.0);
            let at =
                |s: u64| (value_at(points, s as f64).unwrap_or(0.0) + off).clamp(0.0, 1.0) as f64;
            p.automate(*id, at(from), at(to.saturating_sub(1)));
        }
        for &(w, id, off) in &o.plugins[..o.plugin_count] {
            if w != which || self.params.iter().any(|a| a.0 == w && a.1 == id) {
                continue;
            }
            let base = self
                .param_base
                .iter()
                .find(|b| b.0 == w && b.1 == id)
                .map_or(0.5, |b| b.2);
            let v = (base + off).clamp(0.0, 1.0) as f64;
            p.automate(id, v, v);
        }
    }

    /// The pan at a sample, as automated or set.
    pub(crate) fn pan_at(&self, at: u64) -> f32 {
        value_at(&self.pans, at as f64).unwrap_or(self.pan)
    }

    /// 1 when it sounds, 0 when muted, soloed out or muted by automation.
    pub(crate) fn audible_at(&self, at: u64) -> f32 {
        self.audible
            * (1.0
                - value_at(&self.mutes, at as f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0))
    }

    /// A send's level (as gain) at a sample, as automated or set.
    pub(crate) fn send_at(&self, k: usize, at: u64) -> f32 {
        match self.send_levels.get(k).and_then(|p| value_at(p, at as f64)) {
            Some(level) => db(level),
            None => self.sends[k].1,
        }
    }

    /// How left and right become the mix's at a sample: the fader, and the pan by its mode;
    /// each moved by its modulators (`by`: dB, pan).
    pub(crate) fn matrix(&self, at: u64, by: (f32, f32)) -> [f32; 4] {
        let volume = value_at(&self.volume, at as f64).unwrap_or(self.volume_db);
        let volume = modulate::apply_volume(volume, by.0);
        pan_matrix(
            self.pan_mode,
            self.pan_law,
            modulate::apply_pan(self.pan_at(at), by.1),
            self.spread,
            self.audible_at(at) * db(volume),
        )
    }
}

pub(crate) fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Compiles what `project` plays at `rate`. Track `i` is on lane `tracks[i].0`, whose plugins
/// make it `tracks[i].1` samples late; likewise auxes; the master's plugins are on lane `master`.
/// Regions whose audio can't be read are left out, and the reasons returned, so one missing file
/// doesn't stop the rest playing.
pub fn compile(
    project: &Project,
    rate: f64,
    tracks: &[(usize, u32)],
    auxes: &[(usize, u32)],
    master: Option<usize>,
    files: &mut AudioFiles,
) -> (Arrangement, Vec<String>) {
    files.begin();
    let mut problems = Vec::new();
    // Each channel's plugins' latency, and its EQ's.
    let eq_latency = |eq: &Option<Eq>| eq.as_ref().map_or(0, crate::dsp::eq::latency);
    let latencies = |l: &[(usize, u32)], eqs: &mut dyn Iterator<Item = &Option<Eq>>| {
        l.iter()
            .zip(eqs)
            .map(|(&(_, l), eq)| l + eq_latency(eq))
            .collect::<Vec<_>>()
    };
    let pdc = compensate(
        &latencies(tracks, &mut project.tracks.iter().map(|t| &t.eq)),
        &latencies(auxes, &mut project.auxes.iter().map(|a| &a.eq)),
    );
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut plans = Vec::new();
    // Which track (by its place) each plugin hears as a sidechain, by the source's id.
    let source = |r: &PluginRef| {
        let id = r.sidechain?;
        project.tracks.iter().position(|t| t.id == id)
    };
    let mut feeds = vec![false; project.tracks.len()];
    // Each channel's: its inserts', then its EQ's (for its dynamic bands), last.
    let eq_source = |eq: &Option<Eq>| {
        let id = eq.as_ref()?.sidechain?;
        project.tracks.iter().position(|t| t.id == id)
    };
    let mut with_eq = |inserts: &[PluginRef], eq: &Option<Eq>| -> Vec<Option<usize>> {
        let mut found: Vec<_> = inserts.iter().map(source).collect();
        found.push(eq_source(eq));
        for &s in found.iter().flatten() {
            feeds[s] = true;
        }
        found
    };
    let track_sidechains: Vec<_> = project
        .tracks
        .iter()
        .map(|t| with_eq(&t.inserts, &t.eq))
        .collect();
    let aux_sidechains: Vec<_> = project
        .auxes
        .iter()
        .map(|a| with_eq(&a.inserts, &a.eq))
        .collect();
    let master_sidechains = with_eq(&project.master_inserts, &project.master_eq);
    // The lane a follower hears.
    let follows = |kind: Option<&ModKind>| match kind {
        Some(ModKind::Follower { source, .. }) => project
            .tracks
            .iter()
            .position(|o| o.id == *source)
            .map_or(usize::MAX, |j| tracks[j].0),
        _ => usize::MAX,
    };
    for (i, (t, &(slot, _))) in project.tracks.iter().zip(tracks).enumerate() {
        let (mut clips, mut events, mut controls) = (Vec::new(), Vec::new(), Vec::new());
        // Its modulators as patterns' sounds hear them: shared ones as the stack's.
        let ahead = project.mods_of(i);
        // What patterns' automation rows set on the track's own parameters, pass by pass: it
        // takes the place of the track's automation while the region plays.
        let mut stepped: Vec<(Param, u64, u64, Vec<AutoPoint>)> = Vec::new();
        for (r, region) in t.regions.iter().enumerate().filter(|(_, r)| !r.mute) {
            // A looped region plays again from each multiple of its length, the last time cut
            // short where the loop stops.
            let until = match region.looped {
                true => project.loop_end((i, r)),
                false => region.start + region.length,
            };
            let mut start = region.start;
            while start < until {
                // ponytail: a copy of the region per pass; fine for loops, not for thousands.
                let pass = Region {
                    start,
                    length: region.length.min(until - start),
                    ..region.clone()
                };
                play_region(
                    project,
                    rate,
                    &pass,
                    &ahead,
                    files,
                    (&mut events, &mut controls),
                    &mut clips,
                    &mut problems,
                );
                if let Content::Pattern(p) = &pass.content {
                    let set = crate::song::project::pattern_automation(p, pass.length, pass.start);
                    for (target, points) in set {
                        let Some(param) = target.param() else {
                            continue;
                        };
                        let points = points
                            .into_iter()
                            .map(|pt| AutoPoint {
                                tick: pass.start + pt.tick,
                                value: param.from_unit(pt.value),
                                ..pt
                            })
                            .collect();
                        stepped.push((param, pass.start, pass.start + pass.length, points));
                    }
                }
                start += region.length.max(1);
            }
        }
        // A loop cell plays its region from 0, every note ending by the loop's end.
        let cells = t
            .cells
            .iter()
            .map(|cell| {
                let region = Region {
                    start: 0,
                    ..cell.region.clone()
                };
                let (mut events, mut clips, mut controls) = (Vec::new(), Vec::new(), Vec::new());
                play_region(
                    project,
                    rate,
                    &region,
                    &ahead,
                    files,
                    (&mut events, &mut controls),
                    &mut clips,
                    &mut problems,
                );
                let length = project.samples(region.length, rate).max(1);
                for e in &mut events {
                    e.at = e.at.min(length - 1);
                }
                events.sort_by_key(|e| (e.at, e.velocity > 0.0));
                controls.retain(|c| c.at < length);
                controls.sort_by_key(|c| c.at);
                CellPlan {
                    events,
                    controls,
                    clips,
                    length,
                }
            })
            .collect();
        // Offs first at the same sample, so back-to-back notes of one pitch both sound.
        events.sort_by_key(|e| (e.at, e.velocity > 0.0));
        controls.sort_by_key(|c| c.at);
        clips.sort_by_key(|c| c.start);
        let in_samples = |points: &[AutoPoint]| {
            let at = |p: &AutoPoint| project.samples(p.tick, rate);
            points
                .iter()
                .map(|p| AutoPoint { tick: at(p), ..*p })
                .collect()
        };
        // With its automation off, a track follows its faders and knobs (and its patterns'
        // automation rows).
        let mut own = None;
        if !stepped.is_empty() {
            let mut a = match t.automation.mode {
                AutoMode::Off => crate::song::project::Automation::default(),
                _ => t.automation.clone(),
            };
            for (param, from, to, points) in stepped {
                a.replace(param, from, to, points);
            }
            own = Some(a);
        }
        let auto = match &own {
            Some(a) => Some(a),
            None => (t.automation.mode != AutoMode::Off).then_some(&t.automation),
        };
        plans.push(Plan {
            slot,
            events,
            controls,
            sidechains: track_sidechains[i].clone(),
            feeds: false,
            clips,
            audible: if t.mute || (any_solo && !t.solo) {
                0.0
            } else {
                1.0
            },
            volume_db: t.volume_db,
            pan: t.pan,
            pan_mode: t.pan_mode,
            pan_law: project.pan_law,
            spread: t.spread.unwrap_or(1.0),
            place: Place::of(t, t.pan),
            mods: t
                .modulators
                .iter()
                .take(MAX_MODS)
                .map(|m| Modulator {
                    kind: m.kind.clone(),
                    // A send by its place among those that play.
                    routes: m
                        .routes
                        .iter()
                        .filter_map(|r| match r.target {
                            ModTarget::Send(j) => {
                                let playing = |s: &&crate::song::project::AuxSend| {
                                    project.auxes.iter().any(|a| a.bus == s.bus)
                                };
                                let k = t.sends.iter().take(j).filter(playing).count();
                                t.sends.get(j).filter(playing).map(|_| ModRoute {
                                    target: ModTarget::Send(k),
                                    ..*r
                                })
                            }
                            _ => Some(*r),
                        })
                        .collect(),
                })
                .collect(),
            follow: std::array::from_fn(|k| follows(t.modulators.get(k).map(|m| &m.kind))),
            param_base: {
                let refs = match &t.instrument {
                    Some(Instrument::Plugin(r)) => vec![(Which::Instrument, r)],
                    _ => vec![],
                };
                let inserts = t
                    .inserts
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (Which::Insert(i), r));
                refs.into_iter()
                    .chain(inserts)
                    .flat_map(|(w, r)| r.params.iter().map(move |(&id, &v)| (w, id, v as f32)))
                    .collect()
            },
            volume: auto.map_or_else(Vec::new, |a| in_samples(&a.volume)),
            pans: auto.map_or_else(Vec::new, |a| in_samples(&a.pan)),
            mutes: auto.map_or_else(Vec::new, |a| in_samples(&a.mute)),
            send_levels: t
                .sends
                .iter()
                .filter(|s| project.auxes.iter().any(|a| a.bus == s.bus))
                .map(|s| auto.map_or_else(Vec::new, |a| in_samples(a.points(Param::Send(s.bus)))))
                .collect(),
            params: auto
                .map(|a| &a.plugins[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.plugin, a.param, in_samples(&a.points)))
                .collect(),
            eq_auto: auto
                .map(|a| &a.eq[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.band, a.field, in_samples(&a.points)))
                .collect(),
            mod_auto: auto
                .map(|a| &a.mods[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.modulator, a.field, in_samples(&a.points)))
                .collect(),
            cells,
            sends: t
                .sends
                .iter()
                .filter_map(|s| {
                    let aux = project.auxes.iter().position(|a| a.bus == s.bus)?;
                    Some((aux, db(s.level_db), s.pre))
                })
                .collect(),
            delay: pdc.tracks[i],
            output: t
                .output
                .and_then(|bus| project.auxes.iter().position(|a| a.bus == bus)),
            // Through an aux, the aux's own compensation lines it up.
            direct: if t.output.is_some() { 0 } else { pdc.direct },
            dsp: strip_dsp(t.eq.as_ref(), t.comp.as_ref(), t.sat.as_ref(), rate),
            sampler: match &t.instrument {
                Some(Instrument::Sampler(s)) => Some(Arc::new(s.clone())),
                _ => None,
            },
        });
    }
    let auxes = project
        .auxes
        .iter()
        .zip(auxes)
        .zip(&pdc.auxes)
        .zip(aux_sidechains)
        .map(|(((a, &(slot, _)), &delay), sidechains)| {
            let g = if a.mute { 0.0 } else { db(a.volume_db) };
            let mods: Vec<ModKind> = match a.stack {
                true => a
                    .modulators
                    .iter()
                    .take(MAX_MODS)
                    .map(|m| m.kind.clone())
                    .collect(),
                false => Vec::new(),
            };
            // A shared modulator's settings as a track in it automates them.
            let mut mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)> = Vec::new();
            let members = project.tracks.iter().filter(|t| {
                !mods.is_empty() && t.output == Some(a.bus) && t.automation.mode != AutoMode::Off
            });
            for t in members {
                for m in t.automation.mods.iter().filter(|m| !m.points.is_empty()) {
                    let Some(ModKind::Shared { modulator: s }) =
                        t.modulators.get(m.modulator as usize).map(|m| &m.kind)
                    else {
                        continue;
                    };
                    if !mod_auto.iter().any(|(k, f, _)| k == s && *f == m.field) {
                        let points = m
                            .points
                            .iter()
                            .map(|p| AutoPoint {
                                tick: project.samples(p.tick, rate),
                                ..*p
                            })
                            .collect();
                        mod_auto.push((*s, m.field, points));
                    }
                }
            }
            AuxPlan {
                slot,
                follow: std::array::from_fn(|k| follows(mods.get(k))),
                mods,
                mod_auto,
                gain: balance(g, a.pan),
                dsp: strip_dsp(a.eq.as_ref(), a.comp.as_ref(), a.sat.as_ref(), rate),
                delay,
                sidechains,
            }
        })
        .collect::<Vec<_>>();
    for (plan, &f) in plans.iter_mut().zip(&feeds) {
        plan.feeds = f;
    }
    let order = sidechain_order(&track_sidechains);
    let arrangement = Arrangement {
        tempo: project.tempo,
        master: db(project.master_volume_db),
        sidechains: feeds
            .iter()
            .map(|&f| match f {
                true => [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
                false => [Vec::new(), Vec::new()],
            })
            .collect(),
        master_sidechains,
        order,
        plans,
        stack_mods: vec![[0.0; MAX_MODS]; auxes.len()],
        buses: (0..auxes.len())
            .map(|_| [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]])
            .collect(),
        auxes,
        // The master's EQ holds back everything heard: the engine reads that much further ahead.
        lookahead: pdc.lookahead + eq_latency(&project.master_eq) as u64,
        master_strip: strip_dsp(
            project.master_eq.as_ref(),
            project.master_comp.as_ref(),
            project.master_sat.as_ref(),
            rate,
        ),
        cycle: project
            .cycle
            .filter(|c| !c.off && c.end > c.start)
            .map(|c| (project.samples(c.start, rate), project.samples(c.end, rate))),
        master_lane: master,
        beat: project.beat() as f64 / crate::song::project::PPQ as f64,
        beats_per_bar: project.beats_per_bar(),
    };
    files.end();
    (arrangement, problems)
}

/// The order to work tracks out in, given the track each one's inserts hear as a sidechain:
/// every source before what hears it. Sources in a loop keep their places, a block late.
pub(crate) fn sidechain_order(hears: &[Vec<Option<usize>>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(hears.len());
    let mut state = vec![0u8; hears.len()]; // 0 unseen, 1 on the way, 2 placed
    fn visit(t: usize, hears: &[Vec<Option<usize>>], state: &mut [u8], order: &mut Vec<usize>) {
        if state[t] != 0 {
            return;
        }
        state[t] = 1;
        for &s in hears[t].iter().flatten() {
            if s != t {
                visit(s, hears, state, order);
            }
        }
        state[t] = 2;
        order.push(t);
    }
    for t in 0..hears.len() {
        visit(t, hears, &mut state, &mut order);
    }
    order
}

/// Adds the note events, controller moves and audio clips a region plays, in samples; a pattern's
/// steps as the track's modulators move its sounds.
#[allow(clippy::too_many_arguments)]
pub(crate) fn play_region(
    project: &Project,
    rate: f64,
    region: &Region,
    mods: &[Modulator],
    files: &mut AudioFiles,
    (events, controls): (&mut Vec<Event>, &mut Vec<ControlEvent>),
    clips: &mut Vec<Clip>,
    problems: &mut Vec<String>,
) {
    // Plays `content`, which begins `at` ticks into the region, from `from` to `to` (ticks
    // into the region).
    let mut emit = |content: &Content, at: i64, from: u64, to: u64| match content {
        // Notes starting in their part of the region play, for their full length.
        Content::Midi { .. } | Content::Pattern { .. } => {
            let notes = match content {
                // A pattern as it plays: its lanes, and the modulators on its sounds.
                Content::Pattern(p) => {
                    std::borrow::Cow::Owned(crate::song::project::pattern_played(
                        p,
                        (to as i64 - at).max(0) as u64,
                        (region.start as i64 + at).max(0) as u64,
                        mods,
                        project.tempo,
                    ))
                }
                _ => content.notes((to as i64 - at).max(0) as u64).unwrap(),
            };
            for n in notes
                .iter()
                .map(|n| region.played(n))
                .filter(|n| (from as i64..to as i64).contains(&(at + n.start)))
            {
                let on = region.start + (at + n.start) as u64;
                let velocity = n.velocity as f32 / 127.0;
                let (pitch, start) = (n.pitch, project.samples(on, rate));
                events.push(Event {
                    at: start,
                    pitch,
                    velocity,
                });
                let off = project.samples(on + n.length, rate);
                events.push(Event {
                    at: off,
                    pitch,
                    velocity: 0.0,
                });
            }
            // A pattern's automation rows on MIDI controllers, pitch bend and pressure: a move
            // at each step that plays, or a slide to the next in 64ths.
            if let Content::Pattern(p) = content {
                use crate::song::project::RowTarget;
                let span = (to as i64 - at).max(0) as u64;
                let begin = (region.start as i64 + at).max(0) as u64;
                for (target, points) in crate::song::project::pattern_automation(p, span, begin) {
                    let kind = |v: f32| match target {
                        RowTarget::Cc(number) => Some(ControlKind::Cc { number, value: v }),
                        RowTarget::PitchBend => Some(ControlKind::PitchBend(v * 2.0 - 1.0)),
                        RowTarget::Pressure => Some(ControlKind::Aftertouch(v)),
                        _ => None,
                    };
                    for (k, pt) in points.iter().enumerate() {
                        let mut moves = vec![(pt.tick, pt.value)];
                        if let Some(next) = points.get(k + 1).filter(|_| !pt.step) {
                            let n =
                                ((next.tick - pt.tick) / (crate::song::project::PPQ / 16)).max(1);
                            moves.extend((1..n).map(|s| {
                                let f = s as f32 / n as f32;
                                (
                                    pt.tick + s * (next.tick - pt.tick) / n,
                                    pt.value + (next.value - pt.value) * f,
                                )
                            }));
                        }
                        for (tick, v) in moves {
                            let t = at + tick as i64;
                            if (from as i64..to as i64).contains(&t)
                                && let Some(kind) = kind(v)
                            {
                                controls.push(ControlEvent {
                                    at: project.samples(region.start + t as u64, rate),
                                    kind,
                                });
                            }
                        }
                    }
                }
            }
            // Controllers as they stand where this part starts, then their moves; a note's own
            // moves follow it when the region transposes it.
            if let Content::Midi {
                controls: points, ..
            } = content
            {
                let moved = points.iter().map(|c| crate::song::project::ControlPoint {
                    start: at + c.start,
                    ..*c
                });
                for mut c in chase(moved, from as i64, to as i64) {
                    if let Some(p) = c.kind.pitch_mut() {
                        *p = (*p as i16 + region.transpose as i16).clamp(0, 127) as u8;
                    }
                    let tick = region.start + c.start as u64;
                    controls.push(ControlEvent {
                        at: project.samples(tick, rate),
                        kind: c.kind.to_control(),
                    });
                }
            }
        }
        Content::Audio {
            file,
            offset,
            stretch,
            flex,
            formants,
            warps,
            ..
        } => {
            let begin = (from as i64).max(at);
            if begin >= to as i64 {
                return;
            }
            // Warps are at the project's rate; the engine may run at another.
            let scale = rate / project.sample_rate as f64;
            let warps: Vec<(usize, usize)> = warps
                .iter()
                .map(|w| {
                    (
                        (w[0] as f64 * scale) as usize,
                        (w[1] as f64 * scale) as usize,
                    )
                })
                .collect();
            // In the stretched audio, the offset is stretched too (and warped, if it's warped).
            let offset = match (warps.is_empty(), files.frames(file)) {
                (false, Some(n)) => {
                    let len = (n as f64 * stretch).round() as usize;
                    stretch::Map::new(n, len, &warps).at(*offset as usize) as u64
                }
                _ => (*offset as f64 * stretch) as u64,
            };
            let skip = project.samples((begin - at) as u64, rate);
            let start = project.samples(region.start + begin as u64, rate);
            let end = project.samples(region.start + to, rate);
            let fit = Fit {
                ratio: *stretch,
                semitones: region.transpose as f64,
                flex: *flex,
                formants: *formants,
                warps,
            };
            match files.stretched(file, fit) {
                Ok(data) => clips.push(Clip {
                    start,
                    len: end - start,
                    offset: offset + skip,
                    data,
                    fades: [
                        project.samples(region.start, rate),
                        project.samples(region.fade_in.min(region.length), rate),
                        project.samples(region.start + region.length, rate),
                        project.samples(region.fade_out.min(region.length), rate),
                    ],
                }),
                Err(e) => problems.push(e),
            }
        }
        Content::Takes { .. } => {}
    };
    match &region.content {
        Content::Takes { takes, comp } => {
            for (from, to, take) in comp_segments(comp, takes.len(), region.length) {
                emit(&takes[take].content, takes[take].start, from, to);
            }
        }
        content => emit(content, 0, 0, region.length),
    }
}
