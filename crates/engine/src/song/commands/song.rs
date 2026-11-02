//! Commands on the song as a whole: tempo, time signature, master, key, cycle, markers, palette, reference, genre.

use super::*;

/// The song commands.
pub(super) fn run(p: &mut Project, _dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetTempo { bpm } => {
            if !(bpm.is_finite() && (20.0..=999.0).contains(&bpm)) {
                return Err("bpm must be between 20 and 999".into());
            }
            p.set_tempo(bpm);
        }
        Command::SetTempoAt { tick, bpm } => {
            if !(bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&bpm)) {
                return Err("bpm must be between 20 and 999".into());
            }
            p.set_tempo_at(tick, bpm);
        }
        Command::SetTempoMode { mode } => p.tempo_mode = mode,
        Command::DeleteTempo { tick } => {
            if !p.tempos.iter().any(|c| c.tick == tick) {
                return Err(format!("no tempo change at tick {tick}"));
            }
            p.delete_tempo(tick);
        }
        Command::SetMaster { volume_db, pan_law } => {
            if let Some(v) = volume_db {
                p.master_volume_db = finite("volume_db", v)?;
            }
            if let Some(law) = pan_law {
                p.pan_law = law;
            }
        }
        Command::SetTimeSignature { beats, unit } => {
            if !(1..=32).contains(&beats) || !BEAT_UNITS.contains(&unit) {
                return Err("a time signature is 1 to 32 beats of a 1, 2, 4, 8 or 16".into());
            }
            p.time_signature = [beats, unit];
        }
        Command::SetPalette {
            sounds,
            devices,
            only,
        } => {
            if let Some(sounds) = sounds {
                let mut seen = std::collections::HashSet::new();
                for s in &sounds {
                    if !seen.insert(&s.path) {
                        return Err(format!("{} is in the palette twice", s.path.display()));
                    }
                }
                p.palette.sounds = sounds;
            }
            if let Some(devices) = devices {
                for (i, d) in devices.iter().enumerate() {
                    if devices[..i].iter().any(|e| e.same(d)) {
                        return Err(format!("{} is in the palette twice", d.name));
                    }
                }
                p.palette.devices = devices;
            }
            if let Some(only) = only {
                p.palette.only = only;
            }
        }
        Command::AddMarker { tick, name } => {
            let at = p.markers.partition_point(|m| m.tick <= tick);
            p.markers.insert(
                at,
                Marker {
                    tick,
                    name,
                    ..Default::default()
                },
            );
            return Ok(json!({ "marker": at }));
        }
        Command::SetMarker { marker, tick, name } => {
            let n = p.markers.len();
            let m = p
                .markers
                .get_mut(marker)
                .ok_or_else(|| format!("no marker {marker}: there are {n}"))?;
            if let Some(name) = name {
                m.name = name;
            }
            if let Some(tick) = tick {
                m.tick = tick;
                p.markers.sort_by_key(|m| m.tick);
            }
        }
        Command::DeleteMarker { marker } => {
            if marker >= p.markers.len() {
                return Err(format!("no marker {marker}: there are {}", p.markers.len()));
            }
            p.markers.remove(marker);
        }
        Command::MoveSection { section, before } => p.move_section(section, before)?,
        Command::CopySection { section, before } => p.copy_section(section, before)?,
        Command::SwapSections { a, b } => {
            let n = p.sections().len();
            if a >= n || b >= n {
                return Err(format!("there are {n} sections"));
            }
            p.swap_sections(a, b)?
        }
        Command::DeleteSection { section } => p.delete_section(section)?,
        Command::ClearSection { section } => p.clear_section(section)?,
        Command::AddSection { name, bars } => {
            let tick = p.add_section(&name, bars.unwrap_or(8).clamp(1, 512));
            let section = p.markers.iter().position(|m| m.tick == tick).unwrap_or(0);
            return Ok(json!({ "section": section, "tick": tick }));
        }
        Command::InsertTime { tick, length } => p.insert_time(tick, length),
        Command::RemoveTime { from, to } => {
            if to <= from {
                return Err("to must be after from".into());
            }
            p.remove_time(from, to)
        }
        Command::SetKey { key, move_notes } => {
            let parsed = match key {
                None => None,
                Some(k) => Some(Key::parse(&k).ok_or_else(|| {
                    format!("{k:?} isn't a key: try \"A minor\" or \"Eb major\"")
                })?),
            };
            p.set_key(parsed, move_notes);
        }
        Command::SetCycle { start, end, on } => {
            if end <= start {
                return Err("a cycle's end must be after its start".into());
            }
            p.cycle = Some(Cycle {
                start,
                end,
                off: !on,
            });
        }
        Command::SetReference { reference } => p.reference = reference,
        Command::SetGenre { genre } => {
            // Any word for it; one of the person's playbooks by its id, if it names one.
            p.genre = genre
                .filter(|g| !g.trim().is_empty())
                .map(|g| crate::music::playbook::find(&g).map_or(g, |b| b.id));
        }
        _ => unreachable!("not one of the song commands"),
    }
    Ok(json!({}))
}
