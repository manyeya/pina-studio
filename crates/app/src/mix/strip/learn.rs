//! A channel's compressor or limiter set by listening: `pina learn` in a process of its own (it
//! asks this window for the song, renders the channel as the effect hears it and measures it),
//! its answer set as one edit.

use super::*;

impl Workspace {
    /// Sets a channel's compressor by listening to what it plays (a track alone, or the master the
    /// mix). An aux takes a bus compressor's settings as they come: what reaches it isn't heard
    /// alone.
    ///
    /// One of a channel's compressors: its own (`stacked` None), or one stacked on it.
    // shortcut: a stacked one hears the channel as its own compressor would (before its
    // built-ins); render the chain up to it when stacks get long.
    pub fn learn_comp_at(
        &mut self,
        channel: Channel,
        stacked: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let arg = match channel {
            Channel::Track(t) => vec!["--track".to_string(), t.to_string()],
            Channel::Master => vec!["--master".to_string()],
            // shortcut: an aux isn't rendered alone; listen to what's sent to it when a bus
            // compressor wants to be set for its sources.
            Channel::Aux(_) => {
                self.edit(cx, |ws| ws.set_built_in(channel, stacked, Some(bus_comp())));
                return;
            }
        };
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let dir = self.session.dir.clone();
        self.learning = Some(channel);
        self.status = Some("Listening to set the compressor…".into());
        cx.notify();
        let heard = cx.background_spawn(async move {
            let out = std::process::Command::new(exe)
                .arg("learn")
                .arg(&dir)
                .args(&arg)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            serde_json::from_slice::<Option<Comp>>(&out.stdout).map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let heard = heard.await;
            this.update(cx, |ws, cx| {
                ws.learning = None;
                ws.status = Some(match heard {
                    // Still has a compressor (it wasn't taken out while it listened): set.
                    Ok(Some(c)) if ws.built_in::<Comp>(channel, stacked).is_some() => {
                        ws.edit(cx, |ws| ws.set_built_in(channel, stacked, Some(c)));
                        format!(
                            "Compressor set by listening: {}, {} dB at {}:1, {} ms in, {}",
                            c.style.name(),
                            scales::trim(c.threshold_db),
                            scales::trim(c.ratio),
                            scales::trim(c.attack_ms),
                            match c.auto_release {
                                true => "auto release".to_string(),
                                false => format!("{} ms out", scales::trim(c.release_ms)),
                            }
                        )
                    }
                    Ok(Some(_)) => "The compressor was taken out while it listened".into(),
                    Ok(None) => {
                        "Nothing plays there to listen to: the compressor keeps its settings".into()
                    }
                    Err(e) => format!("Couldn't listen: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Workspace {
    /// Sets a channel's saturator by listening to what it plays (a track alone after its EQ and
    /// compressor, or the master the mix): its bands, each one's style, drive, tone and
    /// dynamics for the part in the song's genre, and its output so it's as loud as it went
    /// in. An aux, not heard alone, takes the genre's glue.
    ///
    /// One of a channel's saturators: its own (`stacked` None), or one stacked on it.
    // shortcut: a stacked one hears the channel as its own saturator would.
    pub fn learn_sat_at(
        &mut self,
        channel: Channel,
        stacked: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        use pina_engine::dsp::sat::learn::{Ear, Heard, plan};
        let arg = match channel {
            Channel::Track(t) => vec!["--track".to_string(), t.to_string()],
            Channel::Master => vec!["--master".to_string()],
            // shortcut: an aux isn't rendered alone; listen to what's sent to it when its
            // saturator wants to be set for its sources.
            Channel::Aux(_) => {
                let p = &self.session.project;
                let genre = p.genre.clone().unwrap_or_default();
                let ear = Ear {
                    part: "bus",
                    genre: &genre,
                    lowest_hz: None,
                    key: p.song_key(),
                };
                let (s, _) = plan(&ear, &Heard::flat());
                self.edit(cx, |ws| ws.set_built_in(channel, stacked, Some(s)));
                return;
            }
        };
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let dir = self.session.dir.clone();
        self.learning = Some(channel);
        self.status = Some("Listening to set the saturator…".into());
        cx.notify();
        let heard = cx.background_spawn(async move {
            let out = std::process::Command::new(exe)
                .arg("learn")
                .arg(&dir)
                .args(&arg)
                .arg("--sat")
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let heard = heard.await;
            this.update(cx, |ws, cx| {
                ws.learning = None;
                let learned =
                    heard.map(|v| serde_json::from_value::<Saturator>(v["sat"].clone()).ok());
                ws.status = Some(match learned {
                    // Still has a saturator (it wasn't taken out while it listened): set.
                    Ok(Some(s)) if ws.built_in::<Saturator>(channel, stacked).is_some() => {
                        ws.edit(cx, |ws| ws.set_built_in(channel, stacked, Some(s)));
                        let styles: Vec<&str> = s.bands.iter().map(|b| b.style.name()).collect();
                        format!(
                            "Saturator set by listening: {} band{}, {}",
                            s.bands.len(),
                            if s.bands.len() == 1 { "" } else { "s" },
                            styles.join(" · ")
                        )
                    }
                    Ok(Some(_)) => "The saturator was taken out while it listened".into(),
                    Ok(None) => {
                        "Nothing plays there to listen to: the saturator keeps its settings".into()
                    }
                    Err(e) => format!("Couldn't listen: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Sets a channel's limiter by listening to what reaches it, to come out at `target` LUFS:
    /// its style, release and gain (its ceiling and lookahead kept).
    pub fn learn_limit(&mut self, channel: Channel, target: f32, cx: &mut Context<Self>) {
        let arg = match channel {
            Channel::Track(t) => vec!["--track".to_string(), t.to_string()],
            Channel::Aux(a) => vec!["--aux".to_string(), a.to_string()],
            Channel::Master => vec!["--master".to_string()],
        };
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let dir = self.session.dir.clone();
        self.learning = Some(channel);
        self.status = Some(format!(
            "Listening to set the limiter for {target:.0} LUFS…"
        ));
        cx.notify();
        let heard = cx.background_spawn(async move {
            let out = std::process::Command::new(exe)
                .arg("learn")
                .arg(&dir)
                .args(&arg)
                .args(["--limit", &target.to_string()])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let heard = heard.await;
            this.update(cx, |ws, cx| {
                ws.learning = None;
                let learned = heard.map(|v| {
                    let lufs = v["lufs"].as_f64().unwrap_or(f64::NAN) as f32;
                    let limit = serde_json::from_value::<Limiter>(v["limit"].clone()).ok();
                    limit.map(|l| (l, lufs))
                });
                ws.status = Some(match learned {
                    // Still has a limiter (it wasn't taken out while it listened): set.
                    Ok(Some((l, lufs))) if ws.limiter(channel).is_some() => {
                        ws.edit(cx, |ws| *ws.limiter_mut(channel) = Some(l));
                        format!(
                            "Limiter set by listening: {}, +{:.1} dB, release {} ms: {lufs:.1} LUFS",
                            l.style.name(),
                            l.gain_db,
                            scales::trim(l.release_ms),
                        )
                    }
                    Ok(Some(_)) => "The limiter was taken out while it listened".into(),
                    Ok(None) => {
                        "Nothing plays there to listen to: the limiter keeps its settings".into()
                    }
                    Err(e) => format!("Couldn't listen: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// A bus compressor as it comes: 2:1 glue, a slow attack, an automatic release, the lows not
/// pumping it.
pub fn bus_comp() -> Comp {
    Comp {
        style: pina_engine::song::project::CompStyle::Bus,
        ratio: 2.0,
        attack_ms: 30.0,
        release_ms: 600.0,
        auto_release: true,
        sidechain_hz: 90.0,
        spectral: 0.4,
        ..Comp::default()
    }
}
