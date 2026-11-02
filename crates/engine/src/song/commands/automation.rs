//! Commands on automation: lanes set, smoothed and shaped, and the track's mode.

use super::*;
use crate::song::project::{Fx, OWN_COMP, OWN_SAT, POLY_PARAMS, knobs, poly_param};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AutoParam {
    Volume,
    Pan,
    Mute,
    Send,
    Plugin,
    /// An EQ band's frequency (Hz), gain (dB) or width (Q): the band's place as `id`.
    EqFreq,
    EqGain,
    EqQ,
    /// One of a modulator's settings: the modulator's place as `modulator`, the setting as
    /// `setting`.
    Modulator,
    /// The limiter's gain (dB), ceiling (dB), release (ms) or clip (0 to 1).
    LimitGain,
    LimitCeiling,
    LimitRelease,
    LimitClip,
    /// A knob of an effect stacked on the track: `fx`, `which` of those, `knob` by name; its
    /// values in the knob's own units.
    Fx,
    /// One of the synth's parameters: `knob` its name or id, its values in their own units.
    Synth,
}

/// One of a track's synth's parameters, as automation names it.
pub(crate) fn synth_param(p: &mut Project, t: usize, knob: Option<&str>) -> Result<Param, String> {
    if !matches!(track(p, t)?.instrument, Some(Instrument::Poly(_))) {
        return Err(format!("track {t} has no synth: add one with set_synth"));
    }
    let Some(knob) = knob else {
        return Err("a synth's automation needs `knob`: a parameter's id or name".into());
    };
    let i = poly_param(knob)
        .ok_or_else(|| format!("the synth has no {knob}: `describe` lists them"))?;
    Ok(Param::Poly(i as u16))
}

/// An effect's knob as a command names it: the effect, which of those, the knob, and whether it's
/// the track's own.
type Stacked<'a> = (Option<&'a str>, Option<usize>, Option<&'a str>, bool);

/// A knob of a stacked effect, as automation names it.
pub(crate) fn tool_param(
    p: &mut Project,
    t: usize,
    (fx, which, knob, own): Stacked,
) -> Result<Param, String> {
    let (Some(name), Some(knob)) = (fx, knob) else {
        return Err("an effect's automation needs `fx` and `knob`".into());
    };
    let Ok(fx) = serde_json::from_value::<Fx>(json!(name)) else {
        return Err(format!(
            "no {name}: `fx` is filter, motion, shaper, gate, de_esser, stereo, multiband, pitch, \
             tape_stop, tune, reverb or delay"
        ));
    };
    let which = which.unwrap_or(0);
    let place = match (own, fx) {
        (true, Fx::Comp) if track(p, t)?.comp.is_some() => OWN_COMP as usize,
        (true, Fx::Sat) if track(p, t)?.sat.is_some() => OWN_SAT as usize,
        (true, _) => return Err(format!("track {t} has no {name} of its own")),
        _ => match track(p, t)?.chain_mut().nth_more(fx, which) {
            Some(k) => k,
            None => {
                return Err(format!(
                    "track {t} has no {name} {which}: add it with set_fx"
                ));
            }
        },
    };
    let table = knobs(fx);
    let Some(i) = table.iter().position(|k| k.name.eq_ignore_ascii_case(knob)) else {
        let names: Vec<&str> = table.iter().map(|k| k.name).collect();
        return Err(format!(
            "{name} has no knob {knob}: it has {}",
            names.join(", ")
        ));
    };
    Ok(Param::Tool(place as u8, i as u8))
}

/// A value in a knob's own units, as where the knob sits: what its lane holds.
fn lane_value(p: &mut Project, t: usize, param: Param, v: f32) -> Result<f32, String> {
    if let Param::Poly(i) = param {
        return Ok(POLY_PARAMS[i as usize].scale.pos(v));
    }
    let Param::Tool(place, knob) = param else {
        return Ok(v);
    };
    let kind = track(p, t)?.unit_at(place).map(|u| u.kind());
    Ok(kind
        .and_then(|k| knobs(k).get(knob as usize))
        .map_or(v, |k| k.scale.pos(v)))
}

/// A limiter's setting, as automation names it: the track must have a limiter.
pub(crate) fn limit_param(p: &mut Project, t: usize, param: AutoParam) -> Result<Param, String> {
    if track(p, t)?.limit.is_none() {
        return Err(format!(
            "track {t} has no limiter: add one with set_limiter"
        ));
    }
    Ok(Param::Limit(match param {
        AutoParam::LimitGain => LimitParam::Gain,
        AutoParam::LimitCeiling => LimitParam::Ceiling,
        AutoParam::LimitRelease => LimitParam::Release,
        _ => LimitParam::Clip,
    }))
}

/// An EQ band's setting, as automation names it: its place in the track's EQ.
pub(crate) fn eq_param(
    p: &mut Project,
    t: usize,
    param: AutoParam,
    band: Option<u32>,
) -> Result<Param, String> {
    let field = match param {
        AutoParam::EqFreq => crate::song::project::EqField::Freq,
        AutoParam::EqGain => crate::song::project::EqField::Gain,
        _ => crate::song::project::EqField::Q,
    };
    let Some(band) = band else {
        return Err("an EQ band's automation needs `id`: the band's place in the EQ".into());
    };
    let n = track(p, t)?.eq.map_or(0, |e| e.bands.len());
    if band as usize >= n {
        return Err(format!("track {t}'s EQ has no band {band}: it has {n}"));
    }
    Ok(Param::Eq(band as u8, field))
}

/// The parameter a command's automation is of.
#[allow(clippy::too_many_arguments)]
pub(crate) fn auto_param(
    p: &mut Project,
    t: usize,
    param: AutoParam,
    send: Option<usize>,
    plugin: Option<Which>,
    id: Option<u32>,
    modulator: Option<usize>,
    setting: Option<ModParam>,
    tool: Stacked,
) -> Result<Param, String> {
    Ok(match param {
        AutoParam::Fx => tool_param(p, t, tool)?,
        AutoParam::Synth => synth_param(p, t, tool.2)?,
        AutoParam::Volume => Param::Volume,
        AutoParam::Pan => Param::Pan,
        AutoParam::Mute => Param::Mute,
        AutoParam::Send => send_param(p, t, send)?,
        AutoParam::Plugin => {
            let (Some(plugin), Some(id)) = (plugin, id) else {
                return Err("a plugin parameter needs `plugin` and `id`".into());
            };
            if track(p, t)?.plugin(plugin).is_none() {
                return Err(format!("track {t} has no plugin there"));
            }
            Param::Plugin(plugin, id)
        }
        AutoParam::EqFreq | AutoParam::EqGain | AutoParam::EqQ => eq_param(p, t, param, id)?,
        AutoParam::Modulator => mod_param(p, t, modulator, setting)?,
        AutoParam::LimitGain
        | AutoParam::LimitCeiling
        | AutoParam::LimitRelease
        | AutoParam::LimitClip => limit_param(p, t, param)?,
    })
}

/// A modulator's setting, as automation names it: the modulator's place, and a setting it has.
pub(crate) fn mod_param(
    p: &mut Project,
    t: usize,
    modulator: Option<usize>,
    setting: Option<ModParam>,
) -> Result<Param, String> {
    let (Some(k), Some(setting)) = (modulator, setting) else {
        return Err(
            "a modulator's automation needs `modulator` (its place) and `setting` (value, phase, glide, attack, decay, sustain or release)"
                .into(),
        );
    };
    modulator_at(p, t, k)?;
    // One its stack shares: the stack's settings.
    let kind = p
        .mod_kind(t, k)
        .ok_or_else(|| format!("track {t}'s modulator {k} is one its stack no longer shares"))?;
    if setting.get(kind).is_none() {
        let has: Vec<&str> = ModParam::of(kind).into_iter().map(ModParam::name).collect();
        return Err(format!(
            "track {t}'s modulator {k} ({}) has no {}: it has {}",
            kind.name(),
            setting.name().to_lowercase(),
            match has.is_empty() {
                true => "none to automate".to_string(),
                false => has.join(", ").to_lowercase(),
            }
        ));
    }
    Ok(Param::Mod(k as u8, setting))
}

/// A track's send, as its automation names it: by the bus it sends to.
pub(crate) fn send_param(p: &mut Project, t: usize, send: Option<usize>) -> Result<Param, String> {
    let Some(i) = send else {
        return Err("a send's automation needs `send`: which of the track's sends".into());
    };
    let bus = track(p, t)?
        .sends
        .get(i)
        .ok_or_else(|| format!("track {t} has no send {i}"))?
        .bus;
    Ok(Param::Send(bus))
}

/// The automation commands.
pub(super) fn run(p: &mut Project, _dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            name,
            modulator,
            setting,
            fx,
            which,
            knob,
            own,
            points,
        } => {
            let mut points = points
                .into_iter()
                .map(|pt| match (pt.value, &pt.text) {
                    (Some(value), _) => Ok(AutoPoint {
                        curve: finite("curve", pt.curve.unwrap_or(0.0))?.clamp(-1.0, 1.0),
                        step: pt.step.unwrap_or(false),
                        ease: pt.ease.unwrap_or(false),
                        ..AutoPoint::new(pt.tick, finite("value", value)?)
                    }),
                    (None, Some(t)) => {
                        Err(format!("{t:?} needs the plugin to be read: {UNRESOLVED}"))
                    }
                    (None, None) => Err("each point needs a `value` (or a `text`)".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            points.sort_by_key(|p| p.tick);
            let param = match param {
                AutoParam::Volume => Param::Volume,
                AutoParam::Pan => Param::Pan,
                AutoParam::Mute => Param::Mute,
                AutoParam::Send => send_param(p, t, send)?,
                AutoParam::Plugin => {
                    let (Some(plugin), Some(id)) = (plugin, id) else {
                        return Err("a plugin parameter needs `plugin` and `id`".into());
                    };
                    let ParamRef::Id(id) = id else {
                        return Err(format!(
                            "a parameter by name needs the plugin: {UNRESOLVED}"
                        ));
                    };
                    if track(p, t)?.plugin(plugin).is_none() {
                        return Err(format!("track {t} has no plugin there"));
                    }
                    for pt in &mut points {
                        pt.value = finite("value", pt.value)?.clamp(0.0, 1.0);
                    }
                    Param::Plugin(plugin, id)
                }
                AutoParam::EqFreq | AutoParam::EqGain | AutoParam::EqQ => {
                    let band = match id {
                        Some(ParamRef::Id(n)) => Some(n),
                        Some(_) => return Err("an EQ band's `id` is its place: a number".into()),
                        None => None,
                    };
                    eq_param(p, t, param, band)?
                }
                AutoParam::Modulator => mod_param(p, t, modulator, setting)?,
                AutoParam::LimitGain
                | AutoParam::LimitCeiling
                | AutoParam::LimitRelease
                | AutoParam::LimitClip => limit_param(p, t, param)?,
                AutoParam::Fx => tool_param(p, t, (fx.as_deref(), which, knob.as_deref(), own))?,
                AutoParam::Synth => synth_param(p, t, knob.as_deref())?,
            };
            let (lo, hi) = param.range();
            for pt in &mut points {
                pt.value = lane_value(p, t, param, pt.value)?.clamp(lo, hi);
                pt.step |= param.steps();
            }
            let a = &mut track(p, t)?.automation;
            *a.points_mut(param) = points;
            if let Some(name) = name {
                a.name(param, &name);
            }
            a.tidy();
        }
        Command::SmoothAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            modulator,
            setting,
            fx,
            which,
            knob,
            own,
            from,
            to,
            width,
        } => {
            let tool = (fx.as_deref(), which, knob.as_deref(), own);
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting, tool)?;
            let a = &mut track(p, t)?.automation;
            let points = a.points(param);
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return Err(format!("track {t} has no automation there to smooth"));
            };
            let (from, to) = (from.unwrap_or(first.tick), to.unwrap_or(last.tick));
            if to <= from {
                return Err("`to` must be after `from`".into());
            }
            a.smooth(param, from, to, width.unwrap_or(PPQ / 2));
        }
        Command::ShapeAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            modulator,
            setting,
            fx,
            which,
            knob,
            own,
            shape,
            from,
            to,
            every,
            low,
            high,
        } => {
            let tool = (fx.as_deref(), which, knob.as_deref(), own);
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting, tool)?;
            if to <= from {
                return Err("`to` must be after `from`".into());
            }
            let every = every.unwrap_or(PPQ).max(1);
            let (low, high) = (finite("low", low)?, finite("high", high)?);
            let (low, high) = (
                lane_value(p, t, param, low)?,
                lane_value(p, t, param, high)?,
            );
            track(p, t)?
                .automation
                .shape(param, shape, from, to, every, low, high);
        }
        Command::SetAutomationMode { track: t, mode } => track(p, t)?.automation.mode = mode,
        _ => unreachable!("not one of the automation commands"),
    }
    Ok(json!({}))
}
