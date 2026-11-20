//! Settings (⌘,): a section at a time from the list down the left, kept for every
//! song in `~/Library/Application Support/Pina Studio/settings.json`. Track headers (which parts show,
//! and how tall new tracks are), regions and the grid (names,
//! contents, colour, bar shading, beat lines), editing (the loop zone at a region's end, writing
//! with a double-click), the transport (following the playhead, going back where play started on
//! stop), automation (its lanes' height, what an automated control does moved while stopped,
//! snapping, thinning, Write turning to Latch, automation going with regions), and the editor's
//! tabs (a hidden one's shortcut still opens it). And audio: the output, and its buffer; and
//! MIDI: each controller read as MPE or not, and how far its notes' own bends reach.

use std::path::PathBuf;

use gpui::{Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use pina_engine::audio::{
    engine::{AudioSetup, buffer_sizes, output_devices},
    midi::InputSetup,
    player::Player,
};
use serde::{Deserialize, Serialize};

use crate::{
    arrange::automation::Stopped,
    ui::theme::*,
    workspace::{Bottom, View, Workspace},
};

/// What a setting's switch does.
type Toggle = Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// The editor's tabs left out, by name.
    pub hidden: Vec<String>,
    pub follow_playhead: bool,
    /// Stopping goes back to where play started.
    pub return_on_stop: bool,
    /// The track headers' parts.
    pub header_numbers: bool,
    pub header_icons: bool,
    pub header_kind: bool,
    pub header_meter: bool,
    pub header_strip: bool,
    pub header_record: bool,
    /// A track's height, unless it's been dragged.
    pub track_height: f32,
    /// Regions: their names, what's in them, their colour bright (or soft).
    pub region_names: bool,
    pub region_contents: bool,
    pub region_bright: bool,
    /// The grid: every other four bars shaded, a line each beat.
    pub bar_shading: bool,
    pub beat_lines: bool,
    /// The top half of a region's end loops it; a double-click in an empty lane writes a region.
    pub loop_zone: bool,
    pub double_click_writes: bool,
    /// Automation: how tall each lane is; what moving an automated control does while stopped;
    /// points on the grid; what's recorded thinned of points that change nothing; Write
    /// turning to Latch after a pass; a region's automation going with it when it moves.
    pub auto_height: f32,
    pub stopped: Stopped,
    pub snap_automation: bool,
    pub thin_recorded: bool,
    pub write_to_latch: bool,
    pub carry_automation: bool,
    /// The output it plays through, and that output's buffer.
    pub audio: AudioSetup,
    /// The synth's sounds liked, by name.
    pub liked_sounds: Vec<String>,
    /// The synth's effects added lately, by the names they're offered under, newest first.
    pub recent_effects: Vec<String>,
    /// The synth's sounds played lately (by their keys, newest first); whether a click in the
    /// library loads; the name on sounds and packs made here; the pack Save last put one in.
    pub recent_sounds: Vec<String>,
    /// Recording: how deep its samples, how far its takes move back for the input's lateness
    /// (ms), how many bars it counts in; the click, and whether it's heard only while recording;
    /// a late buffer said in the status bar.
    pub record_depth: pina_engine::audio::record::Depth,
    pub record_offset_ms: f32,
    pub count_in: u8,
    pub click: pina_engine::audio::engine::ClickStyle,
    pub click_only_recording: bool,
    pub late_warning: bool,
    /// Seconds between the song's recovery copies.
    pub keep_every: u32,
    /// Folders of plugins besides the system's.
    pub plugin_folders: Vec<PathBuf>,
    pub load_as_step: bool,
    pub maker: String,
    pub last_pack: Option<String>,
    /// How each MIDI input set apart from the rest is read, by name.
    pub midi_inputs: Vec<MidiInput>,
    /// How much later than it says each output is really heard (ms), by output.
    pub heard_later: std::collections::BTreeMap<String, f32>,
    /// The octave middle C (MIDI 60) is named in: 4 as most learn it, 3 or 5 as some apps have it.
    pub middle_c: i8,
    /// The Songs page as a grid of covers (or a list).
    pub songs_grid: bool,
}

/// A MIDI input, by name, and how it's read.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MidiInput {
    pub name: String,
    #[serde(flatten)]
    pub setup: InputSetup,
}

/// The bend ranges MPE controllers usually have, in semitones.
const NOTE_BENDS: [u8; 4] = [12, 24, 48, 96];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hidden: Vec::new(),
            follow_playhead: true,
            return_on_stop: false,
            header_numbers: true,
            header_icons: true,
            header_kind: true,
            header_meter: true,
            header_strip: true,
            header_record: true,
            track_height: LANE_H,
            region_names: true,
            region_contents: true,
            region_bright: false,
            bar_shading: true,
            beat_lines: true,
            loop_zone: true,
            double_click_writes: true,
            auto_height: 64.0,
            stopped: Stopped::Trim,
            snap_automation: true,
            thin_recorded: true,
            write_to_latch: true,
            carry_automation: false,
            audio: AudioSetup::default(),
            liked_sounds: Vec::new(),
            recent_effects: Vec::new(),
            recent_sounds: Vec::new(),
            record_depth: Default::default(),
            record_offset_ms: 0.0,
            count_in: 0,
            click: Default::default(),
            click_only_recording: false,
            late_warning: true,
            keep_every: 30,
            plugin_folders: Vec::new(),
            load_as_step: false,
            maker: String::new(),
            last_pack: None,
            midi_inputs: Vec::new(),
            heard_later: Default::default(),
            middle_c: 4,
            songs_grid: false,
        }
    }
}

fn file() -> PathBuf {
    pina_engine::paths::support().join("settings.json")
}

impl Settings {
    /// As kept; as they come in tests, which never read or write the person's.
    pub fn load() -> Self {
        match cfg!(test) {
            true => Settings::default(),
            false => std::fs::read(file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn save(&self) {
        if cfg!(test) {
            return;
        }
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn shows(&self, tab: Bottom) -> bool {
        !self.hidden.iter().any(|h| h == tab.key())
    }
}

/// The page's sections, down its left.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Section {
    #[default]
    Tracks,
    Regions,
    Editing,
    Transport,
    Audio,
    Midi,
    Automation,
    Tabs,
    Recording,
    Plugins,
    Sounds,
    Models,
    Saving,
    About,
}

impl Section {
    /// Down the rail, by group: the studio, working, the library.
    const ALL: [Section; 14] = [
        Section::Audio,
        Section::Midi,
        Section::Recording,
        Section::Tracks,
        Section::Regions,
        Section::Editing,
        Section::Transport,
        Section::Automation,
        Section::Tabs,
        Section::Plugins,
        Section::Sounds,
        Section::Models,
        Section::Saving,
        Section::About,
    ];

    /// The group it's under on the rail.
    fn group(self) -> &'static str {
        match self {
            Section::Audio | Section::Midi | Section::Recording => "STUDIO",
            Section::Plugins | Section::Sounds | Section::Models | Section::Saving => "LIBRARY",
            Section::About => "PINA STUDIO",
            _ => "WORKING",
        }
    }

    /// What search finds it by, besides its name and its line.
    fn words(self) -> &'static str {
        match self {
            Section::Audio => {
                "output input device speakers headphones interface sample rate buffer latency test tone cores late dropout bluetooth playhead delay calibrate tap"
            }
            Section::Midi => "keyboard controller mpe bend",
            Section::Recording => "bit depth wav count in click metronome takes",
            Section::Plugins => "vst vst3 clap audio units folders scan",
            Section::Sounds => "synth presets packs maker",
            Section::Models => {
                "stems separate separation vocals drums bass download onnx ai lyrics whisper words"
            }
            Section::Saving => "recovery backup songs folder autosave",
            Section::Transport => "play stop follow playhead",
            Section::Editing => "middle c octave note names c3 c4 c5",
            Section::About => {
                "about credits author made by manyeya khutso siema version licence license mit gpl copyright open source notices thanks"
            }
            _ => "",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Section::Tracks => "Track headers",
            Section::Regions => "Regions & grid",
            Section::Editing => "Editing",
            Section::Transport => "Transport",
            Section::Audio => "Audio",
            Section::Midi => "MIDI",
            Section::Automation => "Automation",
            Section::Tabs => "Editor tabs",
            Section::Recording => "Recording",
            Section::Plugins => "Plugins",
            Section::Sounds => "Sounds & packs",
            Section::Models => "Stems & lyrics",
            Section::Saving => "Saving & files",
            Section::About => "About Pina Studio",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::Tracks => Icon::PanelLeft,
            Section::Regions => Icon::Grid,
            Section::Editing => Icon::Pointer,
            Section::Transport => Icon::Play,
            Section::Audio => Icon::Speaker,
            Section::Midi => Icon::MidiIn,
            Section::Automation => Icon::Fade,
            Section::Tabs => Icon::PanelBottom,
            Section::Recording => Icon::Record,
            Section::Plugins => Icon::Plug,
            Section::Sounds => Icon::Samples,
            Section::Models => Icon::Stack,
            Section::Saving => Icon::Folder,
            Section::About => Icon::Heart,
        }
    }

    fn note(self) -> &'static str {
        match self {
            Section::Tracks => {
                "What each track's header shows, and how tall a track is until you drag it."
            }
            Section::Regions => "How regions and the grid behind them look.",
            Section::Editing => "What the mouse does in the tracks, and what notes are called.",
            Section::Transport => "Playing and stopping.",
            Section::Audio => {
                "Where the sound goes and comes from, how fast, and how much room the computer has to make it."
            }
            Section::Midi => {
                "Your MIDI keyboards and controllers. An MPE controller's notes each bend, slide and press on their own; Pina finds out it's one from how it plays."
            }
            Section::Automation => {
                "Volume, pan, mute, sends and plugin parameters over time: the lanes under each track (A), and what moving an automated control does."
            }
            Section::Tabs => {
                "The tabs under the tracks. A hidden one's shortcut still opens it, its tab showing while it's open."
            }
            Section::Recording => "What recording writes, counting you in, and the click.",
            Section::Plugins => "Where Pina looks for plugins, and what it found.",
            Section::Sounds => "The synth's sounds and packs: who makes them, where they're kept.",
            Section::Models => {
                "The models that part a recording into its vocals, drums, bass and the rest, and that hear the words sung. Pina Studio comes with none: you choose which to download."
            }
            Section::Saving => "Your songs, and getting back what you lost.",
            Section::About => {
                "Who made Pina Studio, which licence covers what, and the open-source software it's built on."
            }
        }
    }
}

impl Bottom {
    /// The editor's tabs, in order.
    pub const ALL: [Bottom; 9] = [
        Bottom::PianoRoll,
        Bottom::Audio,
        Bottom::Steps,
        Bottom::Drums,
        Bottom::Sampler,
        Bottom::Modulation,
        Bottom::Score,
        Bottom::Takes,
        Bottom::Loops,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "piano_roll",
            Bottom::Audio => "audio",
            Bottom::Steps => "steps",
            Bottom::Drums => "drums",
            Bottom::Sampler => "sampler",
            Bottom::Modulation => "modulation",
            Bottom::Score => "score",
            Bottom::Takes => "takes",
            Bottom::Loops => "live_loops",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "Piano Roll",
            Bottom::Audio => "Audio",
            Bottom::Steps => "Step Sequencer",
            Bottom::Drums => "Drum Machine",
            Bottom::Sampler => "Sampler",
            Bottom::Modulation => "Modulation",
            Bottom::Score => "Score",
            Bottom::Takes => "Takes",
            Bottom::Loops => "Live Loops",
        }
    }

    /// Its name on the editor's own tab, short so they all fit.
    pub fn short(self) -> &'static str {
        match self {
            Bottom::Steps => "Steps",
            Bottom::Drums => "Drums",
            Bottom::Loops => "Loops",
            b => b.name(),
        }
    }

    fn what(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "A MIDI region's notes against a keyboard · P",
            Bottom::Audio => {
                "An audio region's file: trim, fades, gain, pitch, reverse; to the Sampler"
            }
            Bottom::Steps => "Patterns: the sounds as pads, a row of steps each",
            Bottom::Drums => "A kit's pads: play them, shape and swap their sounds",
            Bottom::Sampler => "The Sampler's zones, waveform and voice",
            Bottom::Modulation => "LFOs, steps, envelopes and followers, and what they move",
            Bottom::Score => "A MIDI region as notation · N",
            Bottom::Takes => "Takes recorded over each other, to comp",
            Bottom::Loops => "Cells to launch, a column a scene · L",
        }
    }
}

/// A switch's setting, read and written.
type Field = fn(&mut Settings) -> &mut bool;

/// Heights an automation lane can be.
const AUTO_HEIGHTS: [(&str, f32); 3] = [("Small", 44.0), ("Medium", 64.0), ("Large", 96.0)];

/// Heights a track can start at.
const HEIGHTS: [(&str, f32); 4] = [
    ("Small", 52.0),
    ("Medium", LANE_H),
    ("Large", 110.0),
    ("Huge", 160.0),
];

impl Workspace {
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_find.is_none() {
            let find = cx.new(|cx| crate::ui::text::TextInput::search("Search settings", cx));
            self._settings_typed = Some(cx.observe(&find, |ws, input, cx| {
                ws.settings_query = input.read(cx).text().trim().to_lowercase();
                cx.notify();
            }));
            self.settings_find = Some(find);
        }
        if self.settings_section == Section::Audio {
            self.read_outputs();
        }
        self.view = match self.view {
            View::Settings => View::Arrange,
            _ => View::Settings,
        };
        cx.notify();
    }

    /// Settings, on About Pina Studio (the app menu's About).
    pub fn open_about(&mut self, cx: &mut Context<Self>) {
        self.settings_section = Section::About;
        if self.view != View::Settings {
            self.open_settings(cx);
        }
        cx.notify();
    }

    fn set_tab_shown(&mut self, tab: Bottom, show: bool, cx: &mut Context<Self>) {
        let shown = Bottom::ALL
            .iter()
            .filter(|&&b| self.settings.shows(b))
            .count();
        if !show && shown <= 1 {
            self.status = Some("One editor tab at least stays.".into());
            cx.notify();
            return;
        }
        self.settings.hidden.retain(|h| h != tab.key());
        if !show {
            self.settings.hidden.push(tab.key().to_string());
        }
        self.settings.save();
        cx.notify();
    }

    /// What can be played through, as of now: asking the system is too slow to do every frame.
    fn read_outputs(&mut self) {
        self.read_devices();
        self.outputs = output_devices();
        self.buffers = buffer_sizes(self.settings.audio.device.as_deref());
    }

    /// Plays through `audio` from now on. If it can't, what was playing goes on, and says why.
    pub(crate) fn set_audio(&mut self, audio: AudioSetup, cx: &mut Context<Self>) {
        let done = self.play_through(&audio);
        match done {
            Ok(()) => {
                self.settings.audio = audio;
                self.settings.save();
                self.read_outputs();
                self.audio_error = None;
                self.apply_heard_later();
            }
            Err(e) => {
                self.audio_error = Some(e.clone());
                self.status = Some(format!("Couldn't change the audio output: {e}"));
            }
        }
        cx.notify();
    }

    /// The sound moved to `audio`'s output. One that can't run at the engine's rate (Bluetooth
    /// headphones, often only 44.1 kHz) gets the engine started again at one it can, the song
    /// and the playhead as they were.
    fn play_through(&mut self, audio: &AudioSetup) -> Result<(), String> {
        let silent = self.player.output_device().is_none();
        let rate =
            pina_engine::audio::engine::output_rate(self.session.project.sample_rate, audio)?;
        if silent || rate as f64 == self.player.rate() {
            return self.player.set_output(audio);
        }
        if self.recording.is_some() {
            return Err("it plays at another rate: stop recording first".to_string());
        }
        let at = self.player.position();
        let mut player = Player::start(&self.session.project, &self.session.dir, audio)?;
        // The playhead in seconds, at the new rate.
        player.locate((at as f64 * rate as f64 / self.player.rate()) as u64);
        self.player = player;
        self.apply_midi_inputs();
        let project = self.engine_project().into_owned();
        self.player.sync(&project)
    }

    /// Once a second: when the output it should play through isn't the one it does (the system's
    /// output changed, or the one picked was plugged in or pulled out), it moves there.
    pub fn follow_output(&mut self, now: Option<String>, cx: &mut Context<Self>) {
        let Some(playing) = self.player.output_device() else {
            return;
        };
        // The same output, a headset's call over: back to the rate it can play again.
        if now.as_ref() == Some(&playing) {
            let back = pina_engine::audio::engine::output_rate(
                self.session.project.sample_rate,
                &self.settings.audio,
            );
            if self.player.rate() < 32_000.0
                && self.recording.is_none()
                && back.is_ok_and(|r| r as f64 != self.player.rate())
            {
                self.set_audio(self.settings.audio.clone(), cx);
            }
            return;
        }
        // Tried and failed: not again every second, only once it's somewhere else.
        if now.is_none() || now.as_ref() == Some(&playing) || now == self.output_tried {
            return;
        }
        self.output_tried = now;
        self.set_audio(self.settings.audio.clone(), cx);
        if self.audio_error.is_none() {
            self.output_tried = None;
        }
    }

    /// `device` from now on, on a buffer it can do.
    pub(crate) fn set_output_device(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        let mut audio = self.settings.audio.clone();
        let can = buffer_sizes(device.as_deref());
        audio.buffer = audio.buffer.filter(|b| can.contains(b));
        audio.device = device;
        self.set_audio(audio, cx);
    }

    /// How the MIDI input named `name` is read, from now on and next time.
    fn set_midi_input(&mut self, name: &str, setup: InputSetup, cx: &mut Context<Self>) {
        let inputs = &mut self.settings.midi_inputs;
        match inputs.iter_mut().find(|i| i.name == name) {
            Some(i) => i.setup = setup,
            None => inputs.push(MidiInput {
                name: name.to_string(),
                setup,
            }),
        }
        self.settings.save();
        self.player.set_midi_input(name, setup);
        cx.notify();
    }

    /// Each MIDI input read as it was set to be, and the click as it was: when Pina Studio
    /// starts, a song opens, or the engine starts again.
    pub fn apply_midi_inputs(&self) {
        for i in &self.settings.midi_inputs {
            self.player.set_midi_input(&i.name, i.setup);
        }
        self.player.set_click(self.settings.click);
        self.gate_click();
        self.apply_heard_later();
        crate::editors::pianoroll::set_middle_c(self.settings.middle_c);
    }

    /// The click let sound: always, or (set to click only while recording) while recording.
    pub fn gate_click(&self) {
        self.player
            .set_click_gate(!self.settings.click_only_recording || self.recording.is_some());
    }

    fn flip(&mut self, field: Field, cx: &mut Context<Self>) {
        let v = field(&mut self.settings);
        *v = !*v;
        self.settings.save();
        cx.notify();
    }

    /// A section on the rail.
    fn rail_item(&self, s: Section, section: Section, cx: &mut Context<Self>) -> Div {
        let on = s == section;
        div()
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .rounded(px(8.))
            .text_size(px(12.5))
            .cursor_pointer()
            .when(on, |d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
            .when(!on, |d| d.text_color(rgb(TEXT_2)).hover(|d| d.bg(rgb(OFF))))
            .debug_selector(move || format!("settings-{}", s.name()))
            .child(icon(s.icon(), 14.).text_color(rgb(if on { TEXT } else { TEXT_3 })))
            .child(s.name())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.settings_section = s;
                    if s == Section::Audio {
                        ws.read_outputs();
                    }
                    cx.notify();
                }),
            )
    }

    pub fn settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // The input's listened to only while Audio shows.
        if self.settings_section != Section::Audio {
            self.input_meter = None;
        }
        let section = self.settings_section;
        let row = |title: &'static str,
                   what: &'static str,
                   on: bool,
                   name: String,
                   f: Toggle,
                   cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(14.))
                .py(px(11.))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(OFF)))
                .debug_selector(move || name.clone())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child(what),
                        ),
                )
                .child(switch(on))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| f(ws, cx)),
                )
        };
        let card = |rows: Vec<Div>| {
            div()
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .overflow_hidden()
                .children(rows)
        };
        // A switch for one of the settings, by its name in the file.
        let s = self.settings.clone();
        let flag = |title: &'static str,
                    what: &'static str,
                    key: &'static str,
                    field: Field,
                    cx: &mut Context<Self>| {
            let on = *field(&mut s.clone());
            row(
                title,
                what,
                on,
                format!("setting-{key}"),
                Box::new(move |ws, cx| ws.flip(field, cx)),
                cx,
            )
        };
        // A choice of a few, beside its title.
        let choose = |title: &'static str, what: &'static str, segs: Vec<Div>| {
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(14.))
                .py(px(12.))
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child(what),
                        ),
                )
                .child(tray().p(px(2.)).rounded(px(8.)).children(segs))
        };
        let body: Vec<Div> = match section {
            Section::Tracks => {
                let height = self.settings.track_height;
                let heights = HEIGHTS.map(|(label, h)| {
                    seg(label, (height - h).abs() < 0.5, 26.)
                        .debug_selector(move || format!("setting-height-{label}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.settings.track_height = h;
                                ws.settings.save();
                                cx.notify();
                            }),
                        )
                });
                vec![
                    card(vec![
                        flag(
                            "Numbers",
                            "01, 02… beside each name",
                            "header_numbers",
                            |s| &mut s.header_numbers,
                            cx,
                        ),
                        flag(
                            "Icons",
                            "What each track is, at a glance (click one for its colour and icon)",
                            "header_icons",
                            |s| &mut s.header_icons,
                            cx,
                        ),
                        flag(
                            "What it plays",
                            "Its instrument, under its name",
                            "header_kind",
                            |s| &mut s.header_kind,
                            cx,
                        ),
                        flag(
                            "Level meters",
                            "The LEDs beside M S R",
                            "header_meter",
                            |s| &mut s.header_meter,
                            cx,
                        ),
                        flag(
                            "Fader and pan",
                            "Its volume and pan, under M S R",
                            "header_strip",
                            |s| &mut s.header_strip,
                            cx,
                        ),
                        flag(
                            "Record buttons",
                            "R, to arm a track",
                            "header_record",
                            |s| &mut s.header_record,
                            cx,
                        ),
                    ]),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .px(px(14.))
                        .py(px(12.))
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Track height"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(rgb(TEXT_3))
                                        .child("Every track's, until you drag its bottom edge"),
                                ),
                        )
                        .child(tray().p(px(2.)).rounded(px(8.)).children(heights)),
                ]
            }
            Section::Regions => vec![card(vec![
                flag(
                    "Names",
                    "Each region's name across its top",
                    "region_names",
                    |s| &mut s.region_names,
                    cx,
                ),
                flag(
                    "Contents",
                    "Its notes, steps or waveform drawn inside it",
                    "region_contents",
                    |s| &mut s.region_contents,
                    cx,
                ),
                flag(
                    "Bright colours",
                    "Regions filled in their track's colour more strongly",
                    "region_bright",
                    |s| &mut s.region_bright,
                    cx,
                ),
                flag(
                    "Bar shading",
                    "Every other four bars a shade lighter",
                    "bar_shading",
                    |s| &mut s.bar_shading,
                    cx,
                ),
                flag(
                    "Beat lines",
                    "A line each beat, not just each bar",
                    "beat_lines",
                    |s| &mut s.beat_lines,
                    cx,
                ),
            ])],
            Section::Editing => vec![
                {
                    let middle = self.settings.middle_c;
                    let names = [3i8, 4, 5].map(|o| {
                        seg(format!("C{o}"), middle == o, 26.)
                            .debug_selector(move || format!("setting-middle-c-{o}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.settings.middle_c = o;
                                    ws.settings.save();
                                    crate::editors::pianoroll::set_middle_c(o);
                                    cx.notify();
                                }),
                            )
                    });
                    choose(
                        "Middle C is called",
                        "MIDI note 60, 262 Hz: C4 as most learn it; C3 or C5 as some apps name it",
                        names.into(),
                    )
                },
                card(vec![
                    flag(
                        "Loop zone",
                        "The top half of a region's end loops it; off, the whole end trims",
                        "loop_zone",
                        |s| &mut s.loop_zone,
                        cx,
                    ),
                    flag(
                        "Double-click writes",
                        "A double-click in an empty lane makes a MIDI region and opens it in the piano roll",
                        "double_click_writes",
                        |s| &mut s.double_click_writes,
                        cx,
                    ),
                ]),
            ],
            Section::Transport => vec![card(vec![
                flag(
                    "Follow the playhead",
                    "The tracks page along as the song plays, a screen at a time",
                    "follow",
                    |s| &mut s.follow_playhead,
                    cx,
                ),
                flag(
                    "Return when stopping",
                    "Stopping goes back to where play started",
                    "return_on_stop",
                    |s| &mut s.return_on_stop,
                    cx,
                ),
            ])],
            Section::Audio => self.audio_page(cx),
            Section::Recording => self.recording_page(cx),
            Section::Plugins => self.plugin_settings(cx),
            Section::Sounds => self.sound_settings(cx),
            Section::Models => self.models_page(cx),
            Section::Saving => self.saving_page(cx),
            Section::About => self.about_page(cx),
            Section::Midi => {
                let inputs = self.player.midi_inputs();
                let input = |i: pina_engine::audio::midi::InputState, cx: &mut Context<Self>| {
                    let name = i.name.clone();
                    let status = match (i.setup.mpe, i.found) {
                        (false, _) => {
                            "MPE off: every channel's bend and pressure are everyone's".to_string()
                        }
                        (true, true) => format!(
                            "MPE: each note bends, slides and presses on its own, up to {} semitones",
                            i.setup.bend
                        ),
                        (true, false) => {
                            "MPE when it plays like it: notes on their own channels".to_string()
                        }
                    };
                    let bends: Vec<Div> = NOTE_BENDS
                        .into_iter()
                        .map(|st| {
                            let (n, setup) = (
                                name.clone(),
                                InputSetup {
                                    bend: st,
                                    ..i.setup
                                },
                            );
                            seg(format!("{st}"), i.setup.bend == st, 24.)
                                .debug_selector(move || format!("setting-midi-bend-{st}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.set_midi_input(&n, setup, cx)
                                    }),
                                )
                        })
                        .collect();
                    let flipped = InputSetup {
                        mpe: !i.setup.mpe,
                        ..i.setup
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .px(px(14.))
                        .py(px(12.))
                        .border_b_1()
                        .border_color(rgb(LINE_SOFT))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(14.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.))
                                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(i.name.clone()))
                                        .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(status)),
                                )
                                .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child("MPE"))
                                .child(
                                    switch(i.setup.mpe)
                                        .debug_selector(|| "setting-midi-mpe".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, _, _, cx| ws.set_midi_input(&name, flipped, cx)),
                                        ),
                                ),
                        )
                        .when(i.setup.mpe, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.5))
                                            .text_color(rgb(TEXT_3))
                                            .child("How far a note bends, in semitones, until the controller says"),
                                    )
                                    .child(tray().p(px(2.)).rounded(px(8.)).children(bends)),
                            )
                        })
                };
                match inputs.is_empty() {
                    true => vec![card(vec![
                        div()
                            .px(px(14.))
                            .py(px(14.))
                            .text_size(px(12.5))
                            .text_color(rgb(TEXT_3))
                            .child("No MIDI inputs. Plug a keyboard or controller in, then open Pina Studio again."),
                    ])],
                    false => vec![card(inputs.into_iter().map(|i| input(i, cx)).collect())],
                }
            }
            Section::Automation => {
                let height = self.settings.auto_height;
                let heights = AUTO_HEIGHTS
                    .map(|(label, h)| {
                        seg(label, (height - h).abs() < 0.5, 26.)
                            .debug_selector(move || format!("setting-auto-height-{label}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.settings.auto_height = h;
                                    ws.settings.save();
                                    cx.notify();
                                }),
                            )
                    })
                    .into_iter()
                    .collect();
                let stopped = self.settings.stopped;
                let stops = Stopped::ALL
                    .map(|st| {
                        seg(st.name(), st == stopped, 26.)
                            .debug_selector(move || format!("setting-stopped-{}", st.name()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.settings.stopped = st;
                                    ws.settings.save();
                                    cx.notify();
                                }),
                            )
                    })
                    .into_iter()
                    .collect();
                vec![
                    choose(
                        "Lane height",
                        "Each automation lane's, under its track",
                        heights,
                    ),
                    choose(
                        "Moved while stopped",
                        "An automated control moved with the song stopped moves its whole line, sets a point at the playhead, or changes it until you play",
                        stops,
                    ),
                    card(vec![
                        flag(
                            "Snap points",
                            "Points drawn or dragged go to the grid (hold ⌘ to place one freely)",
                            "snap_automation",
                            |s| &mut s.snap_automation,
                            cx,
                        ),
                        flag(
                            "Thin what's recorded",
                            "After a pass, and after the pencil, points that change nothing are taken out",
                            "thin_recorded",
                            |s| &mut s.thin_recorded,
                            cx,
                        ),
                        flag(
                            "Write turns to Latch",
                            "After a pass in Write, the track goes to Latch, so the next doesn't wipe it",
                            "write_to_latch",
                            |s| &mut s.write_to_latch,
                            cx,
                        ),
                        flag(
                            "Automation moves with regions",
                            "Moving or copying a region takes the automation under it along",
                            "carry_automation",
                            |s| &mut s.carry_automation,
                            cx,
                        ),
                    ]),
                ]
            }
            Section::Tabs => vec![card(
                Bottom::ALL
                    .into_iter()
                    .map(|tab| {
                        let on = self.settings.shows(tab);
                        row(
                            tab.name(),
                            tab.what(),
                            on,
                            format!("setting-tab-{}", tab.key()),
                            Box::new(move |ws, cx| ws.set_tab_shown(tab, !on, cx)),
                            cx,
                        )
                    })
                    .collect(),
            )],
        };
        // Narrowed by what's typed: a section's name, its line, or what it holds.
        let words: Vec<String> = self
            .settings_query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let shown: Vec<Section> = Section::ALL
            .into_iter()
            .filter(|s| {
                let all = format!("{} {} {}", s.name(), s.note(), s.words()).to_lowercase();
                words.iter().all(|w| all.contains(w))
            })
            .collect();
        let mut last_group = "";
        let mut rail: Vec<Div> = Vec::new();
        for s in shown {
            if s.group() != last_group {
                last_group = s.group();
                rail.push(
                    div()
                        .px(px(10.))
                        .pt(px(12.))
                        .pb(px(4.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_4))
                        .child(last_group),
                );
            }
            rail.push(self.rail_item(s, section, cx));
        }
        let search = self.settings_find.clone();

        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(
                div()
                    .w(px(232.))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(px(14.))
                    .bg(rgb(SURFACE))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .px(px(8.))
                            .pb(px(14.))
                            .child(icon(Icon::Settings, 17.).text_color(rgb(TEXT_2)))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            ),
                    )
                    .children(search.map(|f| {
                        div()
                            .pb(px(4.))
                            .child(crate::pages::genres::field_box(Some(f), "settings-search"))
                    }))
                    .children(rail),
            )
            .child(
                div()
                    .id("settings")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    // The page as tall as what's on it, so it scrolls (stretched, it never did).
                    .items_start()
                    .child(
                        div()
                            .w(px(if section == Section::Audio {
                                1080.
                            } else {
                                660.
                            }))
                            .py(px(30.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .flex_1()
                                            // Narrower than its words: they wrap, Done stays.
                                            .min_w(px(0.))
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.))
                                            .child(
                                                div()
                                                    .text_size(px(20.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(section.name()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(TEXT_3))
                                                    .child(section.note()),
                                            ),
                                    )
                                    .child(
                                        button("Done", false)
                                            .debug_selector(|| "settings-done".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    ws.view = View::Arrange;
                                                    cx.notify();
                                                }),
                                            ),
                                    ),
                            )
                            .children(body),
                    ),
            )
    }
}
