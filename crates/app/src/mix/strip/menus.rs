//! The strip's menus: plugins for a slot, outputs, sends and inputs; and plugin editors opened.

use super::*;

impl Workspace {
    /// Opens a plugin's editor, saying why not if it can't.
    pub fn open_editor(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let id = self.owner_id(ch);
        match self.player.open_editor(id, which) {
            Ok(()) => self.picture_when_shown(ch, which, cx),
            Err(e) => {
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// Opens a plugin's window once the engine has it loaded (a just-added plugin loads in the
    /// background), giving up after a few seconds.
    pub fn open_editor_when_loaded(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let id = self.owner_id(ch);
        cx.spawn(async move |this, cx| {
            for _ in 0..50 {
                let opened = this
                    .update(cx, |ws, cx| {
                        let ok = ws.player.open_editor(id, which).is_ok();
                        if ok {
                            ws.picture_when_shown(ch, which, cx);
                        }
                        ok
                    })
                    .unwrap_or(true);
                if opened {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    /// The plugin in a slot, if there's one.
    pub(crate) fn slot_plugin(&self, ch: Channel, which: Which) -> Option<&PluginRef> {
        match (ch, which) {
            (Channel::Track(t), w) => self.session.project.tracks.get(t)?.plugin(w),
            (_, Which::Insert(n)) => self.inserts(ch).get(n),
            _ => None,
        }
    }

    /// A slot's menu: Pina Studio's instruments, taking its plugin out, the plugin's presets, and
    /// the picker for every plugin and built-in.
    pub fn plugin_menu(&self, ch: Channel, which: Which, at: Point<Pixels>) -> Menu {
        let mut items = Vec::new();
        match (ch, which) {
            (Channel::Track(track), Which::Instrument) => {
                for (name, instrument) in [
                    ("Sine Synth", Instrument::Synth),
                    ("Drum Kit", Instrument::Drums),
                ] {
                    items.push((
                        name.to_string(),
                        MenuAction::Instrument { track, instrument },
                    ));
                }
            }
            // Taking out a plugin, from a slot that has one.
            (_, Which::Insert(index)) if index < self.inserts(ch).len() => {
                let action = MenuAction::Insert {
                    channel: ch,
                    index,
                    plugin: None,
                };
                items.push(("No Plug-in".to_string(), action));
            }
            _ => {}
        }
        // The presets of the plugin in the slot, the loaded one ticked.
        let loaded = self.slot_plugin(ch, which).and_then(|r| r.preset.clone());
        if let Some(plugin) = self.player.plugin(self.owner_id(ch), which) {
            for preset in plugin.presets() {
                let mark = if loaded.as_ref() == Some(&preset) {
                    "✓ "
                } else {
                    ""
                };
                let label = format!("{mark}Preset: {}", preset.name);
                let action = MenuAction::Preset {
                    channel: ch,
                    which,
                    preset,
                };
                items.push((label, action));
            }
        }
        // Every plugin and built-in, with pictures, in the picker.
        items.push((
            "Choose a Plug-in…".to_string(),
            MenuAction::Plugins(crate::sounds::plugins::PluginOp::OpenMini(ch, which, at)),
        ));
        if self.plugins.is_none() {
            items.push(("Looking for plugins…".to_string(), MenuAction::Nothing));
        }
        // Presets and plugins run long: typing narrows them down.
        Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        }
    }

    /// Where a track can play: the master, or any aux or stack.
    pub fn output_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items = vec![(
            "Stereo Out".to_string(),
            MenuAction::Output { track, bus: None },
        )];
        items.extend(self.session.project.auxes.iter().map(|a| {
            (
                format!("Bus {} ({})", a.bus, a.name),
                MenuAction::Output {
                    track,
                    bus: Some(a.bus),
                },
            )
        }));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The buses a track can send to: every aux, and a new one.
    pub fn send_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items: Vec<_> = self
            .session
            .project
            .auxes
            .iter()
            .map(|a| {
                (
                    format!("Bus {} ({})", a.bus, a.name),
                    MenuAction::Send {
                        track,
                        bus: Some(a.bus),
                    },
                )
            })
            .collect();
        items.push(("New Aux".to_string(), MenuAction::Send { track, bus: None }));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// What an audio track can record: each input channel alone, and in pairs.
    pub fn input_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let channels = pina_engine::audio::record::input_channels();
        let mut items = Vec::new();
        for stereo in [true, false] {
            let step = if stereo { 2 } else { 1 };
            for channel in (0..channels).step_by(step) {
                let input = Input { channel, stereo };
                let action = MenuAction::Input {
                    track,
                    input: Some(input),
                };
                items.push((input.name(), action));
            }
        }
        if items.is_empty() {
            items.push(("No audio input".into(), MenuAction::Nothing));
        }
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// What a send can do: switch between before and after the fader, or go.
    pub fn send_options(&self, track: usize, index: usize, at: Point<Pixels>) -> Menu {
        let pre = self.session.project.tracks[track].sends[index].pre;
        let mark = |on: bool| if on { "✓ " } else { "    " };
        Menu {
            at,
            items: vec![
                (
                    format!("{}Post-fader", mark(!pre)),
                    MenuAction::SendPre {
                        track,
                        index,
                        pre: false,
                    },
                ),
                (
                    format!("{}Pre-fader", mark(pre)),
                    MenuAction::SendPre {
                        track,
                        index,
                        pre: true,
                    },
                ),
                (
                    "Remove Send".into(),
                    MenuAction::RemoveSend { track, index },
                ),
            ],
            ..Default::default()
        }
    }
}
