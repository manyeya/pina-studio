//! What the Drum Machine does: kits made (picked to fit the song, or from a library folder) and
//! loaded into a kit; pads given sounds, from the library, the browser or the Finder; a pad's
//! sounds (or the whole kit's) stepped through, its folder's others first, then the library's
//! that sound most like it; sounds layered on a pad; pads moved, choked and taken out.

use super::*;
use pina_engine::{
    samples::{
        Sample,
        search::{self, KitSeed},
    },
    song::project::{NewPad, PAD_LAYERS},
};

/// How many sounds a pad's lane offers to step through.
const LANE: usize = 48;

/// What layers well under or over a kind of sound: a sub under a kick, a clap on a snare.
fn companions(role: Category) -> Vec<Category> {
    use Category::*;
    match role {
        Kick => vec![Kick, Bass808, Perc],
        Snare => vec![Clap, Snap, Rim, Snare],
        Clap => vec![Snare, Snap, Clap],
        Snap | Rim => vec![Clap, Snare, Snap, Rim],
        HatClosed => vec![Shaker, Tambourine, HatClosed],
        HatOpen => vec![Shaker, Ride, HatOpen],
        Ride | Crash => vec![Crash, Ride, HatOpen],
        r => vec![r, Perc],
    }
}

/// Where a beat's drums go on a starter pattern, by what they are.
pub(crate) fn starter_steps(role: Category) -> Vec<usize> {
    match role {
        Category::Kick => vec![0, 7, 8],
        Category::Snare | Category::Clap => vec![4, 12],
        Category::HatClosed => vec![0, 2, 4, 8, 10, 12],
        Category::HatOpen => vec![6, 14],
        Category::Perc => vec![3, 11],
        Category::Bass808 => vec![0, 10],
        _ => Vec::new(),
    }
}

/// Pads for sounds picked for a kit, each on a note of its own. The first of each kind sits on
/// its General MIDI note (a kick on C1, a snare on D1, a closed hat on F♯1…), so keyboards and
/// MIDI files play it right; a pitched sound (an 808) on the note it plays, so its pad is in
/// tune; the rest on the free notes from C1 up, past those the kit's kinds keep. Hats choke each
/// other, and the beat's drums get a starter pattern.
pub(crate) fn pads_for(picks: &[(Category, Sample)]) -> Vec<NewPad> {
    let gm: Vec<u8> = picks.iter().filter_map(|(r, _)| r.drum_note()).collect();
    let mut used: Vec<u8> = Vec::new();
    let mut seen: Vec<Category> = Vec::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut pads = Vec::new();
    for (role, s) in picks {
        let first = !seen.contains(role);
        seen.push(*role);
        let free = |n: &u8| !used.contains(n) && !gm.contains(n);
        let root = s.analysis.pitch.filter(|_| !role.is_drum()).map(|p| p.note);
        let note = role
            .drum_note()
            .filter(|n| first && !used.contains(n))
            .or_else(|| root.filter(free))
            .or_else(|| (36..128).find(free))
            .or_else(|| (0..128).find(|n| !used.contains(n)));
        let Some(note) = note else {
            break;
        };
        used.push(note);
        let mut zone = search::zone(s, None);
        (zone.note, zone.high, zone.root) = (note, None, None);
        zone.gate = false;
        let base = match role {
            Category::Other => s.name.clone(),
            r => r.name().to_string(),
        };
        let n = names.entry(base.clone()).or_insert(0);
        *n += 1;
        let name = match n {
            1 => base,
            n => format!("{base} {n}"),
        };
        pads.push(NewPad {
            name,
            sampler: Sampler {
                zones: vec![zone],
                ..Default::default()
            },
            note,
            choke: matches!(role, Category::HatClosed | Category::HatOpen).then_some(1),
            steps: match first {
                true => starter_steps(*role),
                false => Vec::new(),
            },
        });
    }
    pads
}

impl Workspace {
    /// The library's folders that make a kit, with how many sounds each has.
    pub(crate) fn drum_folders(&self) -> Vec<(String, usize)> {
        search::drum_folders(&self.browser.library.samples)
    }

    /// A menu of the library's drum folders: picking one makes a kit of it (into kit track
    /// `into`, if given).
    pub(crate) fn folder_menu(
        &mut self,
        at: gpui::Point<Pixels>,
        into: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.folders_menu(at, cx, |folder| DrumOp::Folder { folder, into });
    }

    /// A menu of the library's drum folders, each picked as `op` says.
    pub(crate) fn folders_menu(
        &mut self,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
        op: impl Fn(String) -> DrumOp,
    ) {
        let items: Vec<(String, MenuAction)> = self
            .drum_folders()
            .into_iter()
            .map(|(folder, n)| (format!("{folder}  ·  {n}"), MenuAction::Drums(op(folder))))
            .collect();
        if items.is_empty() {
            self.status =
                Some("No folder in the library has drum hits: add one on the Samples page.".into());
            cx.notify();
            return;
        }
        self.menu = Some(Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        });
        cx.notify();
    }

    /// Adds a kit to the song as a stack, picks it and opens it in the Drum Machine.
    pub(crate) fn add_drum_kit(&mut self, name: &str, pads: Vec<NewPad>, cx: &mut Context<Self>) {
        let n = pads.len();
        let index = self.new_track_index(false);
        let bar = self.session.project.bar();
        let at = self.playhead() / bar * bar;
        let mut made = None;
        self.edit(cx, |ws| {
            made = Some(ws.session.project.add_kit(name, pads, index, at, true));
        });
        let Some(made) = made else {
            return;
        };
        self.select_track(made.kit);
        self.region = Some((made.kit, 0));
        self.drums.from = 36;
        self.drums.pad = self
            .session
            .project
            .tracks
            .get(made.pads.first().copied().unwrap_or(made.kit))
            .and_then(|t| t.pad)
            .map_or(36, |p| p.note);
        self.drums.lanes.clear();
        self.open_drums(cx);
        self.status = Some(format!(
            "Made {name}: {n} pads, each its own channel in a stack"
        ));
    }

    /// The Drum Machine opened under the tracks: from another page (the browser's), the page
    /// stays, the Drum Machine there when the tracks are next shown.
    fn open_drums(&mut self, cx: &mut Context<Self>) {
        if self.view == View::Arrange {
            return self.show_bottom(Bottom::Drums, cx);
        }
        self.bottom = Some(Bottom::Drums);
        self.editor_h = self.editor_h.max(DRUMS_H);
    }

    /// A new kit picked to go together and fit the song (its key and tempo); with no drums in
    /// the library, the built-in synthesized kit. Says whether it's a kit of pads.
    pub fn new_drum_kit(&mut self, cx: &mut Context<Self>) -> bool {
        let seed = KitSeed {
            key: self.sample_key().map(|k| k.name()),
            bpm: self.sample_bpm(),
            roles: vec![
                Category::Kick,
                Category::Snare,
                Category::Clap,
                Category::HatClosed,
                Category::HatOpen,
                Category::Rim,
                Category::Tom,
                Category::Perc,
                Category::Shaker,
                Category::Crash,
            ],
            alternatives: 1,
            ..Default::default()
        };
        let samples = &self.browser.library.samples;
        let picks: Vec<(Category, Sample)> = search::kit(samples, &seed)
            .slots
            .into_iter()
            .filter_map(|s| Some((s.role, samples.get(s.pick?.index)?.clone())))
            .collect();
        if picks.len() < 3 {
            self.add_synth_drums(cx);
            return false;
        }
        let name = match self.sample_key() {
            Some(k) => format!("Kit in {}", k.name()),
            None => "Kit".to_string(),
        };
        self.add_drum_kit(&name, pads_for(&picks), cx);
        true
    }

    /// The built-in drum kit, synthesized: for a library with no drums in it yet.
    fn add_synth_drums(&mut self, cx: &mut Context<Self>) {
        self.new_track(Some(Instrument::Drums), cx);
        self.status = Some(
            "The library has no drums yet, so this is the synthesized kit: add a folder of drum \
             samples on the Samples page for a kit of pads."
                .into(),
        );
        cx.notify();
    }

    /// A kit of a library folder's sounds: a new kit, or into kit track `into` (the pads on its
    /// notes given the folder's sounds, new pads for the rest; the kit's patterns and its pads'
    /// shaping and channels kept).
    pub(crate) fn kit_from_folder(
        &mut self,
        folder: &str,
        into: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let samples = &self.browser.library.samples;
        let picks: Vec<(Category, Sample)> = search::folder_kit(samples, folder)
            .slots
            .into_iter()
            .filter_map(|s| Some((s.role, samples.get(s.pick?.index)?.clone())))
            .collect();
        if picks.is_empty() {
            self.status = Some(format!("{folder} has no one-shots to make a kit of."));
            cx.notify();
            return;
        }
        let pads = pads_for(&picks);
        match into {
            Some(kit) => self.load_into_kit(kit, pads, cx),
            None => {
                let name = folder.rsplit('/').next().unwrap_or(folder).to_string();
                self.add_drum_kit(&name, pads, cx);
            }
        }
    }

    /// Pads loaded into a kit: each on a note the kit has a pad on gives that pad its sound (its
    /// shaping, channel and the kit's patterns kept), the rest come in as new pads.
    pub(crate) fn load_into_kit(&mut self, kit: usize, pads: Vec<NewPad>, cx: &mut Context<Self>) {
        let n = pads.len();
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            for pad in pads {
                let zone = pad.sampler.zones.first().cloned().unwrap_or_default();
                match p.pad_on(kit, pad.note) {
                    Some(t) => {
                        let name = p.tracks[t].name.clone();
                        p.set_pad_sound(t, 0, zone, &name);
                    }
                    None => drop(p.add_pad(kit, pad)),
                }
            }
        });
        self.drums.lanes.clear();
        self.open_drums(cx);
        self.status = Some(format!("Loaded {n} sounds into the kit"));
    }

    /// A library sample on a kit's pad: its sound (its `layer`'s), or (an empty note) a new pad.
    pub(crate) fn set_pad_sample(
        &mut self,
        kit: usize,
        note: u8,
        layer: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.browser.library.samples.get(index).cloned() else {
            return;
        };
        let role = s.analysis.category;
        self.give_pad(kit, note, layer, search::zone(&s, None), role, cx);
    }

    /// A sound on a pad: replacing what it had on `layer` (its name, shaping and channel kept),
    /// or, on an empty note, a new pad named for what it is.
    fn give_pad(
        &mut self,
        kit: usize,
        note: u8,
        layer: usize,
        mut zone: Zone,
        role: Category,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            match p.pad_on(kit, note) {
                Some(t) => {
                    let name = p.tracks[t].name.clone();
                    p.set_pad_sound(t, layer, zone, &name);
                }
                None => {
                    (zone.note, zone.high, zone.root) = (note, None, None);
                    zone.gate = false;
                    let name = match role {
                        Category::Other => zone.name.clone(),
                        r => r.name().to_string(),
                    };
                    let pad = NewPad {
                        name,
                        sampler: Sampler {
                            zones: vec![zone],
                            ..Default::default()
                        },
                        note,
                        choke: matches!(role, Category::HatClosed | Category::HatOpen).then_some(1),
                        steps: Vec::new(),
                    };
                    p.add_pad(kit, pad);
                }
            }
        });
        self.drums.pad = note;
        self.drums.lanes.remove(&(note, layer));
        self.hit_pad(kit, note, 110);
    }

    /// A sound layered on a pad, picked, and heard with the rest.
    fn layer_pad(&mut self, kit: usize, note: u8, zone: Zone, cx: &mut Context<Self>) {
        let Some(t) = self.session.project.pad_on(kit, note) else {
            return;
        };
        let mut added = None;
        self.edit(cx, |ws| added = ws.session.project.add_pad_layer(t, zone));
        let Some(layer) = added else {
            self.status = Some(format!("A pad layers up to {PAD_LAYERS} sounds."));
            return;
        };
        self.drums.pad = note;
        self.drums.layer = (note, layer);
        self.drums.lanes.remove(&(note, layer));
        self.hit_pad(kit, note, 110);
    }

    /// Files dropped on a pad's sound, each layered on it.
    pub(crate) fn layer_files(
        &mut self,
        files: &[std::path::PathBuf],
        note: u8,
        cx: &mut Context<Self>,
    ) {
        let Some(kit) = self.drum_kit() else {
            return;
        };
        for file in files {
            let samples = &self.browser.library.samples;
            let zone = match samples.iter().find(|s| &s.path == file) {
                Some(s) => search::zone(s, None),
                None => Zone {
                    file: file.clone(),
                    from: Some(file.clone()),
                    name: file
                        .file_stem()
                        .map_or("Layer".into(), |n| n.to_string_lossy().into_owned()),
                    ..Default::default()
                },
            };
            self.layer_pad(kit, note, zone, cx);
        }
    }

    /// Files from the Finder dropped on the pads: the first on the pad under the pointer, the
    /// rest on the empty ones after it.
    pub(crate) fn drop_files_on_pads(
        &mut self,
        files: &[std::path::PathBuf],
        note: u8,
        cx: &mut Context<Self>,
    ) {
        let Some(kit) = self.drum_kit() else {
            return;
        };
        let mut next = note;
        for (k, file) in files.iter().enumerate() {
            if k > 0 {
                let p = &self.session.project;
                match (next + 1..128).find(|&n| p.pad_on(kit, n).is_none()) {
                    Some(n) => next = n,
                    None => break,
                }
            }
            let known = self
                .browser
                .library
                .samples
                .iter()
                .position(|s| &s.path == file);
            match known {
                Some(index) => self.set_pad_sample(kit, next, 0, index, cx),
                None => {
                    let name = file
                        .file_stem()
                        .map_or("Pad".into(), |n| n.to_string_lossy().into_owned());
                    let role = pina_engine::song::commands::drum_note(&name)
                        .map_or(Category::Other, gm_role);
                    let zone = Zone {
                        file: file.clone(),
                        from: Some(file.clone()),
                        name,
                        note: next,
                        ..Default::default()
                    };
                    self.give_pad(kit, next, 0, zone, role, cx);
                }
            }
        }
        self.drums.pad = note;
    }

    /// The note a new pad in a kit goes on: the first free one from C1 up.
    pub(crate) fn free_pad_note(&self, kit: usize) -> u8 {
        let p = &self.session.project;
        (36..128)
            .chain(0..36)
            .find(|&n| p.pad_on(kit, n).is_none())
            .unwrap_or(127)
    }

    /// The note a library sample goes on as a new pad: its kind's General MIDI note if that's
    /// free, else the first free one.
    pub(crate) fn free_pad_note_for(&self, kit: usize, index: usize) -> u8 {
        let p = &self.session.project;
        self.browser
            .library
            .samples
            .get(index)
            .and_then(|s| s.analysis.category.drum_note())
            .filter(|&n| p.pad_on(kit, n).is_none())
            .unwrap_or_else(|| self.free_pad_note(kit))
    }

    /// A pad's lane (its `layer`'s): the library's sounds it can step through (its own first,
    /// then its folder's others of its kind and the library's like it), and which it's on. Kept
    /// while it's on one of them, so stepping back finds where it was.
    pub(crate) fn pad_lane(&mut self, kit: usize, note: u8, layer: usize) -> (Vec<usize>, usize) {
        let Some(from) = self.layer_from(kit, note, layer) else {
            return (Vec::new(), 0);
        };
        let key = (note, layer);
        let samples = &self.browser.library.samples;
        if let Some((_, list, _)) = self.drums.lanes.get(&key)
            && let Some(at) = list
                .iter()
                .position(|&i| samples.get(i).is_some_and(|s| s.path == from))
        {
            let list = list.clone();
            self.drums.lanes.insert(key, (from, list.clone(), at));
            return (list, at);
        }
        let Some(me) = samples.iter().position(|s| s.path == from) else {
            return (Vec::new(), 0);
        };
        let mut list = vec![me];
        list.extend(search::variations(samples, &from, LANE));
        self.drums.lanes.insert(key, (from, list.clone(), 0));
        (list, 0)
    }

    /// A pad's next sound (or previous, `by` -1) in its picked layer's lane: given it, and heard.
    pub(crate) fn step_pad(&mut self, note: u8, by: i32, cx: &mut Context<Self>) {
        let Some(kit) = self.drum_kit() else {
            return;
        };
        let layer = self.pad_layer(kit, note);
        let key = (note, layer);
        let (list, at) = self.pad_lane(kit, note, layer);
        if list.len() < 2 {
            self.status = Some("Nothing in the library sounds like it yet.".into());
            cx.notify();
            return;
        }
        let to = (at as i32 + by).rem_euclid(list.len() as i32) as usize;
        self.set_pad_sample(kit, note, layer, list[to], cx);
        // The lane stays: the sound it's on now is in it.
        if let Some(lane) = self.drums.lanes.get_mut(&key) {
            lane.2 = to;
        } else if let Some(s) = self.browser.library.samples.get(list[to]) {
            self.drums.lanes.insert(key, (s.path.clone(), list, to));
        }
    }

    /// Every pad's next sound (or previous) at once, as one edit: the kit as a whole, changed
    /// (each pad's first layer; what's layered on it stays).
    pub(crate) fn step_kit(&mut self, by: i32, cx: &mut Context<Self>) {
        let Some(kit) = self.drum_kit() else {
            return;
        };
        let notes: Vec<u8> = self
            .session
            .project
            .pads_of(kit)
            .into_iter()
            .filter_map(|t| self.session.project.tracks[t].pad.map(|p| p.note))
            .collect();
        let mut moves = Vec::new();
        for &note in &notes {
            let (list, at) = self.pad_lane(kit, note, 0);
            if list.len() > 1 {
                let to = (at as i32 + by).rem_euclid(list.len() as i32) as usize;
                moves.push((note, list[to], to));
            }
        }
        if moves.is_empty() {
            return;
        }
        let samples = self.browser.library.samples.clone();
        self.edit(cx, |ws| {
            for &(note, index, _) in &moves {
                let (Some(t), Some(s)) = (ws.session.project.pad_on(kit, note), samples.get(index))
                else {
                    continue;
                };
                let name = ws.session.project.tracks[t].name.clone();
                ws.session
                    .project
                    .set_pad_sound(t, 0, search::zone(s, None), &name);
            }
        });
        for (note, _, to) in moves {
            if let Some(lane) = self.drums.lanes.get_mut(&(note, 0)) {
                lane.2 = to;
                if let Some(s) = samples.get(lane.1[to]) {
                    lane.0 = s.path.clone();
                }
            }
        }
        self.status = Some(format!(
            "Every pad's {} sound like it",
            if by > 0 { "next" } else { "last" }
        ));
    }

    /// A menu of the sounds a pad can have: its picked layer's lane (or, empty, the library's of
    /// the kind its note plays), picked to give it.
    pub(crate) fn sound_menu(
        &mut self,
        kit: usize,
        note: u8,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let layer = self.pad_layer(kit, note);
        let (mut list, _) = self.pad_lane(kit, note, layer);
        if list.is_empty() {
            let role = match self.drum_pad(kit, note) {
                Some(_) => self.pad_role(kit, note),
                None => gm_role(note),
            };
            let q = search::Query {
                categories: vec![role],
                kind: Some(pina_engine::samples::Kind::OneShot),
                key: self.sample_key().map(|k| k.name()),
                limit: LANE,
                ..Default::default()
            };
            list = search::find(&self.browser.library.samples, &q)
                .into_iter()
                .map(|h| h.index)
                .collect();
        }
        let samples = &self.browser.library.samples;
        let items: Vec<(String, MenuAction)> = list
            .into_iter()
            .filter_map(|index| {
                let s = samples.get(index)?;
                Some((
                    format!("{}  ·  {}", s.name, s.folder),
                    MenuAction::Drums(DrumOp::Sound { note, layer, index }),
                ))
            })
            .collect();
        if items.is_empty() {
            self.status = Some("The library has nothing of that kind yet.".into());
            cx.notify();
            return;
        }
        self.menu = Some(Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        });
        cx.notify();
    }

    /// A menu of sounds to layer on a pad: what goes with its kind (a clap or a rim on a snare,
    /// a sub under a kick), in the song's key, its own folder's first.
    pub(crate) fn layer_menu(
        &mut self,
        kit: usize,
        note: u8,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let role = self.pad_role(kit, note);
        let folder = self.pad_from(kit, note).and_then(|f| {
            let s = self.browser.library.samples.iter().find(|s| s.path == f)?;
            Some(s.folder.clone())
        });
        let q = search::Query {
            categories: companions(role),
            kind: Some(pina_engine::samples::Kind::OneShot),
            key: self.sample_key().map(|k| k.name()),
            limit: LANE * 2,
            ..Default::default()
        };
        let samples = &self.browser.library.samples;
        let mut hits: Vec<usize> = search::find(samples, &q)
            .into_iter()
            .map(|h| h.index)
            .collect();
        // Stable: the folder's own first, the rest as found.
        hits.sort_by_key(|&i| samples.get(i).map(|s| Some(&s.folder) != folder.as_ref()));
        let items: Vec<(String, MenuAction)> = hits
            .into_iter()
            .filter_map(|index| {
                let s = samples.get(index)?;
                Some((
                    format!(
                        "{}  ·  {}  ·  {}",
                        s.analysis.category.name(),
                        s.name,
                        s.folder
                    ),
                    MenuAction::Drums(DrumOp::Layer { note, index }),
                ))
            })
            .collect();
        if items.is_empty() {
            self.status = Some("The library has nothing to layer on it yet.".into());
            cx.notify();
            return;
        }
        self.menu = Some(Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        });
        cx.notify();
    }

    /// A pad let go: dragged onto another, the two trade places (the beat following them).
    pub(crate) fn drop_pad(
        &mut self,
        note: u8,
        at: gpui::Point<f32>,
        moved: bool,
        cx: &mut Context<Self>,
    ) {
        let (Some(kit), Some(to)) = (self.drum_kit(), self.pad_under(at)) else {
            return;
        };
        if !moved || to == note || self.drum_pad(kit, note).is_none() {
            return;
        }
        self.edit(cx, |ws| ws.session.project.swap_pads(kit, note, to));
        let swap = |n: u8| match n {
            n if n == note => to,
            n if n == to => note,
            n => n,
        };
        let lanes = std::mem::take(&mut self.drums.lanes);
        self.drums.lanes = lanes
            .into_iter()
            .map(|((n, l), v)| ((swap(n), l), v))
            .collect();
        self.drums.layer.0 = swap(self.drums.layer.0);
        self.drums.pad = to;
    }

    pub(crate) fn drum_op(&mut self, op: DrumOp, cx: &mut Context<Self>) {
        let kit = self.drum_kit();
        match op {
            DrumOp::Folder { folder, into } => self.kit_from_folder(&folder, into, cx),
            DrumOp::Picks { folder } => self.folder_picks(&folder, cx),
            DrumOp::Sound { note, layer, index } => {
                if let Some(kit) = kit {
                    self.set_pad_sample(kit, note, layer, index, cx);
                }
            }
            DrumOp::Layer { note, index } => {
                let (Some(kit), Some(s)) = (kit, self.browser.library.samples.get(index)) else {
                    return;
                };
                let zone = search::zone(s, None);
                self.layer_pad(kit, note, zone, cx);
            }
            DrumOp::Unlayer { note, layer } => {
                let Some(t) = kit.and_then(|k| self.session.project.pad_on(k, note)) else {
                    return;
                };
                self.edit(cx, |ws| {
                    ws.session.project.remove_pad_layer(t, layer);
                });
                self.drums.layer = (note, layer.saturating_sub(1));
                // Its layers after it moved down: their lanes are worked out again.
                self.drums.lanes.retain(|&(n, _), _| n != note);
            }
            DrumOp::Layering { note, how } => {
                let Some(t) = kit.and_then(|k| self.session.project.pad_on(k, note)) else {
                    return;
                };
                self.edit_sampler(t, cx, |s| s.set_layering(how));
                if let Some(kit) = kit {
                    self.hit_pad(kit, note, 110);
                }
            }
            DrumOp::Blend { note, on } => {
                let Some(t) = kit.and_then(|k| self.session.project.pad_on(k, note)) else {
                    return;
                };
                self.edit_sampler(t, cx, |s| s.set_blend(on));
            }
            DrumOp::Choke { note, group } => {
                let Some(t) = kit.and_then(|k| self.session.project.pad_on(k, note)) else {
                    return;
                };
                self.edit(cx, |ws| {
                    if let Some(p) = &mut ws.session.project.tracks[t].pad {
                        p.choke = group;
                    }
                });
            }
            DrumOp::Remove { note } => {
                let Some(kit) = kit else {
                    return;
                };
                let Some(t) = self.session.project.pad_on(kit, note) else {
                    return;
                };
                self.edit(cx, |ws| {
                    ws.session.project.tracks.remove(t);
                    ws.region = None;
                });
                self.select_track(self.session.project.kit_of(kit.min(t)).unwrap_or(kit));
                self.drums.lanes.retain(|&(n, _), _| n != note);
            }
        }
        cx.notify();
    }
}
