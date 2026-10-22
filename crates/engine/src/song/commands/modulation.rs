//! Commands on modulators: added, set, shared, removed, and what they move.

use super::*;

/// A follower in a command names what it follows by the track's place: its id, here.
pub(crate) fn follower_source(
    p: &mut Project,
    m: &mut crate::dsp::modulate::Modulator,
) -> Result<(), String> {
    if let ModKind::Follower { source, .. } = &mut m.kind {
        *source = track(p, *source as usize)
            .map_err(|e| format!("a follower's source is a track's place: {e}"))?
            .id;
    }
    Ok(())
}

pub(crate) fn modulator_at(
    p: &mut Project,
    t: usize,
    index: usize,
) -> Result<&mut crate::dsp::modulate::Modulator, String> {
    let track = track(p, t)?;
    let n = track.modulators.len();
    track
        .modulators
        .get_mut(index)
        .ok_or_else(|| format!("no modulator {index}: track {t} has {n}"))
}

/// The modulation commands.
pub(super) fn run(p: &mut Project, _dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::AddModulator {
            track: t,
            mut modulator,
        } => {
            track(p, t)?;
            follower_source(p, &mut modulator)?;
            if let ModKind::Shared { modulator: s } = modulator.kind {
                let Some(stack) = p.stack_of(t) else {
                    return Err(format!("track {t} isn't in a stack"));
                };
                if s as usize >= stack.modulators.len() {
                    return Err(format!(
                        "{} shares {} modulators: no {s}",
                        stack.name,
                        stack.modulators.len()
                    ));
                }
            }
            let t = track(p, t)?;
            if t.modulators.len() >= crate::dsp::modulate::MAX_MODS {
                return Err(format!(
                    "a track has {} modulators at most",
                    crate::dsp::modulate::MAX_MODS
                ));
            }
            t.modulators.push(modulator);
        }
        Command::SetModulator {
            track: t,
            index,
            mut modulator,
        } => {
            follower_source(p, &mut modulator)?;
            let m = modulator_at(p, t, index)?;
            // One its stack shares, given settings: the stack's, for every track in it.
            if matches!(m.kind, ModKind::Shared { .. })
                && !matches!(modulator.kind, ModKind::Shared { .. })
            {
                if !modulator.routes.is_empty() {
                    m.routes = modulator.routes;
                }
                *p.mod_kind_mut(t, index).ok_or_else(|| {
                    format!("track {t}'s modulator {index} is one its stack no longer shares")
                })? = modulator.kind;
                return Ok(Value::Null);
            }
            let routes = std::mem::take(&mut m.routes);
            *m = modulator;
            if m.routes.is_empty() {
                m.routes = routes;
            }
        }
        Command::ShareModulator {
            aux: a,
            mut modulator,
        } => {
            follower_source(p, &mut modulator)?;
            let stack = aux(p, a)?;
            if !stack.stack {
                return Err(format!("aux {a} ({}) isn't a stack", stack.name));
            }
            if matches!(modulator.kind, ModKind::Shared { .. }) {
                return Err("a stack shares an lfo, steps, a follower, random or a macro".into());
            }
            let bus = stack.bus;
            let index = p.add_shared(bus, modulator.kind).ok_or_else(|| {
                format!(
                    "a stack shares {} modulators at most",
                    crate::dsp::modulate::MAX_MODS
                )
            })?;
            return Ok(json!({ "index": index }));
        }
        Command::UnshareModulator { aux: a, index } => {
            let stack = aux(p, a)?;
            let (bus, n) = (stack.bus, stack.modulators.len());
            if index >= n {
                return Err(format!("no shared modulator {index}: aux {a} shares {n}"));
            }
            p.remove_shared(bus, index);
        }
        Command::RemoveModulator { track: t, index } => {
            modulator_at(p, t, index)?;
            p.tracks[t].modulators.remove(index);
            p.tracks[t].automation.modulator_removed(index);
        }
        Command::Modulate {
            track: t,
            modulator,
            target,
            depth,
        } => {
            let range = crate::dsp::modulate::depth_range(target);
            let depth = finite("depth", depth)?.clamp(-range, range);
            let m = modulator_at(p, t, modulator)?;
            m.routes.retain(|r| r.target != target);
            if depth != 0.0 {
                m.routes
                    .push(crate::dsp::modulate::ModRoute { target, depth });
            }
        }
        _ => unreachable!("not one of the modulation commands"),
    }
    Ok(json!({}))
}
