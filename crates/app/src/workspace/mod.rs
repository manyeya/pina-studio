//! The main window, over one open project: the control bar; then either the arrangement (the
//! inspector, the tracks, and an editor under them) or the mixer; and a status bar. Every edit
//! checkpoints the session (for undo), changes the project, and syncs the player.

mod controls;
mod drag;
mod editing;
mod geometry;
mod keys;
mod naming;
mod panels;
mod params;
mod saving;
mod status;
mod transport;

pub use controls::*;
pub use drag::*;
pub use editing::*;
pub use keys::*;
pub use naming::*;
pub use panels::*;
pub use params::*;
pub(crate) use saving::*;
pub use status::*;

pub mod context;
pub mod editor;
pub mod inspector;
pub mod record;
pub mod tools;
pub mod topbar;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    path::Path,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

pub use crate::workspace::record::Recording;

use gpui::{
    App, Bounds, Context, DispatchPhase, DragMoveEvent, Entity, ExternalPaths, FocusHandle,
    Focusable, FontWeight, KeyBinding, KeyDownEvent, KeyUpEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, PromptLevel, ScrollDelta, ScrollWheelEvent, Subscription, Window, actions,
    canvas, div, prelude::*, px, rgb,
};
use pina_engine::{
    audio::engine::LiveNote,
    audio::player::Player,
    plugin::scan::{self, PluginInfo},
    song::project::{
        At, AutoMode, Comp, Content, Cycle, Eq, Instrument, PPQ, Param, Place, Project,
        RECOVERY_DIR, STATES_DIR, Track, Which, value_at,
    },
    song::session::Session,
};

use crate::{
    editors::pianoroll::RollOp,
    mix::strip::{self, Channel},
    ui::menu::Menu,
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::context::RegionOp,
};

/// What clicking in the lanes and the piano roll does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Pointer,
    Pencil,
    Scissors,
    Eraser,
    Text,
    Glue,
    Solo,
    Mute,
    Zoom,
    Fade,
    Flex,
}

/// What edits snap to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Snap {
    /// Finer as you zoom in.
    Smart,
    Bar,
    Beat,
    Eighth,
    Sixteenth,
    Off,
}

impl Snap {
    pub const ALL: [Snap; 6] = [
        Snap::Smart,
        Snap::Bar,
        Snap::Beat,
        Snap::Eighth,
        Snap::Sixteenth,
        Snap::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Snap::Smart => "Smart",
            Snap::Bar => "Bar",
            Snap::Beat => "Beat",
            Snap::Eighth => "1/8",
            Snap::Sixteenth => "1/16",
            Snap::Off => "Off",
        }
    }
}

/// Which channels the mixer shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Channels {
    All,
    Tracks,
    Buses,
}

/// An edit an agent made through the socket.
#[derive(Clone, Debug)]
pub struct AgentEdit {
    pub at: Instant,
    /// What it did, in words: "set notes · Bass, region 1".
    pub what: String,
    pub track: Option<usize>,
    pub region: Option<usize>,
}

impl AgentEdit {
    /// How long ago, in words: "now", "12s", "3m".
    pub fn ago(&self) -> String {
        let s = self.at.elapsed().as_secs();
        match s {
            0..=4 => "now".into(),
            5..=59 => format!("{s}s ago"),
            _ => format!("{}m ago", s / 60),
        }
    }
}

/// How long ago, in words: "just now", "4 minutes ago", "2 hours ago", "3 days ago".
fn ago(at: SystemTime) -> String {
    let s = at.elapsed().map_or(0, |d| d.as_secs());
    let (n, unit) = match s {
        0..60 => return "just now".into(),
        60..3_600 => (s / 60, "minute"),
        3_600..86_400 => (s / 3_600, "hour"),
        _ => (s / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// The piano roll's settings for its quantize and tools.
#[derive(Clone, Copy, Debug)]
pub struct RollSettings {
    /// Ticks.
    pub grid: u64,
    pub strength: f32,
    /// 50 straight to 75.
    pub swing: f32,
    /// The track whose notes show faintly behind, by id.
    pub ghost: Option<u64>,
    /// Shading the rows out of the song's key.
    pub scale: bool,
    /// Notes drawn and moved land on the key's notes.
    pub in_key: bool,
    /// A chord stamped for each note.
    pub stamp: crate::editors::pianoroll::Stamp,
    /// Notes coloured by velocity, cool to hot; else the track's colour.
    pub by_velocity: bool,
    /// The length a new note is drawn at: the last note picked or stretched (0, the grid).
    pub length: u64,
    /// How tall a pitch's row is, as zoomed.
    pub key_h: f32,
    /// What the lane under the notes shows and edits.
    pub lane: crate::editors::pianoroll::Lane,
}

/// A level meter's reading: falling back gently, and its peak held a moment.
#[derive(Clone, Copy, Default)]
pub struct Level {
    pub now: [f32; 2],
    pub held: [f32; 2],
    since: Option<Instant>,
}

impl Level {
    fn update(&mut self, peak: [f32; 2]) {
        for (now, peak) in self.now.iter_mut().zip(peak) {
            *now = peak.max(*now * 0.85);
        }
        let loudest = self.now[0].max(self.now[1]);
        let expired = self
            .since
            .is_none_or(|t| t.elapsed() > Duration::from_millis(1500));
        if expired || loudest >= self.held[0].max(self.held[1]) {
            self.held = self.now;
            self.since = Some(Instant::now());
        }
    }

    pub fn moving(&self) -> bool {
        self.now.iter().chain(&self.held).any(|&l| l > 1e-4)
    }
}

pub struct Workspace {
    pub session: Session,
    pub player: Player,
    focus: FocusHandle,
    /// The selected track, and region.
    pub track: usize,
    /// When ⌘- or ⇧-clicks picked several tracks, all of them by id (the selected one too).
    pub tracks_picked: Vec<u64>,
    pub region: Option<At>,
    /// Horizontal zoom, and scroll in pixels.
    pub px_per_beat: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub view: View,
    pub bottom: Option<Bottom>,
    pub show_inspector: bool,
    /// The compact sample browser, beside the tracks.
    pub show_browser: bool,
    /// The Agent panel shows beside the tracks, where the Samples panel would.
    pub show_agent: bool,
    /// The Browser is open to pick sounds, instruments or effects for the agent.
    pub picking: bool,
    /// The conversation with Claude in it.
    pub chat: crate::agent::chat::Chat,
    /// The Browser: devices, presets, sounds and songs.
    pub catalog: crate::sounds::catalog::Catalog,
    /// Every song, for the Songs page and the menus.
    pub songs: Vec<crate::pages::projects::Song>,
    /// The Playbooks page's genres, and the one being edited.
    pub genres: crate::pages::genres::Genres,
    /// The Song Study page's songs, and the one shown.
    pub studies: crate::pages::study::Studies,
    /// How many times the song has changed, for what's worked out from it (the critic).
    pub edits: u64,
    /// A song to open, once the next frame has the window to ask about saving with.
    pub opening: Option<std::path::PathBuf>,
    /// A song action from a menu, for the next frame.
    pub song_pending: Option<crate::ui::menu::MenuAction>,
    /// The Songs page's new-song settings, search and songs picked, once it's been shown.
    pub home: Option<crate::pages::projects::Home>,
    /// A song being renamed on its card.
    pub renaming: Option<crate::pages::projects::Renaming>,
    /// Each track's icon and colour, as what it plays says or as chosen (`looks.rs`).
    pub looks: Vec<crate::ui::looks::Shown>,
    /// The analysis of the song's audio the looks are worked out from.
    pub heard: crate::ui::looks::Heard,
    /// The colour and icon picker, open on a track.
    pub picker: Option<crate::ui::looks::Picker>,
    /// A colour or icon being tried on a track, under the pointer in the picker.
    pub trying: Option<(usize, crate::ui::looks::Trying)>,
    pub tool: Tool,
    pub snap: Snap,
    /// Playing the selected region alone, looped (pattern mode); and the region it
    /// plays, with the start and length it had then.
    pub pattern_mode: bool,
    pub pattern: Option<(At, u64, u64)>,
    /// Whether choosing a key moves the music into it, or only names it.
    pub key_moves_notes: bool,
    /// The channel the mixer's side panel shows.
    pub channel: Channel,
    pub channels: Channels,
    /// The mixer's rows: EQ, inserts and sends.
    pub mixer_rows: [bool; 3],
    /// The notes picked in the piano roll (or score), by index in the region shown; the last
    /// the one clicked.
    pub picked: Vec<usize>,
    /// Notes copied in the piano roll, their starts from the first's.
    pub clip: Vec<pina_engine::song::project::Note>,
    /// What a piano roll drag carries: the notes picked up, as they began; the notes picked
    /// before a rubber band; the pitches the pencil sounds.
    pub held: Vec<(usize, pina_engine::song::project::Note)>,
    pub held_picks: Vec<usize>,
    pub painting: Vec<u8>,
    /// The pitch under the mouse in the piano roll.
    pub roll_hover: Option<u8>,
    /// Where the lane under the piano roll was drawn last frame.
    pub lane_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Bounce sheet, when it's open (and what it's bouncing).
    pub export: Option<crate::mix::export::Export>,
    /// The track headers' column and the inspector, as wide as they've been dragged.
    pub header_w: f32,
    pub inspector_w: f32,
    /// Regions copied (⌘C on the tracks), to paste at the playhead.
    pub region_clip: Option<pina_engine::song::project::Region>,
    /// A MIDI or pattern region's contents as they were when its time-stretch drag began.
    pub stretching: Option<pina_engine::song::project::Content>,
    /// ⇧ held while dragging a fader or knob: it moves a fifth as far.
    pub fine: bool,
    /// Modulation: each track's modulators' values and their recent ones; the knobs a
    /// modulator can be dropped on, where they were drawn; the steps' editors; the modulator
    /// being dragged, where; the one picked (knobs it moves set its depth); a clock for those
    /// running free.
    pub mod_values: HashMap<u64, [f32; pina_engine::dsp::modulate::MAX_MODS]>,
    pub mod_trace: HashMap<(u64, usize), std::collections::VecDeque<f32>>,
    pub mod_targets: crate::editors::modulation::DropTargets,
    /// Plugins' parameter names, by track and plugin (`param_names`).
    param_names: RefCell<HashMap<(u64, Which), (std::time::Instant, ParamNames)>>,
    pub mod_steps: crate::editors::modulation::StepsAt,
    pub mod_drag_at: Option<Point<f32>>,
    pub mod_edit: Option<(usize, usize)>,
    /// A modulator waiting to be mapped to whatever's turned next in a plugin's own window.
    pub mod_learn: Option<crate::editors::modulation::Learn>,
    pub mod_clock: Instant,
    /// The Zoom tool's drag, where it began and is now (window x); what the Solo tool's hold
    /// changed, to put back (pattern mode, its region, playing, the selection).
    pub zoom_span: Option<(f32, f32)>,
    #[allow(clippy::type_complexity)]
    pub solo_restore: Option<(bool, Option<(At, u64, u64)>, bool, Option<At>)>,
    /// The Binaural Panner, open on a track, where; where its view looks from (degrees round
    /// and down); and where the view is on screen.
    pub panner: Option<(usize, Point<Pixels>)>,
    pub orbit: (f32, f32),
    pub panner_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The last control a sideways swipe moved, and when: one undo for each swipe.
    pub swiped: Option<(Name, std::time::Instant)>,
    /// A track's colour and icon picker waiting for the next frame (from a menu), and where.
    pub pending_look: Option<(usize, Point<Pixels>)>,
    /// The New Track palette, when it's open; and the presets it searches.
    pub adder: Option<crate::arrange::adder::Adder>,
    pub sounds: crate::arrange::adder::Sounds,
    /// The step sequencer's view: its edit mode, picked row and steps, page, panes, input.
    pub steps: crate::editors::steps::StepsView,
    /// The Plugins page: its filters, the facts about each plugin, pictures being taken.
    pub plugin_page: crate::sounds::plugins::PluginPage,
    /// What the window shows, kept for every song (Settings, ⌘,); the page's section shown;
    /// where play last started (to go back to on stop, if Settings say).
    pub settings: crate::pages::settings::Settings,
    pub settings_section: crate::pages::settings::Section,
    pub play_from: u64,
    /// The Sampler editor: the zone picked and the waveforms read; where its waveform was drawn.
    pub sampler_ui: crate::editors::sampler::SamplerUi,
    pub sampler_wave_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The piano roll's vertical scroll, and where its grid was drawn last frame.
    pub roll_scroll_y: f32,
    pub roll_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub roll: RollSettings,
    pub show_automation: bool,
    /// How tall the editor under the tracks stands, as dragged.
    pub editor_h: f32,
    /// The compact sample browser's width, as dragged.
    pub panel_w: f32,
    /// The parameters each track's automation lanes show, top to bottom, by track id.
    pub auto_lanes: HashMap<u64, Vec<Param>>,
    /// Automation points selected, the lane last worked on, points copied, and a lane as it was
    /// when a drag on it began.
    pub auto_picked: Option<crate::arrange::automation::Picked>,
    pub auto_focus: Option<(u64, Param)>,
    pub auto_clip: Option<pina_engine::song::project::AutoClip>,
    pub auto_grab: Vec<pina_engine::song::project::AutoPoint>,
    /// Controls a hand has taken over from their automation, those being trimmed, and those
    /// recorded this pass (to thin when it ends).
    pub taken: HashMap<(u64, Param), crate::arrange::automation::Live>,
    pub touches: HashMap<(u64, Param), crate::arrange::automation::Touch>,
    pub written: std::collections::HashSet<(u64, Param)>,
    /// The lines Touch is recording over, as they were.
    pub recorded: HashMap<(u64, Param), Vec<pina_engine::song::project::AutoPoint>>,
    /// Whether the song was playing last frame, to tell a pass starting or ending.
    pub was_playing: bool,
    /// What each group of aliases played after the last edit, to tell which one changed.
    links: HashMap<u64, Content>,
    /// Automation being recorded (Touch or Latch), by track id and parameter: the last tick
    /// written, the control's value, and whether it's still held.
    pub writing: HashMap<(u64, Param), (u64, f32, bool)>,
    pub recording: Option<Recording>,
    /// Musical typing's octave shift, when it's on; and the notes its held keys play.
    pub typing: Option<i8>,
    typed: HashMap<String, u8>,
    /// An open pop-up menu.
    pub menu: Option<Menu>,
    /// A name being typed.
    pub naming: Option<Naming>,
    /// Something to start naming on the next frame (from a menu, which has no window to hand).
    pub pending_name: Option<Name>,
    /// Which track records and hears the MIDI keyboard.
    pub arm: Arm,
    /// Give the workspace back the keyboard on the next frame (after naming, or searching).
    pub refocus: bool,
    /// Live Loops launched, by track id: the scene (None, stopped) and the sample it begins at.
    pub live: HashMap<u64, (Option<usize>, u64)>,
    /// The plugins installed, once the scan is done.
    pub plugins: Option<Vec<PluginInfo>>,
    pub drag: Option<Drag>,
    /// Where files dragged in from the Finder are, and which, while they're over the window.
    pub file_drag: Option<(Point<f32>, Vec<std::path::PathBuf>)>,
    /// Meter readings by track or aux id; None is the master.
    pub levels: HashMap<Option<u64>, Level>,
    /// Compressor gain reduction, in dB, by channel id; None is the master.
    pub reductions: HashMap<Option<u64>, f32>,
    /// How busy the audio thread is: 1 is all of its time; and when that was last read.
    pub load: f32,
    load_read: Instant,
    pub status: Option<String>,
    /// What agents have done, newest last.
    pub agent: Vec<AgentEdit>,
    /// The parameters Smart Controls shows for each track's instrument plugin, by track id.
    pub smart: HashMap<u64, (String, Vec<(u32, String)>)>,
    pub exporting: bool,
    /// Where the lanes and the EQ graph were drawn last frame, for turning pointer positions into
    /// ticks and frequencies.
    pub lanes_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Channel EQ's window, when it's open; and bands copied from it.
    pub eq_view: Option<crate::mix::eq::EqView>,
    pub eq_clipboard: Vec<pina_engine::song::project::EqBand>,
    /// The window's size, for what fits.
    pub viewport_h: f32,
    pub viewport_w: f32,
    /// Where each slider was drawn last frame.
    pub slider_bounds: Rc<RefCell<HashMap<Slider, Bounds<Pixels>>>>,
    /// The sample browser: the library, what's searched for, and what's found.
    pub browser: crate::sounds::browser::Browser,
    title: String,
}

pub fn pt(p: Point<Pixels>) -> Point<f32> {
    p.map(f32::from)
}

impl Workspace {
    pub fn new(
        session: Session,
        mut player: Player,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let status = player.sync(&session.project).err();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |ws, cx| ws.should_close(window, cx))
                .unwrap_or(true)
        });
        let links = session.project.link_contents();
        let browser = crate::sounds::browser::Browser::new(cx);
        let chat = crate::agent::chat::Chat::new(cx);
        let catalog = crate::sounds::catalog::Catalog::new(cx);
        // A song not saved yet opens on the Songs page.
        let view = match session.on_disk {
            true => View::Arrange,
            false => View::Songs,
        };
        // The CPU load and meters refresh while nothing else redraws.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(KEEP_EVERY).await;
                if this.update(cx, |ws, _| ws.keep_edits()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut ws = Workspace {
            session,
            player,
            focus,
            track: 0,
            tracks_picked: Vec::new(),
            region: None,
            px_per_beat: 48.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            view,
            bottom: None,
            show_inspector: true,
            show_browser: false,
            show_agent: false,
            picking: false,
            tool: Tool::Pointer,
            snap: Snap::Smart,
            pattern_mode: false,
            pattern: None,
            key_moves_notes: true,
            channel: Channel::Track(0),
            channels: Channels::All,
            mixer_rows: [true; 3],
            picked: Vec::new(),
            clip: Vec::new(),
            held: Vec::new(),
            held_picks: Vec::new(),
            painting: Vec::new(),
            roll_hover: None,
            lane_bounds: Rc::new(Cell::new(Bounds::default())),
            export: None,
            header_w: HEADER_W,
            inspector_w: INSPECTOR_W,
            region_clip: None,
            stretching: None,
            fine: false,
            panner: None,
            mod_values: HashMap::new(),
            mod_trace: HashMap::new(),
            mod_targets: Default::default(),
            param_names: Default::default(),
            mod_steps: Default::default(),
            mod_drag_at: None,
            mod_edit: None,
            mod_learn: None,
            mod_clock: Instant::now(),
            zoom_span: None,
            solo_restore: None,
            orbit: crate::editors::binaural::ORBIT,
            panner_bounds: Default::default(),
            swiped: None,
            pending_look: None,
            adder: None,
            sounds: Default::default(),
            steps: Default::default(),
            plugin_page: crate::sounds::plugins::PluginPage::new(cx),
            settings: crate::pages::settings::Settings::load(),
            settings_section: Default::default(),
            play_from: 0,
            sampler_ui: Default::default(),
            sampler_wave_bounds: Rc::new(Cell::new(Bounds::default())),
            roll_scroll_y: crate::editors::pianoroll::scroll_to(72),
            roll_bounds: Rc::new(Cell::new(Bounds::default())),
            roll: RollSettings {
                grid: PPQ / 4,
                strength: 1.0,
                swing: 50.0,
                ghost: None,
                scale: true,
                in_key: false,
                stamp: crate::editors::pianoroll::Stamp::Note,
                by_velocity: false,
                length: 0,
                key_h: crate::editors::pianoroll::KEY_H,
                lane: crate::editors::pianoroll::Lane::Velocity,
            },
            show_automation: false,
            editor_h: EDITOR_H,
            panel_w: crate::sounds::browser::PANEL_W,
            auto_lanes: HashMap::new(),
            auto_picked: None,
            auto_focus: None,
            auto_clip: None,
            auto_grab: Vec::new(),
            taken: HashMap::new(),
            touches: HashMap::new(),
            written: Default::default(),
            recorded: HashMap::new(),
            was_playing: false,
            links,
            writing: HashMap::new(),
            recording: None,
            typing: None,
            typed: HashMap::new(),
            menu: None,
            naming: None,
            pending_name: None,
            arm: Arm::Selected,
            refocus: false,
            plugins: None,
            live: HashMap::new(),
            drag: None,
            file_drag: None,
            levels: HashMap::new(),
            reductions: HashMap::new(),
            load: 0.0,
            load_read: Instant::now(),
            status,
            agent: Vec::new(),
            smart: HashMap::new(),
            exporting: false,
            lanes_bounds: Rc::new(Cell::new(Bounds::default())),
            eq_view: None,
            eq_clipboard: Vec::new(),
            slider_bounds: Rc::new(RefCell::new(HashMap::new())),
            viewport_h: 900.0,
            viewport_w: 1440.0,
            browser,
            chat,
            catalog,
            songs: Vec::new(),
            genres: Default::default(),
            studies: Default::default(),
            edits: 0,
            opening: None,
            song_pending: None,
            home: None,
            renaming: None,
            looks: Vec::new(),
            heard: Default::default(),
            picker: None,
            trying: None,
            title: String::new(),
        };
        ws.refresh_looks(cx);
        ws.refresh_songs(cx);
        // For screenshots and docs: PINA_PAGE=playbooks[:genre[@scroll]] (or study, agent)
        // opens on that page.
        if let Ok(page) = std::env::var("PINA_PAGE") {
            match page.split_once(':').unwrap_or((page.as_str(), "")) {
                ("playbooks", rest) => {
                    let (genre, down) = rest.split_once('@').unwrap_or((rest, "0"));
                    ws.open_genres((!genre.is_empty()).then_some(genre), cx);
                    let down: f32 = down.parse().unwrap_or(0.0);
                    ws.genres.scroll.set_offset(gpui::point(px(0.), px(-down)));
                }
                ("study", _) => ws.open_study(cx),
                ("agent", rest) => {
                    ws.show_agent = true;
                    ws.chat.critic.open = rest == "critic";
                }
                _ => {}
            }
        }
        ws
    }

    /// The color a track's regions, header and strip use.
    pub fn color(&self, track: usize) -> u32 {
        self.shown(track).color
    }

    /// What the engine plays: the project, or in pattern mode just the selected region, looped.
    fn engine_project(&self) -> Cow<'_, Project> {
        let mut p = self.pattern_project();
        // Controls a hand has taken over play as it has them.
        if !self.taken.is_empty() {
            self.without_live(p.to_mut());
        }
        p
    }

    fn pattern_project(&self) -> Cow<'_, Project> {
        let project = &self.session.project;
        let Some(((t, r), ..)) = self.pattern.filter(|&((t, r), ..)| {
            self.pattern_mode && project.tracks.get(t).is_some_and(|t| r < t.regions.len())
        }) else {
            return Cow::Borrowed(project);
        };
        let mut p = project.clone();
        let region = &project.tracks[t].regions[r];
        p.cycle = Some(Cycle {
            start: region.start,
            end: region.start + region.length,
            off: false,
        });
        for (i, track) in p.tracks.iter_mut().enumerate() {
            track.solo = false;
            track.mute = i != t;
            if i == t {
                let keep = track.regions[r].clone();
                track.regions = vec![keep];
            }
        }
        Cow::Owned(p)
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The knobs a modulator can be dropped on, as this frame draws them.
        self.mod_targets.borrow_mut().clear();
        if let Some(what) = self.pending_name.take() {
            self.start_naming(what, window, cx);
        }
        if let Some((track, at)) = self.pending_look.take() {
            self.open_picker(track, at, window, cx);
        }
        if let Some(what) = self.steps.ask.take() {
            self.start_naming(what, window, cx);
        }
        if let Some(what) = self.plugin_page.ask.take() {
            self.start_naming(what, window, cx);
        }
        if let Some((ch, which, at)) = self.plugin_page.mini_pending.take() {
            self.open_mini(ch, which, at, window, cx);
        }
        if let Some(dir) = self.opening.take() {
            self.open_song(dir, window, cx);
        }
        if let Some(action) = self.song_pending.take() {
            self.song_action(action, window, cx);
        }
        if self.view == View::Songs && self.home.is_none() {
            self.home = Some(crate::pages::projects::Home::new(window, cx));
        }
        self.browser_frame(window, cx);
        if std::mem::take(&mut self.refocus) && self.naming.is_none() {
            self.focus.focus(window, cx);
        }
        self.viewport_h = f32::from(window.viewport_size().height);
        self.viewport_w = f32::from(window.viewport_size().width);
        self.player.collect();
        self.player.idle();
        // A stretch made in the background goes in, in place of what stood in for it.
        if self.player.fresh() {
            let project = self.engine_project().into_owned();
            self.status = self.player.sync(&project).err();
        }
        self.follow_pattern(cx);
        self.read_meters();
        self.eq_frame();
        self.capture(cx);
        self.automation_frame(cx);
        self.show_automation_values();
        let armed = self.armed().map(|t| self.session.project.tracks[t].id);
        self.player.arm(armed);
        let playing = self.player.playing();
        if playing && self.settings.follow_playhead {
            self.follow_playhead();
        }
        // ponytail: redraws every frame while anything moves; fine until the UI gets heavy.
        let busy = playing
            || self.recording.is_some()
            || !self.writing.is_empty()
            || self.levels.values().any(Level::moving)
            || self.reductions.values().any(|&g| g > 0.01)
            || self.agent_presence().is_some()
            || self.browser.auditioning()
            || self.player.rendering()
            || self.eq_view.as_ref().is_some_and(|v| playing || v.moving())
            || (self.bottom == Some(Bottom::Modulation)
                && self
                    .session
                    .project
                    .tracks
                    .get(self.track)
                    .is_some_and(|t| !t.modulators.is_empty()));
        if busy || self.drag.is_some() {
            window.request_animation_frame();
        }
        let dir = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy());
        let title = format!(
            "{dir}{}",
            if self.session.edited {
                " — Edited"
            } else {
                ""
            }
        );
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        self.steps_actions(div(), cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .track_focus(&self.focus)
            .key_context({
                let context = match (
                    &self.naming,
                    self.menu.as_ref().is_some_and(|m| m.searchable),
                ) {
                    (Some(_), _) => "Naming",
                    (None, true) => "Menu",
                    // The search field has the keyboard: letters and space type.
                    _ if self.browser.searching => "Search",
                    _ if self.typing.is_some() => "Workspace Typing",
                    // The piano roll open: arrows, copy and paste work on its notes.
                    _ if self.view == View::Arrange
                        && self.bottom == Some(Bottom::PianoRoll)
                        && !self.browser_open() =>
                    {
                        "Workspace Arrange Roll"
                    }
                    // The step sequencer open: its shortcuts.
                    _ if self.view == View::Arrange
                        && self.bottom == Some(Bottom::Steps)
                        && !self.browser_open() =>
                    {
                        "Workspace Arrange Steps"
                    }
                    _ => "Workspace Arrange",
                };
                // Up and down pick sounds while the browser shows.
                let mut context = match self.browser_open() {
                    true => format!("{context} Samples"),
                    false => context.to_string(),
                };
                // The EQ window clicked in last: ⌘A and Escape are its bands'.
                if self.naming.is_none() && self.eq_view.as_ref().is_some_and(|v| v.focused) {
                    context += " Eq";
                }
                gpui::KeyContext::parse(&context).unwrap_or_default()
            })
            .on_key_down(cx.listener(|ws, e: &KeyDownEvent, window, cx| {
                // A key a plugin's mapped to puts it on the track (not while typing in a field).
                if !ws.browser.searching && !ws.typing_in_field(window, cx) && ws.mapped_key(e, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                ws.type_key(e, window, cx)
            }))
            .on_key_up(cx.listener(|ws, e: &KeyUpEvent, _, cx| ws.release_key(e, cx)))
            .on_action(
                cx.listener(|ws, _: &TogglePianoRoll, _, cx| {
                    ws.toggle_bottom(Bottom::PianoRoll, cx)
                }),
            )
            .on_action(cx.listener(|ws, _: &ToggleAutomation, _, cx| {
                ws.show_automation = !ws.show_automation;
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &Record, _, cx| ws.record(cx)))
            .on_action(cx.listener(|ws, _: &NewPattern, _, cx| ws.new_pattern(cx)))
            .on_action(cx.listener(|ws, _: &TogglePlugins, _, cx| ws.toggle_plugins_page(cx)))
            .on_action(
                cx.listener(|ws, _: &ToggleScore, _, cx| ws.toggle_bottom(Bottom::Score, cx)),
            )
            .on_action(
                cx.listener(|ws, _: &ToggleLoops, _, cx| ws.toggle_bottom(Bottom::Loops, cx)),
            )
            .on_action(cx.listener(|ws, _: &ToggleSamples, _, cx| ws.toggle_samples(cx)))
            .on_action(cx.listener(|ws, _: &ToggleSamplesPage, _, cx| ws.toggle_samples_page(cx)))
            .on_action(cx.listener(|ws, _: &PreviousSample, _, cx| ws.step_sample(-1, cx)))
            .on_action(cx.listener(|ws, _: &NextSample, _, cx| ws.step_sample(1, cx)))
            .on_action(cx.listener(|ws, _: &CreateStack, _, cx| {
                // A summing stack of the selected tracks (⇧⌘D).
                let tracks = ws.picked_tracks();
                if !tracks.is_empty() {
                    ws.make_stack(&tracks, cx);
                }
            }))
            .on_action(cx.listener(|ws, _: &ToggleTyping, _, cx| ws.toggle_typing(cx)))
            .on_action(cx.listener(|ws, _: &PlayStop, _, cx| ws.play_stop(cx)))
            .on_action(cx.listener(|ws, _: &GoToStart, _, cx| ws.go_to_start(cx)))
            .on_action(cx.listener(|ws, _: &Undo, _, cx| ws.undo(cx)))
            .on_action(cx.listener(|ws, _: &EqSelectAll, _, cx| {
                ws.eq_key(crate::mix::eq::EqOp::SelectAll, cx);
            }))
            .on_action(cx.listener(|ws, _: &EqDeselect, _, cx| {
                if let Some(v) = ws.eq_view.as_mut() {
                    v.picked.clear();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|ws, _: &Redo, _, cx| ws.redo(cx)))
            .on_action(cx.listener(|ws, _: &Delete, _, cx| ws.delete(cx)))
            .on_action(cx.listener(|ws, _: &DeleteTrack, _, cx| ws.delete_track(cx)))
            .on_action(cx.listener(|ws, _: &Split, _, cx| ws.split(cx)))
            .on_action(cx.listener(|ws, _: &CopyRegion, _, cx| {
                if ws.eq_key(crate::mix::eq::EqOp::Copy, cx) {
                    return;
                }
                if !ws.copy_picked(cx) {
                    ws.auto_clip = None;
                    ws.region_key(RegionOp::Copy, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &CutRegion, _, cx| {
                if ws.copy_picked(cx) {
                    ws.delete_picked(cx);
                } else {
                    ws.auto_clip = None;
                    ws.region_key(RegionOp::Cut, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &PasteRegion, _, cx| {
                if ws.eq_key(crate::mix::eq::EqOp::Paste, cx) {
                    return;
                }
                if !ws.paste_points(cx) {
                    ws.paste_key(cx)
                }
            }))
            .on_action(
                cx.listener(|ws, _: &RepeatRegion, _, cx| ws.region_key(RegionOp::Duplicate, cx)),
            )
            .on_action(cx.listener(|ws, _: &MuteRegion, _, cx| ws.region_key(RegionOp::Mute, cx)))
            .on_action(cx.listener(|ws, _: &NewTrack, window, cx| {
                let at = gpui::point(px(12.), px(CONTROL_BAR_H + 44.));
                ws.open_adder(at, window, cx)
            }))
            .on_action(cx.listener(|ws, _: &ZoomIn, _, cx| ws.zoom(1.25, cx)))
            .on_action(cx.listener(|ws, _: &ZoomOut, _, cx| ws.zoom(0.8, cx)))
            .on_action(cx.listener(|ws, _: &Save, _, cx| {
                ws.save(cx);
            }))
            .on_action(cx.listener(|ws, _: &Export, _, cx| ws.open_export(cx)))
            .on_action(cx.listener(|ws, _: &ToggleCycle, _, cx| ws.toggle_cycle(cx)))
            .on_action(cx.listener(|ws, _: &ToggleMetronome, _, cx| ws.toggle_metronome(cx)))
            .on_action(cx.listener(|ws, _: &Quit, window, cx| ws.quit(window, cx)))
            // Audio dragged in from the Finder: followed here, and let go by `file_listener`.
            // (gpui's own drop needs the pointer to count as hovering, which it doesn't after a
            // key press, as a Finder drag isn't a mouse move.)
            .on_drag_move(cx.listener(|ws, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                ws.file_drag = Some((pt(e.event.position), e.drag(cx).paths().to_vec()));
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &ToggleMixer, _, cx| ws.toggle_mixer(cx)))
            .on_action(cx.listener(|ws, _: &ToggleAgent, window, cx| ws.toggle_agent(window, cx)))
            .on_action(cx.listener(|ws, _: &NewSong, window, cx| ws.show_home(true, window, cx)))
            .on_action(cx.listener(|ws, _: &CloseSong, window, cx| {
                ws.song_action(crate::ui::menu::MenuAction::CloseSong, window, cx)
            }))
            .on_action(cx.listener(|ws, _: &ShowSongs, _, cx| ws.toggle_songs(cx)))
            .on_action(cx.listener(|ws, _: &EditPlaybooks, _, cx| ws.open_genres(None, cx)))
            .on_action(cx.listener(|ws, _: &StudySong, _, cx| ws.open_study(cx)))
            .on_action(cx.listener(|ws, _: &ShowSettings, _, cx| ws.open_settings(cx)))
            .on_action(cx.listener(|ws, _: &ToolMenu, _, cx| {
                // Under the toolbar, where the pointer would be.
                let x = if ws.show_inspector { ws.side_w() } else { 0.0 } + 14.0;
                ws.tool_menu(gpui::point(px(x), px(CONTROL_BAR_H + TOP_H)), cx);
            }))
            .on_action(
                cx.listener(|ws, _: &SelectAllNotes, _, cx| ws.roll_op(RollOp::SelectAll, cx)),
            )
            .on_action(cx.listener(|ws, _: &CopyNotes, _, cx| ws.roll_op(RollOp::Copy, cx)))
            .on_action(cx.listener(|ws, _: &CutNotes, _, cx| ws.roll_op(RollOp::Cut, cx)))
            .on_action(cx.listener(|ws, _: &PasteNotes, _, cx| ws.roll_op(RollOp::Paste, cx)))
            .on_action(
                cx.listener(|ws, _: &DuplicateNotes, _, cx| ws.roll_op(RollOp::Duplicate, cx)),
            )
            .on_action(cx.listener(|ws, _: &NotesUp, _, cx| ws.roll_op(RollOp::Transpose(1), cx)))
            .on_action(
                cx.listener(|ws, _: &NotesDown, _, cx| ws.roll_op(RollOp::Transpose(-1), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NotesOctaveUp, _, cx| ws.roll_op(RollOp::Transpose(12), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NotesOctaveDown, _, cx| {
                    ws.roll_op(RollOp::Transpose(-12), cx)
                }),
            )
            .on_action(cx.listener(|ws, _: &NudgeLeft, _, cx| ws.roll_op(RollOp::Nudge(-4), cx)))
            .on_action(cx.listener(|ws, _: &NudgeRight, _, cx| ws.roll_op(RollOp::Nudge(4), cx)))
            .on_action(
                cx.listener(|ws, _: &NudgeLeftFine, _, cx| ws.roll_op(RollOp::Nudge(-1), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NudgeRightFine, _, cx| ws.roll_op(RollOp::Nudge(1), cx)),
            )
            .on_action(cx.listener(|ws, _: &QuantizeNotes, _, cx| ws.roll_op(RollOp::Quantize, cx)))
            .on_action(cx.listener(|ws, _: &DeselectNotes, _, cx| ws.roll_op(RollOp::Deselect, cx)))
            .on_action(cx.listener(|ws, _: &BrowserUp, _, cx| ws.move_picked(-1, cx)))
            .on_action(cx.listener(|ws, _: &BrowserDown, _, cx| ws.move_picked(1, cx)))
            .on_action(cx.listener(|ws, _: &BrowserNewTrack, _, cx| ws.use_picked(true, cx)))
            .on_action(cx.listener(|ws, _: &ToggleInspector, _, cx| {
                ws.show_inspector = !ws.show_inspector;
                cx.notify();
            }))
            // With no song open, the Songs page is the whole window: nothing to play.
            .when(!self.home_alone(), |d| d.child(self.control_bar(cx)))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .when(self.view == View::Mixer, |d| d.child(self.mixer_view(cx)))
                    .when(self.view == View::Samples, |d| {
                        d.child(self.samples_page(cx))
                    })
                    .when(self.view == View::Songs, |d| d.child(self.songs_page(cx)))
                    .when(self.view == View::Playbooks, |d| {
                        d.child(self.genres_page(cx))
                    })
                    .when(self.view == View::Study, |d| d.child(self.study_page(cx)))
                    .when(self.view == View::Settings, |d| {
                        d.child(self.settings_page(cx))
                    })
                    .when(self.view == View::Plugins, |d| {
                        d.child(self.plugins_page(cx))
                    })
                    .when(self.view == View::Arrange, |d| {
                        d.when(self.show_inspector, |d| d.child(self.inspector(cx)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .child(self.tracks_area(cx))
                                    .when_some(self.bottom, |d, pane| {
                                        d.child(self.editor(pane, cx))
                                    }),
                            )
                            .when(self.show_browser, |d| d.child(self.browser_panel(cx)))
                            .when(self.show_agent && !self.show_browser, |d| {
                                d.child(self.agent_panel(cx))
                            })
                    }),
            )
            .when(!self.home_alone() || self.status.is_some(), |d| {
                d.child(self.status_bar())
            })
            .when(self.drag.is_some(), |d| d.child(self.drag_listener(cx)))
            .when(self.file_drag.is_some(), |d| {
                d.child(self.file_listener(cx))
            })
            .children(self.sample_ghost())
            .children(self.device_ghost())
            .children(self.panner_view(cx))
            .children(self.mod_ghost())
            .children(self.eq_window(cx))
            .children(self.mini_view(cx))
            .children(self.menu_view(cx))
            .children(self.adder_view(cx))
            .children(self.export_view(cx))
            .children(self.picker_view(cx))
    }
}
