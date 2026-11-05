//! Commands on tracks: added, deleted, set, their instruments and inserts, inputs, outputs and stacks.

use super::*;

/// The tracks commands.
pub(super) fn run(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::AddTrack {
            name,
            instrument,
            index,
        } => {
            let index = index.unwrap_or(p.tracks.len()).min(p.tracks.len());
            let mut instrument = instrument;
            if let Some(i) = &mut instrument {
                import_instrument(dir, i)?;
            }
            p.add_track(
                index,
                Track {
                    name,
                    instrument,
                    ..Default::default()
                },
            );
            return Ok(json!({ "track": index }));
        }
        Command::ImportMidi {
            file,
            at,
            index,
            transpose,
            fit,
        } => {
            let mut f = crate::song::midi_file::open(&file)?;
            let name = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let study = crate::samples::midi::study(&f, &name);
            crate::samples::midi::mark_drums(&mut f, &study);
            let shift = match (fit, p.song_key(), study.key.as_deref().and_then(Key::parse)) {
                (true, Some(to), Some(from)) => crate::samples::midi::transpose_to(&from, &to),
                _ => transpose.unwrap_or(0),
            };
            let made = p.import_midi(&f, at, index, shift);
            return Ok(json!({
                "tracks": made,
                "parts": f.parts.iter().map(|x| &x.name).collect::<Vec<_>>(),
                "transpose": shift,
                "key": study.key,
                "summary": study.summary,
            }));
        }
        Command::DeleteTrack { track: t } => {
            track(p, t)?;
            p.tracks.remove(t);
        }
        Command::SetTrack {
            track: t,
            name,
            color,
            icon,
            volume_db,
            pan,
            pan_mode,
            spread,
            elevation,
            distance,
            mute,
            solo,
            midi_fx,
        } => {
            let t = track(p, t)?;
            if let Some(v) = midi_fx {
                if v.len() > crate::dsp::midi_fx::MAX_MIDI_FX {
                    return Err(format!(
                        "midi_fx: {} is more than a track runs ({})",
                        v.len(),
                        crate::dsp::midi_fx::MAX_MIDI_FX
                    ));
                }
                t.midi_fx = v;
            }
            if let Some(v) = pan_mode {
                t.pan_mode = v;
            }
            if let Some(v) = spread {
                t.spread = Some(finite("spread", v)?.clamp(-1.0, 1.0));
            }
            if let Some(v) = elevation {
                t.elevation = Some(finite("elevation", v)?.clamp(-90.0, 90.0));
            }
            if let Some(v) = distance {
                t.distance = Some(finite("distance", v)?.clamp(Place::NEAREST, Place::FARTHEST));
            }
            if let Some(v) = name {
                t.name = v;
            }
            if let Some(v) = color {
                t.color = match v.as_str() {
                    "auto" => None,
                    c if crate::music::look::parse_color(c).is_some() => Some(c.to_lowercase()),
                    c => return Err(format!("color: {c:?} isn't \"#rrggbb\" or \"auto\"")),
                };
            }
            if let Some(v) = icon {
                t.icon = match v.as_str() {
                    "auto" => None,
                    i if crate::music::look::icon(i).is_some() => Some(i.to_string()),
                    i => {
                        let ids: Vec<&str> =
                            crate::music::look::ICONS.iter().map(|i| i.id).collect();
                        return Err(format!("icon: no icon {i:?}; there are {}", ids.join(", ")));
                    }
                };
            }
            if let Some(v) = volume_db {
                t.volume_db = finite("volume_db", v)?;
            }
            if let Some(v) = pan {
                t.pan = finite("pan", v)?.clamp(-1.0, 1.0);
            }
            if let Some(v) = mute {
                t.mute = v;
            }
            if let Some(v) = solo {
                t.solo = v;
            }
        }
        Command::SetInstrument {
            track: t,
            instrument,
        } => {
            let mut instrument = instrument;
            import_instrument(dir, &mut instrument)?;
            let t = track(p, t)?;
            if t.regions
                .iter()
                .any(|r| matches!(r.content, Content::Audio { .. }))
            {
                return Err("an audio track with audio regions can't take an instrument".into());
            }
            t.set_instrument(instrument);
        }
        Command::SetSynth {
            track: t,
            patch,
            settings,
        } => {
            use crate::song::project::{POLY_PATCHES, poly_patch};
            let tr = track(p, t)?;
            if tr
                .regions
                .iter()
                .any(|r| matches!(r.content, Content::Audio { .. }))
            {
                return Err("an audio track with audio regions can't take an instrument".into());
            }
            let s = match (&patch, &tr.instrument) {
                (Some(name), _) => poly_patch(name).ok_or_else(|| {
                    let names: Vec<&str> = POLY_PATCHES.iter().map(|p| p.0).collect();
                    format!("no patch {name}: there are {}", names.join(", "))
                })?,
                (None, Some(Instrument::Poly(s))) => (**s).clone(),
                (None, _) => poly_patch("Init").unwrap_or_default(),
            };
            let s = match settings {
                Some(v) => {
                    let mut was = serde_json::to_value(&s).map_err(|e| e.to_string())?;
                    mixing::overlay(&mut was, v);
                    serde_json::from_value(was).map_err(|e| format!("the synth: {e}"))?
                }
                None => s,
            };
            track(p, t)?.set_instrument(Instrument::Poly(Box::new(s)));
        }
        Command::SetInsert {
            channel,
            index,
            plugin,
        } => {
            chain(p, channel)?.set_insert(index, plugin)?;
        }
        Command::MoveEffect { channel, from, to } => {
            chain(p, channel)?.move_fx(from, to)?;
        }
        Command::SetInput { track: t, input } => {
            let tr = track(p, t)?;
            if tr.instrument.is_some() {
                return Err(format!(
                    "track {t} is an instrument track: inputs are for audio tracks"
                ));
            }
            tr.input = input;
        }
        Command::SetOutput { track: t, bus } => {
            if let Some(b) = bus
                && !p.auxes.iter().any(|a| a.bus == b)
            {
                return Err(format!("no aux on bus {b}"));
            }
            track(p, t)?.output = bus;
        }
        Command::AddStack { tracks, name } => {
            if tracks.is_empty() {
                return Err("a stack needs tracks".into());
            }
            for &t in &tracks {
                track(p, t)?;
            }
            let (bus, moved) = p.add_stack(&tracks);
            if let Some(name) = name {
                p.auxes.last_mut().unwrap().name = name;
            }
            let tracks: Vec<usize> = tracks.iter().map(|&t| moved[t]).collect();
            return Ok(json!({ "bus": bus, "tracks": tracks, "moved": moved }));
        }
        Command::FlattenStack { aux: a } => {
            let stack = aux(p, a)?;
            if !stack.stack {
                return Err(format!("aux {a} ({}) isn't a stack", stack.name));
            }
            let bus = stack.bus;
            p.flatten_stack(bus);
        }
        Command::AddKit {
            name,
            pads,
            index,
            at,
            pattern,
        } => {
            if pads.is_empty() {
                return Err("a kit needs a pad at least".into());
            }
            let mut used = Vec::new();
            let mut made = Vec::new();
            for k in &pads {
                let file = files::import(dir, &k.file)?;
                let name = k.name.clone().unwrap_or_else(|| {
                    file.file_stem()
                        .map_or("Pad".into(), |n| n.to_string_lossy().into_owned())
                });
                let note = k
                    .note
                    .or_else(|| drum_note(&name).filter(|n| !used.contains(n)))
                    .unwrap_or_else(|| (36..128).find(|n| !used.contains(n)).unwrap_or(127));
                used.push(note);
                // Hats choke each other unless told otherwise.
                let choke = k
                    .choke
                    .or_else(|| matches!(note, 42 | 44 | 46).then_some(1));
                made.push(crate::song::project::NewPad {
                    sampler: crate::song::project::Sampler {
                        zones: vec![crate::song::project::Zone {
                            file,
                            from: k.file.is_absolute().then(|| k.file.clone()),
                            name: name.clone(),
                            note,
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    name,
                    note,
                    choke,
                    steps: k.steps.clone(),
                });
            }
            let name = name.unwrap_or_else(|| "Kit".into());
            let index = index.unwrap_or(p.tracks.len());
            let kit = p.add_kit(&name, made, index, at, pattern);
            return Ok(
                json!({ "kit": kit.kit, "pads": kit.pads, "aux": p.auxes.iter().position(|a| a.bus == kit.bus) }),
            );
        }
        Command::SplitKit { track: t } => {
            if track(p, t)?.instrument != Some(Instrument::Kit) {
                return Err(format!("track {t} isn't a kit track"));
            }
            return Ok(json!({ "regions": p.split_kit(t) }));
        }
        _ => unreachable!("not one of the tracks commands"),
    }
    Ok(json!({}))
}

/// The General MIDI drum note a sound's name says it is.
pub fn drum_note(name: &str) -> Option<u8> {
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| n.contains(w));
    Some(match () {
        _ if has(&["open hat", "open hh", "openhat", "ohh", "oh "]) => 46,
        _ if has(&["closed hat", "hat", "hh", "hihat", "hi-hat"]) => 42,
        _ if has(&["808", "sub"]) => 35,
        _ if has(&["kick", "bd", "bass drum"]) => 36,
        _ if has(&["rim"]) => 37,
        _ if has(&["snare", "sd"]) => 38,
        _ if has(&["clap", "cp"]) => 39,
        _ if has(&["tom"]) => 45,
        _ if has(&["crash"]) => 49,
        _ if has(&["ride"]) => 51,
        _ if has(&["shaker", "maraca"]) => 70,
        _ if has(&["tamb"]) => 54,
        _ if has(&["cowbell"]) => 56,
        _ if has(&["perc", "conga", "bongo"]) => 63,
        _ => return None,
    })
}
