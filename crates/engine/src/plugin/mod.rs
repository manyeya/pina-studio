//! Hosting plugins: VST3, CLAP and VST2 (and on macOS, Audio Units), behind one interface. Each
//! format loads a plugin as two halves: a `Plugin` on the main thread (its parameters, settings,
//! presets and editor) and a `Processor` the audio thread runs. What differs by platform (loading
//! a plugin's binary, its editor's window) is in `platform`.

#[cfg(target_os = "macos")]
pub mod au;
pub mod clap;
pub mod platform;
pub mod scan;
pub mod vst2;
pub mod vst3;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub use platform::{MainThreadMarker, Window};

use crate::song::project::PluginRef;
pub use crate::song::project::{ExpressionKind, Format, Preset, PresetSource};

/// A note for an instrument, `offset` samples into the block.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Note {
    pub offset: u32,
    pub pitch: u8,
    /// 0 is a note off.
    pub velocity: f32,
}

/// What an instrument hears besides notes, `offset` samples into the block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
    pub offset: u32,
    pub kind: ControlKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlKind {
    /// A MIDI controller (0 to 127; 1 is the mod wheel, 64 the sustain pedal), 0 to 1.
    Cc { number: u8, value: f32 },
    /// -1 to 1, across the plugin's bend range.
    PitchBend(f32),
    /// Channel pressure, 0 to 1.
    Aftertouch(f32),
    /// One held note's pressure, 0 to 1.
    PolyPressure { pitch: u8, value: f32 },
    /// One held note's expression, the note known by its pitch.
    Expression {
        pitch: u8,
        kind: ExpressionKind,
        value: f32,
    },
}

/// A plugin parameter: its id and name, and its value, default and steps from 0 to 1.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PluginParam {
    pub id: u32,
    pub name: String,
    pub value: f64,
    pub default: f64,
    /// 0 for a continuous parameter; otherwise how many steps from 0 to 1 (a switch has 1, a
    /// choice of four 3), each step's value being `step / steps`.
    pub steps: u32,
    /// Its unit, like "Hz" or "dB", if the plugin says.
    pub units: String,
}

/// A plugin's main-thread side: its parameters, settings, presets and editor.
pub trait PluginApi {
    fn name(&self) -> &str;
    /// The parameters a person can turn: automatable, not hidden, not the bypass.
    fn params(&self) -> Vec<PluginParam>;
    /// Sets a parameter (0 to 1), in the plugin's controls and, in turn, its processor.
    fn set_param(&self, id: u32, value: f64);
    /// Shows a parameter's value (0 to 1) in the plugin's controls and editor only.
    fn show_param(&self, id: u32, value: f64);
    /// A parameter's value now, 0 to 1.
    fn param_value(&self, id: u32) -> f64;
    /// A value (0 to 1) as the plugin shows it, like "2.40 kHz".
    fn param_text(&self, id: u32, value: f64) -> String;
    /// The plugin's own reading of a typed value ("850 Hz", "Sine"), 0 to 1, if it has one.
    /// `value_from_text` falls back on reading the plugin's display when it hasn't.
    fn param_from_text(&self, id: u32, text: &str) -> Option<f64>;
    /// Its settings, to save with the project and give back to `instantiate` or `set_state`.
    fn state(&self) -> Option<Vec<u8>>;
    fn set_state(&self, state: &[u8]) -> Result<(), String>;
    /// What it can load: its own programs, then preset files of its format.
    fn presets(&self) -> Vec<Preset>;
    fn load_preset(&self, preset: &Preset) -> Result<(), String>;
    /// Saves its settings as a preset file of its format called `name`, where its presets live,
    /// and says where.
    fn save_preset(&self, name: &str) -> Result<PathBuf, String>;
    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String>;
    /// Work the plugin asked to do on the main thread. Call it often.
    fn idle(&self) {}
    /// Whether it has a sidechain input (as its processor says, for `set_sidechain`).
    fn sidechain(&self) -> bool {
        false
    }
}

/// A plugin's audio-thread side. Dropping it shuts the plugin down, so it's dropped back on the
/// main thread, after its `Plugin`.
pub trait ProcessorApi: Send {
    /// How late its output is, in samples.
    fn latency(&self) -> u32;
    /// Where the transport is, before a `process`.
    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64);
    /// Moves a parameter (0 to 1) across the next block, from `start` to `end`, as automation
    /// does. Past `AUTOMATED` parameters a block, the rest wait.
    fn automate(&mut self, id: u32, start: f64, end: f64);
    /// Whether it has a sidechain: a second input, for what another track plays.
    fn sidechain(&self) -> bool {
        false
    }
    /// Whether edits from the main thread are still waiting to be applied.
    fn pending(&self) -> bool;
    /// Runs one block in place: `buf` goes to the main input (if it has one) and comes back
    /// holding the main output; `sidechain`, if given, goes to its sidechain input. Applies the
    /// notes and controls (each sorted by offset) and any edits. Allocation-free.
    fn process(
        &mut self,
        buf: [&mut [f32]; 2],
        frames: usize,
        notes: &[Note],
        controls: &[Control],
        sidechain: Option<[&[f32]; 2]>,
    );
}

pub type Plugin = Box<dyn PluginApi>;
pub type Processor = Box<dyn ProcessorApi>;

/// Most parameters one plugin follows automation for, per block.
pub const AUTOMATED: usize = 32;

/// Applies what's waiting: runs silent blocks until the main thread's edits are all in, so a
/// render starts with the sound the project sets.
pub fn settle(p: &mut Processor, max_block: usize) {
    let (mut l, mut r) = (vec![0f32; max_block], vec![0f32; max_block]);
    for _ in 0..16 {
        if !p.pending() {
            break;
        }
        p.process([&mut l, &mut r], max_block, &[], &[], None);
    }
}

/// The libraries plugins are loaded from, each kept loaded once opened.
#[derive(Default)]
pub struct Libraries {
    vst3: HashMap<PathBuf, vst3::Module>,
    clap: HashMap<PathBuf, clap::Library>,
    vst2: HashMap<PathBuf, vst2::Library>,
}

impl Libraries {
    pub fn new() -> Libraries {
        Libraries::default()
    }
}

/// Loads the plugin `r` names at `rate`, restoring `state` (from `PluginApi::state`) if given.
pub fn instantiate(
    libs: &mut Libraries,
    r: &PluginRef,
    rate: f64,
    max_block: usize,
    state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    match r.format {
        Format::Vst3 => vst3::instantiate(&mut libs.vst3, r, rate, max_block, state),
        Format::Clap => clap::instantiate(&mut libs.clap, r, rate, max_block, state),
        Format::Vst2 => vst2::instantiate(&mut libs.vst2, r, rate, max_block, state),
        #[cfg(target_os = "macos")]
        Format::Au => au::instantiate(r, rate, max_block, state),
        #[cfg(not(target_os = "macos"))]
        Format::Au => Err(format!("{}: Audio Units are macOS's", r.class)),
    }
}

/// An open editor: its window, and what closes the plugin's view. Dropping it closes both.
pub struct Editor {
    pub window: Window,
    close: Option<Box<dyn FnOnce()>>,
}

impl Editor {
    /// `close` detaches the plugin's view; it runs before the window closes.
    pub fn new(window: Window, close: impl FnOnce() + 'static) -> Editor {
        Editor {
            window,
            close: Some(Box::new(close)),
        }
    }

    /// Brings the window back, if it was closed.
    pub fn show(&self) {
        self.window.show();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(close) = self.close.take() {
            close();
        }
        self.window.close();
    }
}

/// Parameter edits from the main thread (Pina Studio's, or the plugin's own editor's) to the audio
/// thread, in order.
pub struct EditSender(std::sync::Mutex<rtrb::Producer<(u32, f64)>>);

impl EditSender {
    /// A full queue drops the edit; the next one catches up.
    pub fn push(&self, id: u32, value: f64) {
        let _ = self.0.lock().unwrap().push((id, value));
    }
}

/// The audio thread's end of an `EditSender`. A change to a stepped parameter (a mode or type
/// switch, which can reset others) is the last a block applies, so what was set after it lands
/// after it rather than being reset by it.
pub struct EditReceiver {
    rx: rtrb::Consumer<(u32, f64)>,
    stepped: Vec<u32>,
}

impl EditReceiver {
    /// Hands this block's edits to `apply`, in order.
    pub fn drain(&mut self, mut apply: impl FnMut(u32, f64)) {
        while let Ok((id, value)) = self.rx.pop() {
            apply(id, value);
            if self.stepped.binary_search(&id).is_ok() {
                break;
            }
        }
    }

    pub fn pending(&self) -> bool {
        !self.rx.is_empty()
    }
}

/// A queue of edits; `stepped` lists the parameters with steps (see `EditReceiver`).
pub fn edit_queue(mut stepped: Vec<u32>) -> (EditSender, EditReceiver) {
    stepped.sort_unstable();
    let (tx, rx) = rtrb::RingBuffer::new(1024);
    (
        EditSender(std::sync::Mutex::new(tx)),
        EditReceiver { rx, stepped },
    )
}

/// Turns a typed value ("850 Hz", "-6 dB", "Sine", "40%") into a parameter value, 0 to 1: as
/// the plugin reads it if it can, else by the plugin's own display: a step whose name matches,
/// or the number found between the values shown either side of it (displays rise or fall
/// steadily).
pub fn value_from_text(p: &dyn PluginApi, param: &PluginParam, text: &str) -> Result<f64, String> {
    let text = text.trim();
    let shown = |v: f64| p.param_text(param.id, v);
    // The plugin's reading, if what it then shows agrees: some ignore units ("1 kHz" as 1 Hz).
    let agrees = |v: f64| {
        let back = shown(v);
        match (number(text), number(&back)) {
            (Some(a), Some(b)) => {
                let (a, b) = (a * scale(text), b * scale(&back));
                (a - b).abs() <= 0.02 * a.abs().max(b.abs()).max(1e-6)
            }
            _ => back.trim().eq_ignore_ascii_case(text),
        }
    };
    if let Some(v) = p
        .param_from_text(param.id, text)
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
        .filter(|&v| agrees(v))
    {
        return Ok(v);
    }
    let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    if let Some(named) = named_values(p, param) {
        if let Some((_, v)) = named.iter().find(|(name, _)| same(name, text)) {
            return Ok(*v);
        }
        if number(text).is_none() {
            let names: Vec<&str> = named.iter().map(|(n, _)| n.as_str()).collect();
            return Err(format!(
                "{}: no setting reads {text:?}; it's one of {}",
                param.name,
                names.join(", ")
            ));
        }
    }
    let want = number(text).ok_or_else(|| {
        format!(
            "{}: no setting reads {text:?} (it shows values like {:?})",
            param.name,
            shown(0.5)
        )
    })?;
    // Values as written mean kilo-units when the display uses them ("2.4 kHz" or "2400 Hz").
    let scaled = |s: &str| number(s).map(|n| n * scale(s));
    let target = want * scale(text);
    let samples = 256;
    let points: Vec<(f64, f64)> = (0..=samples)
        .filter_map(|i| {
            let v = i as f64 / samples as f64;
            scaled(&shown(v)).map(|n| (v, n))
        })
        .collect();
    let bracket = points.windows(2).find(|w| {
        let (a, b) = (w[0].1, w[1].1);
        (a <= target && target <= b) || (b <= target && target <= a)
    });
    let Some(w) = bracket else {
        let (lo, hi) = (points.first(), points.last());
        return Err(match (lo, hi) {
            (Some(lo), Some(hi)) => format!(
                "{}: {text:?} is out of its range, {:?} to {:?}",
                param.name,
                shown(lo.0),
                shown(hi.0)
            ),
            _ => format!("{}: its values aren't numbers", param.name),
        });
    };
    // Narrow in on it between the two.
    let (mut lo, mut hi) = (w[0].0, w[1].0);
    let rising = w[1].1 >= w[0].1;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        match scaled(&shown(mid)) {
            Some(n) if (n < target) == rising => lo = mid,
            Some(_) => hi = mid,
            None => break,
        }
    }
    let v = (lo + hi) / 2.0;
    Ok(if param.steps > 0 {
        (v * param.steps as f64).round() / param.steps as f64
    } else {
        v
    })
}

/// The values a parameter shows as names ("Classic", "Sine", "Wavetable"), with a value that
/// gives each: its steps' names, or for a parameter that doesn't say it has steps but shows only
/// words, each word's span of values (by its middle).
pub fn named_values(p: &dyn PluginApi, param: &PluginParam) -> Option<Vec<(String, f64)>> {
    let shown = |v: f64| p.param_text(param.id, v);
    if (1..=128).contains(&param.steps) {
        let steps = param.steps as f64;
        return Some(
            (0..=param.steps)
                .map(|s| (shown(s as f64 / steps), s as f64 / steps))
                .collect(),
        );
    }
    if param.steps != 0 || [0.0, 0.5, 1.0].iter().any(|&v| number(&shown(v)).is_some()) {
        return None;
    }
    let samples = 256;
    let mut runs: Vec<(String, f64, f64)> = Vec::new();
    for i in 0..=samples {
        let v = i as f64 / samples as f64;
        let text = shown(v);
        if number(&text).is_some() {
            return None;
        }
        match runs.last_mut() {
            Some(run) if run.0 == text => run.2 = v,
            _ => runs.push((text, v, v)),
        }
    }
    (runs.len() <= 128).then(|| {
        runs.into_iter()
            .map(|(t, a, b)| (t, (a + b) / 2.0))
            .collect()
    })
}

/// The number a display starts with, like -6.5 in "-6.5 dB" (not the 2 in "FM2"); "-inf"
/// counts as very low.
fn number(s: &str) -> Option<f64> {
    let rest = s.trim_start();
    if rest.to_lowercase().starts_with("-inf") || rest.starts_with("-∞") {
        return Some(-1e9);
    }
    if !rest.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c)) {
        return None;
    }
    let end = rest
        .char_indices()
        .skip(1)
        .find(|&(_, c)| !(c.is_ascii_digit() || c == '.'))
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].parse().ok()
}

/// 1000 for a value in kilo-units ("2.4 kHz", "1.2 k", "1.5 s" read as ms when shown in ms is
/// left alone: only k and M prefixes count), else 1.
fn scale(s: &str) -> f64 {
    let unit = s
        .trim_start_matches(|c: char| c.is_ascii_digit() || "+-. ".contains(c))
        .trim_start();
    match unit.chars().next() {
        Some('k') | Some('K') => 1e3,
        Some('M') if !unit.starts_with("ms") => 1e6,
        _ => 1.0,
    }
}

/// Where a format's preset files for a plugin live, the user's first: on macOS
/// `~/Library/Audio/Presets/<vendor>/<name>` and the same under `/Library`; on Windows
/// `Documents\VST3 Presets\<vendor>\<name>` and the same under `ProgramData`.
pub fn preset_dirs(vendor: &str, name: &str) -> Vec<PathBuf> {
    let var = |v: &str| std::env::var_os(v).map(PathBuf::from).unwrap_or_default();
    let roots = match cfg!(windows) {
        true => [
            var("USERPROFILE").join("Documents").join("VST3 Presets"),
            var("PROGRAMDATA").join("VST3 Presets"),
        ],
        false => [
            var("HOME").join("Library/Audio/Presets"),
            PathBuf::from("/Library/Audio/Presets"),
        ],
    };
    roots
        .into_iter()
        .map(|d| d.join(vendor).join(name))
        .collect()
}

/// Every file ending in `ext` under `dirs`, in subfolders too, as presets named by their files.
pub fn preset_files(dirs: &[PathBuf], ext: &str) -> Vec<Preset> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                walk(&path, ext, out);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
            {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    dirs.iter().for_each(|d| walk(d, ext, &mut files));
    files.sort();
    files
        .into_iter()
        .map(|path| Preset {
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            source: PresetSource::File { file: path },
        })
        .collect()
}

/// A file name for a preset called `name`: its characters, less those paths can't have.
pub fn preset_file_name(name: &str, ext: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "/:\\".contains(c) { '-' } else { c })
        .collect();
    format!("{}.{ext}", clean.trim())
}

#[cfg(test)]
mod tests;
