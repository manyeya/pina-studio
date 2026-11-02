//! Commands on regions: MIDI, patterns and audio added, moved, trimmed, split, flexed, comped, aliased; notes edited.

use super::*;

/// The regions commands.
pub(super) fn run(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::AddMidiRegion {
            track: t,
            start,
            length,
            notes,
            mut controls,
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: MIDI goes on instrument tracks"
                ));
            }
            controls.sort_by_key(|c| c.start);
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content: Content::Midi { notes, controls },
                ..Default::default()
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
        }
        Command::SetControls {
            track: t,
            region: r,
            mut controls,
        } => {
            region_at(p, t, r)?;
            controls.sort_by_key(|c| c.start);
            let only = "only MIDI regions have controller moves: that's audio, a pattern or takes";
            *p.controls_mut((t, r)).ok_or(only)? = controls;
        }
        Command::SetNotes {
            track: t,
            region: r,
            notes,
        } => {
            region_at(p, t, r)?;
            let only = "only MIDI regions have notes: that's audio, or a take folder";
            *p.notes_mut((t, r)).ok_or(only)? = notes;
        }
        Command::AddPatternRegion {
            track: t,
            start,
            length,
            step,
            rows,
            swing,
            steps,
            mode,
            legato,
            swing_eighths,
            key,
            scale,
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: patterns go on instrument tracks"
                ));
            }
            let mut pattern = crate::song::project::Pattern::new(
                step.unwrap_or(PPQ / 4).max(1),
                rows,
                finite("swing", swing.unwrap_or(50.0))?.clamp(50.0, 75.0),
            );
            settle(
                &mut pattern,
                (steps, mode, legato, swing_eighths, key, scale),
            )?;
            let content = Content::Pattern(pattern);
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content,
                ..Default::default()
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
        }
        Command::SetPattern {
            track: t,
            region: r,
            rows: new,
            swing: new_swing,
            step,
            steps,
            mode,
            legato,
            swing_eighths,
            key,
            scale,
        } => {
            region_at(p, t, r)?;
            match &mut p.tracks[t].regions[r].content {
                Content::Pattern(p) => {
                    if let Some(new) = new {
                        // As long as the rows given (unless `steps` says otherwise).
                        p.rows = new;
                        p.length = 0;
                    }
                    if let Some(v) = new_swing {
                        p.swing = finite("swing", v)?.clamp(50.0, 75.0);
                    }
                    if let Some(step) = step {
                        p.step = step.max(1);
                    }
                    p.tidy();
                    settle(p, (steps, mode, legato, swing_eighths, key, scale))?;
                }
                _ => return Err("that isn't a pattern region".into()),
            }
        }
        Command::AddAudioRegion {
            track: t,
            file,
            start,
            offset,
            length,
        } => {
            if track(p, t)?.instrument.is_some() {
                return Err(format!(
                    "track {t} is an instrument track: audio goes on audio tracks"
                ));
            }
            let file = import(dir, &file)?;
            // How long it is: a WAV's header says; AIFF, FLAC and MP3 are read.
            let (frames, from) = match hound::WavReader::open(dir.join(&file)) {
                Ok(wav) => (wav.duration() as u64, wav.spec().sample_rate as f64),
                Err(_) => {
                    let (sides, rate) = crate::audio::engine::read_audio(&dir.join(&file))?;
                    (sides[0].len() as u64, rate)
                }
            };
            // Played at the project's rate, converted if it was recorded at another.
            let frames = (frames as f64 * p.sample_rate as f64 / from) as u64;
            let length =
                length.unwrap_or_else(|| p.ticks_for(start, frames.saturating_sub(offset)).max(1));
            let content = Content::Audio {
                file,
                offset,
                stretch: 1.0,
                stretch_mode: StretchMode::default(),
                follow: crate::song::project::Follow::Off,
                tempo: None,
                grid: None,
                formants: false,
                warps: Vec::new(),
            };
            let regions = &mut track(p, t)?.regions;
            regions.push(Region {
                start,
                length,
                content,
                ..Default::default()
            });
            return Ok(json!({ "region": [t, regions.len() - 1] }));
        }
        Command::MoveRegion {
            track: t,
            region: r,
            start,
            to_track,
        } => {
            region_at(p, t, r)?;
            let to = to_track.unwrap_or(t);
            track(p, to)?;
            let midi = matches!(p.region((t, r)).content, Content::Midi { .. });
            if p.tracks[to].instrument.is_some() != midi {
                return Err(format!("track {to} can't play that region"));
            }
            let at = p.move_region((t, r), to, start);
            return Ok(json!({ "region": [at.0, at.1] }));
        }
        Command::TrimRegion {
            track: t,
            region: r,
            start,
            end,
        } => {
            region_at(p, t, r)?;
            let looped = p.region((t, r)).looped;
            if let Some(end) = end {
                let region = p.region((t, r));
                let (from, pass_end) = (region.start, region.start + region.length);
                // A looped region trimmed out runs its loops further: one pass stays one pass,
                // or its notes would stop and the rest of it be silence.
                match looped && end > pass_end {
                    true => p.tracks[t].regions[r].loop_length = Some(end - from),
                    false => p.trim_end((t, r), end),
                }
            }
            if let Some(start) = start {
                p.trim_start((t, r), start);
            }
            if looped {
                return Ok(loops(p, (t, r)));
            }
        }
        Command::SplitRegion {
            track: t,
            region: r,
            at,
        } => {
            region_at(p, t, r)?;
            let second = p
                .split_region((t, r), at)
                .ok_or("that tick isn't inside the region")?;
            return Ok(json!({ "region": [second.0, second.1] }));
        }
        Command::SetCell {
            track: t,
            scene,
            region,
        } => {
            let tr = track(p, t)?;
            if let Some(r) = &region
                && r.content.is_midi() != tr.instrument.is_some()
            {
                return Err(format!("track {t} can't play that loop"));
            }
            tr.cells.retain(|c| c.scene != scene);
            if let Some(region) = region {
                tr.cells.push(Cell {
                    scene,
                    region: Region { start: 0, ..region },
                });
            }
        }
        Command::StretchRegion {
            track: t,
            region: r,
            length,
        } => {
            region_at(p, t, r)?;
            p.time_stretch((t, r), length)?;
        }
        Command::AnalyzeTempo {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            let Content::Audio { file, .. } = &p.region((t, r)).content else {
                return Err("that isn't an audio region".into());
            };
            let found = find_timing(p, dir, file)?
                .ok_or("no beat to follow in it: a one-shot, or nothing steady enough")?;
            let answer = json!({
                "tempo": (found.tempo * 100.0).round() / 100.0,
                "beats": found.grid.beats.len(),
                "meter": found.grid.meter,
                "loop": found.looped,
            });
            if let Content::Audio { tempo, grid, .. } = &mut p.tracks[t].regions[r].content {
                (*tempo, *grid) = (Some(found.tempo), Some(found.grid));
            }
            return Ok(answer);
        }
        Command::AdaptTempo {
            track: t,
            region: r,
            steady,
            align,
        } => {
            region_at(p, t, r)?;
            p.adapt_to((t, r), steady, align)?;
            return Ok(json!({ "tempo": p.tempo, "tempos": p.tempos.len() }));
        }
        Command::FitTempo {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            let c = p
                .cycle
                .filter(|c| c.end > c.start)
                .ok_or("set a cycle to fit it to")?;
            let bpm = p.fit_tempo((t, r), c.start, c.end)?;
            return Ok(json!({ "bpm": (bpm * 100.0).round() / 100.0 }));
        }
        Command::Comp {
            track: t,
            region: r,
            from,
            to,
            take,
        } => {
            region_at(p, t, r)?;
            p.comp((t, r), from, to, take)?;
        }
        Command::Flatten {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            let regions: Vec<[usize; 2]> =
                p.flatten((t, r)).into_iter().map(|(a, b)| [a, b]).collect();
            return Ok(json!({ "regions": regions }));
        }
        Command::DeleteRegion {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            p.tracks[t].regions.remove(r);
        }
        Command::SetRegion {
            track: t,
            region: r,
            name,
            looped,
            loop_length,
            fade_in,
            fade_out,
            transpose,
            velocity,
            quantize,
            stretch_mode,
            follow,
            tempo,
            formants,
            warps,
        } => {
            region_at(p, t, r)?;
            let looping = looped == Some(true) || loop_length.is_some_and(|l| l > 0);
            let audio = stretch_mode.is_some()
                || follow.is_some()
                || tempo.is_some()
                || formants.is_some()
                || warps.is_some();
            if audio && !matches!(p.tracks[t].regions[r].content, Content::Audio { .. }) {
                return Err(
                    "stretch_mode, follow, tempo, formants and warps are for audio regions".into(),
                );
            }
            if let Some(bpm) = tempo {
                p.set_audio_tempo((t, r), bpm);
            }
            if let Some(f) = follow {
                p.set_follow((t, r), f);
            }
            let region = &mut p.tracks[t].regions[r];
            if let Content::Audio {
                stretch_mode: f,
                formants: k,
                warps: m,
                ..
            } = &mut region.content
            {
                *f = stretch_mode.unwrap_or(*f);
                *k = formants.unwrap_or(*k);
                if let Some(mut v) = warps {
                    v.sort();
                    *m = v;
                }
            }
            if let Some(v) = name {
                region.name = v;
            }
            if let Some(v) = looped {
                region.looped = v;
            }
            if let Some(v) = loop_length {
                region.loop_length = (v > 0).then_some(v);
                region.looped |= v > 0;
                // Loops can't stop inside one pass: given that, the two were meant the other
                // way round, this is one pass and the region's length how far they run.
                if v > 0 && v < region.length {
                    (region.length, region.loop_length) = (v, Some(region.length));
                }
            }
            if let Some(v) = fade_in {
                region.fade_in = v.min(region.length);
            }
            if let Some(v) = fade_out {
                region.fade_out = v.min(region.length);
            }
            if let Some(v) = transpose {
                region.transpose = v.clamp(-96, 96);
            }
            if let Some(v) = velocity {
                region.velocity = v.clamp(-99, 99);
            }
            if let Some(v) = quantize {
                region.quantize = (v > 0).then_some(v);
            }
            if looping {
                return Ok(loops(p, (t, r)));
            }
        }
        Command::AddAlias {
            track: t,
            region: r,
            start,
            to_track,
        } => {
            region_at(p, t, r)?;
            let to = to_track.unwrap_or(t);
            track(p, to)?;
            let at = p.alias((t, r), to, start)?;
            return Ok(json!({ "region": [at.0, at.1] }));
        }
        Command::Unlink {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            p.unlink((t, r));
        }
        Command::EditNotes {
            track: t,
            region: r,
            edit,
            note,
        } => {
            region_at(p, t, r)?;
            let only = "only MIDI regions have notes: that's audio, a pattern or a take folder";
            let notes = p.notes_mut((t, r)).ok_or(only)?;
            if note.is_some_and(|i| i >= notes.len()) {
                return Err(format!("that region has {} notes", notes.len()));
            }
            edit.apply(notes, note.as_slice());
        }
        _ => unreachable!("not one of the regions commands"),
    }
    Ok(json!({}))
}

/// A pattern's settings from a command, each as given (or left).
type Settings = (
    Option<usize>,
    Option<crate::song::project::PlayMode>,
    Option<bool>,
    Option<bool>,
    Option<u8>,
    Option<String>,
);

fn settle(p: &mut crate::song::project::Pattern, s: Settings) -> Result<(), String> {
    let (steps, mode, legato, eighths, key, scale) = s;
    if let Some(n) = steps {
        if !(1..=crate::song::project::MAX_STEPS).contains(&n) {
            return Err(format!("steps {n}: a pattern has 1 to 64"));
        }
        p.set_length(n);
    }
    if let Some(m) = mode {
        p.mode = m;
    }
    if let Some(l) = legato {
        p.legato = l;
    }
    if let Some(e) = eighths {
        p.swing_eighths = e;
    }
    if let Some(k) = key {
        p.key = k % 12;
    }
    if let Some(s) = scale {
        p.scale = match s.as_str() {
            "chromatic" | "" => None,
            name => Some(
                serde_json::from_value(serde_json::Value::String(name.into())).map_err(|_| {
                    format!("{name:?} isn't a scale: major, minor, dorian, blues… or chromatic")
                })?,
            ),
        };
    }
    p.tidy();
    Ok(())
}

/// Listens to a file in the project for its timing (its frames at the project's rate).
pub(crate) fn find_timing(
    p: &Project,
    dir: &Path,
    file: &Path,
) -> Result<Option<crate::study::timing::Found>, String> {
    let (channels, rate) = crate::audio::engine::read_audio(&dir.join(file))?;
    let name = file
        .file_stem()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    Ok(crate::study::timing::find(
        &channels,
        rate,
        p.sample_rate as f64,
        &name,
    ))
}

/// How far a region's loops run, and why not if they don't.
fn loops(p: &Project, at: (usize, usize)) -> Value {
    let (r, to, bar) = (p.region(at), p.loop_end(at), p.bar().max(1));
    let mut answer = json!({
        "loops_to": to,
        "bars": format!("{}-{}", r.start / bar + 1, to.div_ceil(bar)),
    });
    let last = r
        .content
        .notes(r.length)
        .and_then(|n| n.iter().map(|n| n.start.max(0) as u64 + n.length).max());
    if to <= r.start + r.length {
        answer["note"] = json!(
            "it doesn't repeat: `length` is one pass, and it plays once. Give loop_length, the \
             ticks from its start to where its loops stop, longer than its length; or \
             trim_region its end out to there"
        );
    } else if let Some(last) = last.filter(|l| l * 4 < r.length * 3) {
        answer["note"] = json!(format!(
            "each pass ends in silence: its notes stop at bar {} of its {} bars. Trim one pass \
             to its notes with set_region loop_length, or write the rest",
            last.div_ceil(bar),
            r.length.div_ceil(bar)
        ));
    }
    answer
}

/// The looped regions a command may put another region across: track, region, where they stop.
pub(super) fn loops_running(p: &Project, cmd: &Command) -> Vec<(usize, usize, u64)> {
    let (t, moved) = match *cmd {
        Command::AddMidiRegion { track, .. }
        | Command::AddPatternRegion { track, .. }
        | Command::AddAudioRegion { track, .. } => (track, None),
        Command::AddAlias {
            track, to_track, ..
        } => (to_track.unwrap_or(track), None),
        Command::MoveRegion {
            track,
            region,
            to_track,
            ..
        } => (
            to_track.unwrap_or(track),
            (to_track.unwrap_or(track) == track).then_some(region),
        ),
        _ => return Vec::new(),
    };
    let Some(track) = p.tracks.get(t) else {
        return Vec::new();
    };
    (0..track.regions.len())
        .filter(|&r| track.regions[r].looped && Some(r) != moved)
        .map(|r| (t, r, p.loop_end((t, r))))
        .collect()
}

/// A region dropped into a looped region's run stops it there: its loops carry on after it, in
/// time, as far as they went. shortcut: the rest is a copy, not an alias, unless it was one;
/// link them if editing one loop should edit them all.
pub(super) fn carry_loops(p: &mut Project, ran: &[(usize, usize, u64)]) {
    for &(t, i, was) in ran {
        let stop = p.loop_end((t, i));
        if stop >= was {
            continue;
        }
        let r = p.tracks[t].regions[i].clone();
        let Some(after) = p.tracks[t]
            .regions
            .iter()
            .filter(|o| o.start > r.start)
            .min_by_key(|o| o.start)
            .map(|o| o.start + o.length)
        else {
            continue;
        };
        p.tracks[t].regions[i].loop_length = Some(stop - r.start);
        if after >= was {
            continue;
        }
        // What's left of the loop it landed in, then whole loops again.
        let pass = r.length.max(1);
        let whole = r.start + (after - r.start).div_ceil(pass) * pass;
        let plain = Region {
            start: whole - pass,
            looped: false,
            loop_length: None,
            ..r.clone()
        };
        if let Some(part) = (whole > after)
            .then(|| crate::music::arrange::clip(p, &plain, after, whole.min(was)))
            .flatten()
        {
            p.tracks[t].regions.push(part);
        }
        if whole < was {
            p.tracks[t].regions.push(Region {
                start: whole,
                looped: true,
                loop_length: Some(was - whole),
                ..r
            });
        }
    }
}
