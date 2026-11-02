//! Commands on the mix: sends and auxes, and the strip's EQ, compressor and saturator.

use super::*;

pub(crate) fn aux(p: &mut Project, i: usize) -> Result<&mut crate::song::project::Aux, String> {
    let n = p.auxes.len();
    p.auxes
        .get_mut(i)
        .ok_or_else(|| format!("no aux {i}: there are {n}"))
}

/// A channel's effects, to change.
pub(crate) fn chain(p: &mut Project, ch: ChannelRef) -> Result<ChainMut<'_>, String> {
    Ok(match ch {
        ChannelRef::Track(i) => {
            let n = p.tracks.len();
            p.tracks
                .get_mut(i)
                .ok_or_else(|| format!("no track {i}: there are {n}"))?
                .chain_mut()
        }
        ChannelRef::Aux(i) => aux(p, i)?.chain_mut(),
        ChannelRef::Master => p.master_chain_mut(),
    })
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

/// As `set_effect`, for an effect whose style sets how it usually is: a new one (or one whose
/// `style` changes) starts as that style's, then takes the fields given.
fn set_styled<T, S>(
    slot: &mut Option<T>,
    changes: Option<Value>,
    styled: impl Fn(S) -> T,
) -> Result<(), String>
where
    T: Serialize + serde::de::DeserializeOwned + Default + Clone,
    S: serde::de::DeserializeOwned,
{
    if let Some(style) = changes.as_ref().and_then(|c| c.get("style")) {
        let style: S = serde_json::from_value(style.clone()).map_err(|e| e.to_string())?;
        let was =
            serde_json::to_value(slot.clone().unwrap_or_default()).map_err(|e| e.to_string())?;
        if was.get("style") != changes.as_ref().and_then(|c| c.get("style")) {
            *slot = Some(styled(style));
        }
    }
    set_effect(slot, changes)
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
    pub limit: &'a mut Option<Limiter>,
}

/// A channel's built-in EQ, compressor, saturator and limiter.
pub fn strip(p: &mut Project, channel: ChannelRef) -> Result<Strip<'_>, String> {
    Ok(match channel {
        ChannelRef::Track(i) => {
            let t = track(p, i)?;
            Strip {
                eq: &mut t.eq,
                comp: &mut t.comp,
                sat: &mut t.sat,
                limit: &mut t.limit,
            }
        }
        ChannelRef::Aux(i) => {
            let a = aux(p, i)?;
            Strip {
                eq: &mut a.eq,
                comp: &mut a.comp,
                sat: &mut a.sat,
                limit: &mut a.limit,
            }
        }
        ChannelRef::Master => Strip {
            eq: &mut p.master_eq,
            comp: &mut p.master_comp,
            sat: &mut p.master_sat,
            limit: &mut p.master_limit,
        },
    })
}

/// Changes the `which`th built-in of its kind on a channel (0 its own, 1 on the others in the
/// order they run; one past the last adds another, last before a limiter that's last) with `f`:
/// what it leaves None is taken out.
fn on_one<T: crate::song::project::AsUnit>(
    p: &mut Project,
    channel: ChannelRef,
    which: usize,
    own: impl FnOnce(Strip<'_>) -> &mut Option<T>,
    f: impl FnOnce(&mut Option<T>) -> Result<(), String>,
) -> Result<(), String> {
    match which {
        0 => f(own(strip(p, channel)?)),
        n => on_stacked(p, channel, n - 1, 1, f),
    }
}

/// Changes the `nth` (from 0) built-in of its kind stacked on a channel with `f` (one past the
/// last adds another); what it leaves None is taken out. `own` is how many of the kind the
/// channel has of its own (1 or 0), for saying which there are.
fn on_stacked<T: crate::song::project::AsUnit>(
    p: &mut Project,
    channel: ChannelRef,
    nth: usize,
    own: usize,
    f: impl FnOnce(&mut Option<T>) -> Result<(), String>,
) -> Result<(), String> {
    let mut c = chain(p, channel)?;
    let at = c.nth_more(T::KIND, nth);
    let have = c.more.iter().filter(|u| u.kind() == T::KIND).count();
    if at.is_none() && nth > have {
        return Err(format!(
            "no {}: the channel has {} of these (which {} adds one)",
            nth + own,
            have + own,
            have + own
        ));
    }
    let mut slot = at.and_then(|k| T::of(&c.more[k]));
    f(&mut slot)?;
    match (at, slot) {
        (Some(k), Some(v)) => c.more[k] = v.unit(),
        (Some(k), None) => c.remove_more(k)?,
        (None, Some(v)) => drop(c.add_more(v.unit())),
        (None, None) => {}
    }
    Ok(())
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
        Command::SetEq { channel, eq, which } => {
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
            on_one(
                p,
                channel,
                which,
                |s| s.eq,
                |slot| {
                    let mut new = *slot;
                    set_effect(&mut new, eq)?;
                    if let (Some(e), Some(source)) = (&mut new, source) {
                        e.sidechain = source;
                    }
                    if let Some(e) = &new {
                        check_eq(e)?;
                    }
                    *slot = new;
                    Ok(())
                },
            )?;
        }
        Command::SetEqBand {
            channel,
            band: i,
            which,
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
            on_one(
                p,
                channel,
                which,
                |s| s.eq,
                |slot| {
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
                                let d = serde_json::from_value::<Dynamic>(v)
                                    .map_err(|e| e.to_string())?;
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
                    Ok(())
                },
            )?;
        }
        Command::SetComp {
            channel,
            comp,
            which,
        } => on_one(p, channel, which, |s| s.comp, |s| set_effect(s, comp))?,
        Command::SetSaturator {
            channel,
            sat,
            which,
        } => on_one(p, channel, which, |s| s.sat, |s| set_effect(s, sat))?,
        Command::SetLimiter {
            channel,
            limit,
            which,
        } => on_one(p, channel, which, |s| s.limit, |s| set_effect(s, limit))?,
        Command::SetReverb {
            channel,
            reverb,
            which,
        } => on_stacked::<crate::song::project::Reverb>(p, channel, which, 0, |s| {
            set_styled(s, reverb, crate::song::project::ReverbStyle::reverb)
        })?,
        Command::SetFx {
            channel,
            fx,
            settings,
            which,
        } => {
            use crate::song::project::{
                DeEsser, FilterFx, Gate, Motion, MotionStyle, Multiband, PitchShift, Shaper,
                Stereo, TapeStop, Tune,
            };
            let s = settings;
            match fx.as_str() {
                "filter" => on_stacked::<FilterFx>(p, channel, which, 0, |x| set_effect(x, s)),
                "motion" => on_stacked::<Motion>(p, channel, which, 0, |x| {
                    set_styled(x, s, MotionStyle::motion)
                }),
                "shaper" => on_stacked::<Shaper>(p, channel, which, 0, |x| set_effect(x, s)),
                "gate" => on_stacked::<Gate>(p, channel, which, 0, |x| set_effect(x, s)),
                "de_esser" => on_stacked::<DeEsser>(p, channel, which, 0, |x| set_effect(x, s)),
                "stereo" => on_stacked::<Stereo>(p, channel, which, 0, |x| set_effect(x, s)),
                "multiband" => on_stacked::<Multiband>(p, channel, which, 0, |x| set_effect(x, s)),
                "pitch" => on_stacked::<PitchShift>(p, channel, which, 0, |x| set_effect(x, s)),
                "tape_stop" => on_stacked::<TapeStop>(p, channel, which, 0, |x| set_effect(x, s)),
                "tune" => on_stacked::<Tune>(p, channel, which, 0, |x| set_effect(x, s)),
                _ => Err(format!(
                    "no {fx}: `fx` is filter, motion, shaper, gate, de_esser, stereo, multiband, \
                     pitch, tape_stop or tune"
                )),
            }?
        }
        Command::SetDelay {
            channel,
            delay,
            which,
        } => on_stacked::<crate::song::project::Delay>(p, channel, which, 0, |s| {
            set_styled(s, delay, crate::song::project::DelayStyle::delay)
        })?,
        _ => unreachable!("not one of the mixing commands"),
    }
    Ok(json!({}))
}
