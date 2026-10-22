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
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: patterns go on instrument tracks"
                ));
            }
            let content = Content::Pattern {
                step: step.unwrap_or(PPQ / 4).max(1),
                rows,
                swing: finite("swing", swing.unwrap_or(50.0))?.clamp(50.0, 75.0),
            };
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
        } => {
            region_at(p, t, r)?;
            match &mut p.tracks[t].regions[r].content {
                Content::Pattern { rows, swing, .. } => {
                    if let Some(new) = new {
                        *rows = new;
                    }
                    if let Some(v) = new_swing {
                        *swing = finite("swing", v)?.clamp(50.0, 75.0);
                    }
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
            let length = length.unwrap_or_else(|| p.ticks(frames.saturating_sub(offset)).max(1));
            let content = Content::Audio {
                file,
                offset,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
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
            if let Some(end) = end {
                p.trim_end((t, r), end);
            }
            if let Some(start) = start {
                p.trim_start((t, r), start);
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
        Command::FlexRegion {
            track: t,
            region: r,
            length,
        } => {
            region_at(p, t, r)?;
            p.flex((t, r), length)?;
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
            flex,
            follow,
            formants,
            warps,
        } => {
            region_at(p, t, r)?;
            let region = &mut p.tracks[t].regions[r];
            if flex.is_some() || follow.is_some() || formants.is_some() || warps.is_some() {
                let Content::Audio {
                    flex: f,
                    follow: w,
                    formants: k,
                    warps: m,
                    ..
                } = &mut region.content
                else {
                    return Err("flex, follow, formants and warps are for audio regions".into());
                };
                *f = flex.unwrap_or(*f);
                *w = follow.unwrap_or(*w);
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
