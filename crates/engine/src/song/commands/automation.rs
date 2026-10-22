//! Commands on automation: lanes set, smoothed and shaped, and the track's mode.

use super::*;

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
) -> Result<Param, String> {
    Ok(match param {
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
            };
            let (lo, hi) = param.range();
            for pt in &mut points {
                pt.value = pt.value.clamp(lo, hi);
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
            from,
            to,
            width,
        } => {
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting)?;
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
            shape,
            from,
            to,
            every,
            low,
            high,
        } => {
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting)?;
            if to <= from {
                return Err("`to` must be after `from`".into());
            }
            let every = every.unwrap_or(PPQ).max(1);
            let (low, high) = (finite("low", low)?, finite("high", high)?);
            track(p, t)?
                .automation
                .shape(param, shape, from, to, every, low, high);
        }
        Command::SetAutomationMode { track: t, mode } => track(p, t)?.automation.mode = mode,
        _ => unreachable!("not one of the automation commands"),
    }
    Ok(json!({}))
}
