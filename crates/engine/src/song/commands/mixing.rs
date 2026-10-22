//! Commands on the mix: sends and auxes, and the strip's EQ, compressor and saturator.

use super::*;

pub(crate) fn aux(p: &mut Project, i: usize) -> Result<&mut crate::song::project::Aux, String> {
    let n = p.auxes.len();
    p.auxes
        .get_mut(i)
        .ok_or_else(|| format!("no aux {i}: there are {n}"))
}

/// Sets a built-in effect from the fields given (a band's too), the rest staying as they were,
/// or as the effect's defaults if it wasn't there. `None` takes it out.
pub(crate) fn set_effect<T>(slot: &mut Option<T>, changes: Option<Value>) -> Result<(), String>
where
    T: Serialize + serde::de::DeserializeOwned + Default + Clone,
{
    let Some(changes) = changes else {
        *slot = None;
        return Ok(());
    };
    let current = slot.clone().unwrap_or_default();
    let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    overlay(&mut value, changes);
    *slot = Some(serde_json::from_value(value).map_err(|e| e.to_string())?);
    Ok(())
}

/// JSON changes laid over JSON: objects merge, anything else replaces.
pub(crate) fn overlay(base: &mut Value, changes: Value) {
    match (base, changes) {
        (Value::Object(base), Value::Object(changes)) => {
            for (k, v) in changes {
                overlay(base.entry(k).or_insert(Value::Null), v);
            }
        }
        (base, changes) => *base = changes,
    }
}

/// An EQ's bands in their ranges: each says which is out and what it can be.
pub(crate) fn check_eq(eq: &Eq) -> Result<(), String> {
    for (i, b) in eq.bands.iter().enumerate() {
        let fail = |what: String| Err(format!("band {i}: {what}"));
        if !(10.0..=30_000.0).contains(&b.freq) {
            return fail(format!("freq {} Hz: it's 10 to 30000", b.freq));
        }
        if !(-30.0..=30.0).contains(&b.gain_db) {
            return fail(format!("gain_db {}: it's -30 to 30", b.gain_db));
        }
        if !(0.025..=40.0).contains(&b.q) {
            return fail(format!("q {}: it's 0.025 to 40", b.q));
        }
        if b.shape.slope_range().is_some() && b.shape.fit_slope(b.slope) != b.slope {
            let (lo, hi) = b.shape.slope_range().unwrap_or_default();
            let takes = match b.shape {
                crate::song::project::Shape::LowCut | crate::song::project::Shape::HighCut => {
                    format!("{lo} to {hi}, or 144 (a brickwall)")
                }
                crate::song::project::Shape::Notch | crate::song::project::Shape::AllPass => {
                    format!("{lo} to {hi} in twelves")
                }
                _ => format!("{lo} to {hi}"),
            };
            return fail(format!(
                "slope {} dB an octave: a {} takes {takes}",
                b.slope,
                b.shape.name().to_lowercase()
            ));
        }
        if let Some(d) = b.dynamic
            && (!(-30.0..=30.0).contains(&d.range_db)
                || d.threshold_db.is_some_and(|t| !(-80.0..=0.0).contains(&t))
                || !(0.0..=1.0).contains(&d.density))
        {
            return fail(
                "dynamic: range_db is -30 to 30, threshold_db -80 to 0, density 0 to 1".into(),
            );
        }
    }
    if !(0.0..=2.0).contains(&eq.scale) {
        return Err(format!("scale {}: it's 0 to 2", eq.scale));
    }

    Ok(())
}

/// A channel's built-in effects.
pub struct Strip<'a> {
    pub eq: &'a mut Option<Eq>,
    pub comp: &'a mut Option<Comp>,
    pub sat: &'a mut Option<Saturator>,
}

/// A channel's built-in EQ, compressor and saturator.
pub fn strip(p: &mut Project, channel: ChannelRef) -> Result<Strip<'_>, String> {
    Ok(match channel {
        ChannelRef::Track(i) => {
            let t = track(p, i)?;
            Strip {
                eq: &mut t.eq,
                comp: &mut t.comp,
                sat: &mut t.sat,
            }
        }
        ChannelRef::Aux(i) => {
            let a = aux(p, i)?;
            Strip {
                eq: &mut a.eq,
                comp: &mut a.comp,
                sat: &mut a.sat,
            }
        }
        ChannelRef::Master => Strip {
            eq: &mut p.master_eq,
            comp: &mut p.master_comp,
            sat: &mut p.master_sat,
        },
    })
}

/// The mixing commands.
pub(super) fn run(p: &mut Project, _dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::AddSend {
            track: t,
            bus,
            level_db,
            pre,
        } => {
            track(p, t)?;
            let bus = match bus {
                Some(b) if p.auxes.iter().any(|a| a.bus == b) => b,
                Some(b) => return Err(format!("no aux on bus {b}")),
                None => p.add_aux(),
            };
            let level_db = finite("level_db", level_db)?;
            track(p, t)?.sends.push(AuxSend { bus, level_db, pre });
            return Ok(json!({ "bus": bus }));
        }
        Command::SetSend {
            track: t,
            send,
            level_db,
            pre,
        } => {
            let s = track(p, t)?
                .sends
                .get_mut(send)
                .ok_or(format!("track {t} has no send {send}"))?;
            if let Some(v) = level_db {
                s.level_db = finite("level_db", v)?;
            }
            if let Some(v) = pre {
                s.pre = v;
            }
        }
        Command::RemoveSend { track: t, send } => {
            let sends = &mut track(p, t)?.sends;
            if send >= sends.len() {
                return Err(format!("track {t} has no send {send}"));
            }
            sends.remove(send);
        }
        Command::SetAux {
            aux: a,
            name,
            volume_db,
            pan,
            mute,
        } => {
            let a = aux(p, a)?;
            if let Some(v) = name {
                a.name = v;
            }
            if let Some(v) = volume_db {
                a.volume_db = finite("volume_db", v)?;
            }
            if let Some(v) = pan {
                a.pan = finite("pan", v)?.clamp(-1.0, 1.0);
            }
            if let Some(v) = mute {
                a.mute = v;
            }
        }
        Command::SetEq { channel, eq } => {
            // Loading a song takes the nearest; asked for, it's a mistake.
            let resolution = eq.as_ref().and_then(|e| e.get("resolution")?.as_f64());
            if let Some(r) = resolution.filter(|r| !(0.0..=3.0).contains(r)) {
                return Err(format!("resolution {r}: it's 0 (low) to 3 (very high)"));
            }
            // Its sidechain is a track, numbered as commands number them: its id here.
            let source = match eq.as_ref().and_then(|e| e.get("sidechain")) {
                Some(Value::Null) => Some(None),
                Some(v) => {
                    let t = v
                        .as_u64()
                        .ok_or("sidechain is a track's number (from 0), or null for none")?;
                    Some(Some(track(p, t as usize)?.id))
                }
                None => None,
            };
            let slot = strip(p, channel)?.eq;
            let mut new = *slot;
            set_effect(&mut new, eq)?;
            if let (Some(e), Some(source)) = (&mut new, source) {
                e.sidechain = source;
            }
            if let Some(e) = &new {
                check_eq(e)?;
            }
            *slot = new;
        }
        Command::SetEqBand {
            channel,
            band: i,
            shape,
            freq,
            gain_db,
            q,
            slope,
            placement,
            dynamic,
            off,
            gain_q,
            remove,
        } => {
            let slot = strip(p, channel)?.eq;
            let mut eq = slot.unwrap_or_default();
            let n = eq.bands.len();
            if i > n || (remove && i == n) {
                return Err(format!("no band {i}: the EQ has {n} (band {n} adds one)"));
            }
            if remove {
                eq.bands.remove(i);
            } else {
                let mut b = eq.bands.get(i).copied().unwrap_or_default();
                if let Some(shape) = shape {
                    // A new shape starts with its own width, unless one's given.
                    if shape != b.shape && q.is_none() {
                        b.q = shape.default_q();
                    }
                    b.shape = shape;
                }
                b.freq = freq.unwrap_or(b.freq);
                b.gain_db = gain_db.unwrap_or(b.gain_db);
                b.q = q.unwrap_or(b.q);
                b.slope = slope.unwrap_or(b.slope);
                b.placement = placement.unwrap_or(b.placement);
                b.off = off.unwrap_or(b.off);
                b.gain_q = gain_q.unwrap_or(b.gain_q);
                match dynamic {
                    None => {}
                    Some(Value::Bool(false)) | Some(Value::Null) => b.dynamic = None,
                    Some(changes) => {
                        let d = b.dynamic.unwrap_or_default();
                        let mut v = serde_json::to_value(d).map_err(|e| e.to_string())?;
                        overlay(&mut v, changes);
                        let d = serde_json::from_value::<Dynamic>(v).map_err(|e| e.to_string())?;
                        b.dynamic = Some(d);
                    }
                }
                match i == n {
                    true => {
                        eq.bands.push(b).ok_or("an EQ has at most 24 bands")?;
                    }
                    false => eq.bands[i] = b,
                }
            }
            check_eq(&eq)?;
            *slot = Some(eq);
        }
        Command::SetComp { channel, comp } => set_effect(strip(p, channel)?.comp, comp)?,
        Command::SetSaturator { channel, sat } => set_effect(strip(p, channel)?.sat, sat)?,
        _ => unreachable!("not one of the mixing commands"),
    }
    Ok(json!({}))
}
