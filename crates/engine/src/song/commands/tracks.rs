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
        } => {
            let t = track(p, t)?;
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
        Command::SetInsert {
            channel,
            index,
            plugin,
        } => {
            let list = match channel {
                ChannelRef::Track(i) => {
                    return track(p, i)?.set_insert(index, plugin).map(|_| json!({}));
                }
                ChannelRef::Aux(i) => &mut aux(p, i)?.inserts,
                ChannelRef::Master => &mut p.master_inserts,
            };
            match plugin {
                Some(plugin) if index < list.len() => list[index] = plugin,
                Some(plugin) => list.push(plugin),
                None if index < list.len() => drop(list.remove(index)),
                None => return Err(format!("no insert {index} to remove")),
            }
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
        _ => unreachable!("not one of the tracks commands"),
    }
    Ok(json!({}))
}
