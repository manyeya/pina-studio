//! The pages: song study, playbooks, settings and songs.

use super::*;

/// Sets up the page, if it's one of these; says whether it was.
pub(super) fn set_up(
    shot: &Shot,
    ws: &mut Workspace,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Workspace>,
) -> bool {
    let Shot { page, song, out } = *shot;
    let _ = (song, out, &window);
    match page {
        "study" => {
            ws.open_study(cx);
            if let Some(want) = &song {
                ws.studies.open = ws
                    .studies
                    .list
                    .iter()
                    .find(|s| s.file.to_string_lossy().contains(want))
                    .map(|s| s.file.clone());
            }
        }
        // A picture of a spectrogram (drawn from the demo's render, as a plotting tool exports
        // one), studied: as it reads on its own, with its axes given, Create a song like this
        // over it, and the demo against it. An audio study's blueprint, of the demo's render.
        // Studies are kept with the shots, never the person's.
        "study-picture" | "study-picture-read" | "study-create" | "study-compare"
        | "study-like" => {
            use pina_engine::study::reference::{
                Calibration, Closeness, Colors, FreqScale, Like, Source,
            };
            let kept = out.join("studies");
            let _ = std::fs::create_dir_all(&kept);
            unsafe { std::env::set_var("PINA_STUDIES", &kept) };
            let p = ws.session.project.clone();
            let dir = ws.session.dir.clone();
            let [l, _] = crate::cli::rendered(&p, &dir, None).unwrap();
            let seconds = l.len() as f32 / p.sample_rate as f32;
            if page == "study-like" {
                let wav = out.join("Demo render.wav");
                pina_engine::music::analysis::write_wav(&wav, &l, &l, p.sample_rate).unwrap();
                crate::cli::study::study(&wav, false).unwrap();
                ws.open_study(cx);
                ws.studies.open = Some(wav);
                return true;
            }
            let png = out.join("Reference spectrogram.png");
            spectrogram_png(&l, p.sample_rate as f32, &png, page == "study-compare");
            let given = Calibration {
                seconds: Some(seconds.round()),
                low_hz: Some(40.0),
                high_hz: Some(16_000.0),
                scale: Some(FreqScale::Log),
                ..Default::default()
            };
            ws.open_study(cx);
            match page {
                "study-picture" => ws.read_picture(png.clone(), Calibration::default(), false, cx),
                _ => ws.read_picture(png.clone(), given.clone(), false, cx),
            }
            if page == "study-create" {
                ws.open_create(png.clone(), cx);
            }
            if page == "study-compare" {
                ws.session.project.like = Some(Like {
                    file: png,
                    source: Source::Image,
                    calibration: Some(Calibration {
                        colors: Some(Colors::Magma),
                        ..given
                    }),
                    style: String::new(),
                    rhythm: Closeness::Close,
                    form: Closeness::Close,
                    sound: Closeness::Close,
                });
                ws.compare_like(cx);
            }
        }
        "study-empty" => {
            ws.open_study(cx);
            ws.studies.list.clear();
            ws.studies.open = None;
        }
        // PINA_SHOT_SONG: the book that study would make, shown and never saved.
        "playbooks" | "playbooks-sound" => {
            ws.open_genres(None, cx);
            ws.open_study(cx);
            let study = song.and_then(|want| {
                let mut found = ws.studies.list.iter();
                found
                    .find(|s| s.file.to_string_lossy().contains(want))
                    .cloned()
            });
            if let Some(s) = study {
                ws.genres.draft = Some(pina_engine::study::to_playbook(&s, &[]));
            }
            ws.view = View::Playbooks;
            ws.genre_fields(cx);
            if page == "playbooks-sound" {
                ws.genres.tab = crate::pages::genres::BookTab::Sound;
            }
        }
        // The tracks zoomed in on a song longer than the screen, scrolled into it.
        "arrange-long" => {
            let bar = ws.session.project.bar();
            for t in &mut ws.session.project.tracks {
                t.height = Some(300.0);
                for r in &mut t.regions {
                    r.length = r.length.max(bar * 24);
                }
            }
            ws.px_per_beat = 90.0;
            ws.scroll_x = 1200.0;
            cx.notify();
        }
        // The shortcuts sheet, whole and searched.
        "shortcuts" | "shortcuts-search" => {
            ws.toggle_shortcuts(window, cx);
            if page == "shortcuts-search" {
                let find = ws.shortcuts.as_ref().unwrap().find.clone();
                find.update(cx, |f, cx| f.set_text("dup", cx));
            }
        }
        "settings-audio" => {
            ws.settings_section = crate::pages::settings::Section::Audio;
            ws.outputs = vec!["MacBook Pro Speakers".into(), "Scarlett 2i2 USB".into()];
            ws.buffers = vec![64, 128, 256, 512, 1024, 2048];
            ws.settings.audio.buffer = Some(512);
            ws.open_settings(cx);
            // A laptop with Bluetooth headphones on, as it would read.
            use pina_engine::audio::engine::DeviceInfo;
            let d = |name: &str, channels, rates: &[u32], own| DeviceInfo {
                name: name.into(),
                channels,
                rates: rates.to_vec(),
                own_rate: Some(own),
                bluetooth: name == "Headphones",
            };
            // PINA_SHOT_CALL: the headphones on a call, their microphone open.
            let on_call = std::env::var("PINA_SHOT_CALL").is_ok();
            ws.outputs_info = vec![
                match on_call {
                    true => d("Headphones", 1, &[], 16_000),
                    false => d("Headphones", 2, &[44_100], 44_100),
                },
                d(
                    "Laptop Speakers",
                    2,
                    &[44_100, 48_000, 88_200, 96_000],
                    48_000,
                ),
                d(
                    "Audio Interface",
                    8,
                    &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
                    48_000,
                ),
            ];
            ws.inputs_info = vec![
                d("Laptop Microphone", 1, &[44_100, 48_000, 96_000], 48_000),
                d("Headphones", 1, &[], 16_000),
            ];
            ws.in_levels = vec![0.12];
        }
        // About Pina Studio, its list of every crate open or not.
        "settings-about" | "settings-about-all" => {
            ws.settings_section = crate::pages::settings::Section::About;
            ws.about_all = page == "settings-about-all";
            ws.open_settings(cx);
        }
        "settings-recording" | "settings-plugins" | "settings-sounds" | "settings-models"
        | "settings-saving" => {
            use crate::pages::settings::Section;
            ws.settings_section = match page {
                "settings-recording" => Section::Recording,
                "settings-plugins" => Section::Plugins,
                "settings-sounds" => Section::Sounds,
                "settings-models" => Section::Models,
                _ => Section::Saving,
            };
            ws.settings.count_in = 1;
            ws.settings.click.sound = 1;
            ws.open_settings(cx);
        }
        // Flip: the demo's drums sliced into eight chops on a Sampler track, flipped, a chop
        // picked.
        "sampler-flip" => {
            let p = &ws.session.project;
            if let Some(t) = p.tracks.iter().position(|t| t.name == "Drums") {
                let file = match &p.tracks[t].regions[0].content {
                    pina_engine::song::project::Content::Audio { file, .. } => file.clone(),
                    _ => Default::default(),
                };
                let rate = p.sample_rate as f64;
                let read = pina_engine::audio::engine::read_audio(&ws.session.dir.join(&file));
                if let Ok((sides, own)) = read {
                    let frames = (sides[0].len() as f64 * rate / own) as u64;
                    let cuts: Vec<u64> = (1..8).map(|k| k * frames / 8).collect();
                    let made = ws
                        .session
                        .project
                        .sampler_from_audio((t, 0), frames, Some(&cuts));
                    if let Some(s) = made {
                        ws.session.project.flip_chops((s, 0), 3);
                        ws.select_track(s);
                        ws.region = Some((s, 0));
                        ws.sampler_ui.chop = Some(2);
                        ws.changed(cx);
                        ws.show_bottom(crate::workspace::Bottom::Sampler, cx);
                    }
                }
            }
        }
        // The Separate Stems sheet on the demo's drums, with the models there are (PINA_MODELS for
        // a folder of your own); "-running", as if it were a third of the way through.
        "stems-sheet" | "stems-sheet-running" => {
            let p = &ws.session.project;
            if let Some(t) = p.tracks.iter().position(|t| t.name == "Drums") {
                ws.open_stems((t, 0), cx);
                if page == "stems-sheet-running"
                    && let Some(model) = ws.stems_sheet.as_ref().and_then(|s| s.model.clone())
                {
                    ws.separating = Some(crate::editors::audio::Separating {
                        model,
                        at: (t, 0),
                        done: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(340)),
                        stop: Default::default(),
                        started: std::time::Instant::now() - std::time::Duration::from_secs(95),
                        kept: false,
                    });
                }
            }
        }
        // The Audio editor on the demo's drums, lyrics beside it as if heard (or, "-get",
        // waiting for Whisper to download).
        "audio-lyrics" | "audio-lyrics-get" | "audio-flips" | "audio-flips-finding" => {
            use pina_engine::lyrics::Line;
            let p = &ws.session.project;
            if let Some(t) = p.tracks.iter().position(|t| t.name == "Drums") {
                let file = match &p.tracks[t].regions[0].content {
                    pina_engine::song::project::Content::Audio { file, .. } => file.clone(),
                    _ => Default::default(),
                };
                // Its words spread across it, as Whisper would hear them.
                let line = |text: &str, start: f32, end: f32, sung: usize| {
                    let said: Vec<&str> = text.split_whitespace().collect();
                    let each = (end - start) / said.len().max(1) as f32;
                    Line {
                        text: text.into(),
                        start,
                        end,
                        sung,
                        words: said
                            .iter()
                            .enumerate()
                            .map(|(k, w)| pina_engine::lyrics::Word {
                                text: w.to_string(),
                                start: start + k as f32 * each,
                                end: start + (k + 1) as f32 * each,
                            })
                            .collect(),
                    }
                };
                ws.lyrics = Some(crate::editors::audio::Lyrics::shown(
                    file,
                    "Whisper large-v3 turbo",
                    vec![
                        line("Ain't no holding back", 0.05, 0.75, 2),
                        line("Because you are all I see", 0.8, 1.55, 1),
                        line("Ain't no holding back", 1.6, 2.35, 2),
                    ],
                ));
                ws.select_track(t);
                ws.region = Some((t, 0));
                ws.show_bottom(crate::workspace::Bottom::Audio, cx);
                // Tall enough to show the hooks and every line under them.
                ws.editor_h = ws.editor_h.max(560.0);
                if page.starts_with("audio-flips") {
                    use pina_engine::lyrics::flips::{Flip, Hit};
                    let hit = |from: f32, to: f32, beat: f32, beats: f32, pitch: i8, reverse| Hit {
                        from,
                        to,
                        beat,
                        beats,
                        pitch,
                        reverse,
                        words: String::new(),
                    };
                    let flip =
                        |name: &str, style: &str, says: &str, why: &str, bars, pitch, hits| Flip {
                            name: name.into(),
                            style: style.into(),
                            says: says.into(),
                            why: why.into(),
                            bars,
                            pitch,
                            hits,
                        };
                    let ideas = vec![
                        flip(
                            "No Holding",
                            "chipmunk",
                            "ain't no holding back, all I see",
                            "Up five, the line turns to soul, and 'all I see' answers it on bar two.",
                            2,
                            5,
                            vec![
                                hit(0.05, 0.75, 0.0, 3.5, 0, false),
                                hit(1.175, 1.55, 4.0, 2.5, 0, false),
                                hit(0.575, 0.75, 7.0, 1.0, 0, true),
                            ],
                        ),
                        flip(
                            "I See You",
                            "chop",
                            "I see you, back, back, holding back",
                            "Three words from two lines say something new, and the low 'holding back' drops it into the loop.",
                            1,
                            0,
                            vec![
                                hit(1.3, 1.55, 0.0, 1.0, 0, false),
                                hit(0.925, 1.05, 1.0, 0.5, 0, false),
                                hit(0.575, 0.75, 1.5, 0.5, 0, false),
                                hit(0.575, 0.75, 2.0, 0.5, 0, false),
                                hit(0.4, 0.75, 2.5, 1.5, -12, false),
                            ],
                        ),
                        flip(
                            "All All All",
                            "stutter",
                            "all all all all, all I see",
                            "The stutter climbs on sixteenths and throws the whole line on the one.",
                            2,
                            0,
                            vec![
                                hit(1.175, 1.55, 0.0, 3.0, 0, false),
                                hit(1.175, 1.3, 6.0, 0.25, 0, false),
                                hit(1.175, 1.3, 6.25, 0.25, 0, false),
                                hit(1.175, 1.3, 6.5, 0.25, 2, false),
                                hit(1.175, 1.3, 6.75, 0.25, 4, false),
                                hit(1.175, 1.55, 7.0, 1.0, 7, false),
                            ],
                        ),
                    ];
                    let heard = ws
                        .lyrics
                        .as_ref()
                        .map(|l| l.heard.clone())
                        .unwrap_or_default();
                    let finding = page == "audio-flips-finding";
                    let mut flips = crate::editors::audio::Flips::shown(
                        heard,
                        match finding {
                            true => ideas[..2].to_vec(),
                            false => ideas,
                        },
                        finding.then_some((
                            crate::agent::Kind::ClaudeCode,
                            crate::editors::audio::Stage::Writing,
                            23,
                        )),
                    );
                    if !finding {
                        flips.playing = Some((
                            1,
                            std::time::Instant::now() - std::time::Duration::from_millis(900),
                            8.0,
                            2.4,
                        ));
                    }
                    ws.flips.now = Some(flips);
                    ws.flips.tab = crate::editors::audio::LyricsTab::Flips;
                    ws.flips.style = Some("any");
                }
                if page == "audio-lyrics-get"
                    && let Some(l) = ws.lyrics.take()
                {
                    ws.lyrics_wait = Some(crate::editors::audio::LyricsWait {
                        file: l.file,
                        at: (t, 0),
                        hearing: None,
                        failed: None,
                    });
                }
            }
        }
        "audio-slices" | "audio-slices-voice" => {
            let p = &ws.session.project;
            if let Some(t) = p.tracks.iter().position(|t| t.name == "Drums") {
                let file = match &p.tracks[t].regions[0].content {
                    pina_engine::song::project::Content::Audio { file, .. } => file.clone(),
                    _ => Default::default(),
                };
                ws.select_track(t);
                ws.region = Some((t, 0));
                ws.editor_h = ws.editor_h.max(560.0);
                ws.show_bottom(crate::workspace::Bottom::Audio, cx);
                let voice = page == "audio-slices-voice";
                let (sides, rate) =
                    pina_engine::audio::engine::read_audio(&ws.session.dir.join(&file)).unwrap();
                let stop = std::sync::atomic::AtomicBool::new(false);
                let mut slices = pina_engine::smart::slices(&sides, rate, voice, |_| {}, &stop);
                match voice {
                    // A voice alone, its words heard: its chops named by them.
                    true => {
                        let sung = ["ain't", "no", "holding", "back", "all", "I", "see", "you"];
                        for (s, w) in slices.iter_mut().zip(sung) {
                            s.name = w.into();
                        }
                    }
                    // One with the voice in it, as a song's would be.
                    false => {
                        if let Some(s) = slices.get_mut(3) {
                            s.voice = true;
                        }
                    }
                }
                ws.beat = Some(crate::editors::audio::Beat {
                    at: (t, 0),
                    file: file.clone(),
                    cut: file,
                    heard: match voice {
                        true => pina_engine::smart::InIt::Voice,
                        false => pina_engine::smart::InIt::Mixed,
                    },
                    guessed: false,
                    slices,
                    audio: std::sync::Arc::new((sides, rate)),
                    unvoiced: false,
                    filter: None,
                    playing: Some((
                        2,
                        std::time::Instant::now() + std::time::Duration::from_secs(9),
                    )),
                });
            }
        }
        "settings-midi" => {
            ws.settings_section = crate::pages::settings::Section::Midi;
            ws.player.list_midi_input("MPE Controller");
            ws.player.list_midi_input("USB Keyboard");
            ws.open_settings(cx);
        }
        "settings-auto" => {
            ws.settings_section = crate::pages::settings::Section::Automation;
            ws.open_settings(cx);
        }
        // The sample browser's page, on the library as it was last read.
        "samples" | "samples-kit" | "samples-kit-folder" => {
            ws.set_library(pina_engine::samples::library::Library::load(), cx);
            if page == "samples-kit-folder" {
                // A kit in the song, picked, to load the folder's sounds into.
                ws.new_drum_kit(cx);
                if let Some(kit) = ws.drum_kit() {
                    ws.fit_kit(kit, cx);
                }
            }
            ws.toggle_samples_page(cx);
            match page {
                "samples-kit" => {
                    ws.browser.view = crate::sounds::browser::Results::Kit;
                    ws.build_kit();
                }
                "samples-kit-folder" => {
                    ws.browser.view = crate::sounds::browser::Results::Kit;
                    ws.folder_picks("MusicRadar Hardware Drum Machine/Hits/Jomox Airbase 99", cx);
                }
                _ => {}
            }
        }
        // MIDI files in the browser, the library's studied as they're read (nothing written):
        // the page, a file picked; and the compact browser beside the tracks.
        "samples-midi" | "panel-midi" | "samples-midi-map" | "samples-midi-kit" => {
            use pina_engine::samples::midi;
            let mut library = pina_engine::samples::library::Library::load();
            library.midi = library
                .folders
                .iter()
                .flat_map(|root| {
                    midi::midi_files(root)
                        .into_iter()
                        .filter_map(|f| midi::study_file(&f, root, 0, 0).ok())
                })
                .collect();
            ws.set_library(library, cx);
            ws.browser.midi_map = midi::places(&ws.browser.library.midi);
            match page {
                "panel-midi" => {
                    ws.toggle_samples(cx);
                    ws.browse(crate::sounds::catalog::Scope::Sounds, cx);
                }
                _ => ws.toggle_samples_page(cx),
            }
            ws.browser.play_in_key = true;
            ws.browser.view = match page {
                "samples-midi-map" => crate::sounds::browser::Results::Map,
                "samples-midi-kit" => crate::sounds::browser::Results::Kit,
                _ => crate::sounds::browser::Results::List,
            };
            ws.show_midi(true, cx);
            if let Some(h) = ws.browser.midi_hits.get(2) {
                ws.browser.midi_selected = Some(h.index);
            }
        }
        // The compact browser's sounds, each with its play button.
        "panel-sounds" | "panel-sounds-picked" | "samples-picked" => {
            let library = pina_engine::samples::library::Library::load();
            ws.set_library(library, cx);
            match page {
                "samples-picked" => ws.toggle_samples_page(cx),
                _ => {
                    ws.toggle_samples(cx);
                    ws.browse(crate::sounds::catalog::Scope::Sounds, cx);
                }
            }
            // A loop picked: its card, with Add to song and Slice.
            let samples = &ws.browser.library.samples;
            let loop_at = samples
                .iter()
                .position(|s| s.analysis.kind == pina_engine::samples::Kind::Loop);
            if page != "panel-sounds"
                && let Some(i) = loop_at
            {
                ws.select_sample(i, false, cx);
            }
        }
        "settings" => {
            ws.settings.hidden = vec!["score".into(), "takes".into(), "live_loops".into()];
            ws.open_settings(cx);
        }
        // Modulation: an LFO, steps, an envelope, a follower and random on the keys,
        // mapped to its pan, volume, low cut and presence; values as if playing.
        // The loading screen, a third of the way through opening the song shown (with
        // PINA_SHOT_PROJECT, a big one of your own); "opening-start" before anything's in.
        "opening" | "opening-start" => {
            let session = Session::open(&ws.session.dir).unwrap();
            let player = Player::silent(&session.project, &session.dir);
            let mut o = crate::pages::projects::opening::Opening::new(ws, session, player);
            if page == "opening" {
                o.steps_for_shot(ws.session.project.tracks.len() / 3 + 1);
            }
            o.began(std::time::Duration::from_millis(1700));
            ws.loading = Some(o);
            cx.notify();
        }
        // "songs-new": the new song's panel up over them; "songs-first": no songs yet.
        "home" | "songs" | "songs-grid" | "songs-picked" | "songs-new" | "songs-first" => {
            // A library's worth: tempos, keys, times and arrangements of every size.
            let names = [
                ("Night Drive", 92.0, 0, Some("A minor"), [4, 4], 7, 192.0),
                (
                    "Rain on Glass",
                    74.0,
                    3_600,
                    Some("Db major"),
                    [6, 8],
                    5,
                    168.0,
                ),
                ("Untitled 9", 120.0, 7_200, None, [4, 4], 2, 16.0),
                (
                    "Lo-fi Sketch",
                    84.0,
                    90_000,
                    Some("F major"),
                    [4, 4],
                    4,
                    90.0,
                ),
                (
                    "Warehouse",
                    128.0,
                    200_000,
                    Some("G minor"),
                    [4, 4],
                    9,
                    364.0,
                ),
                (
                    "Morning Pages",
                    100.0,
                    400_000,
                    Some("C major"),
                    [3, 4],
                    6,
                    220.0,
                ),
                (
                    "Glass Harbour",
                    110.0,
                    800_000,
                    Some("E minor"),
                    [4, 4],
                    8,
                    262.0,
                ),
                (
                    "Paper Lanterns",
                    68.0,
                    1_100_000,
                    Some("Bb major"),
                    [6, 8],
                    5,
                    185.0,
                ),
                (
                    "Second Wind",
                    140.0,
                    1_500_000,
                    Some("F# minor"),
                    [4, 4],
                    7,
                    176.0,
                ),
                (
                    "Tidepool",
                    96.0,
                    2_900_000,
                    Some("Ab major"),
                    [7, 8],
                    6,
                    213.0,
                ),
                (
                    "Copper Sky",
                    118.0,
                    3_500_000,
                    Some("D minor"),
                    [4, 4],
                    8,
                    310.0,
                ),
                (
                    "Slow Bloom",
                    80.0,
                    4_100_000,
                    Some("C# minor"),
                    [4, 4],
                    5,
                    140.0,
                ),
            ];
            let colors = [
                0xff7a59, 0xa78bfa, 0x3ddc97, 0x38bdf8, 0x2dd4bf, 0xf472b6, 0xffc53d, 0xfb923c,
                0x94a3b8,
            ];
            ws.songs = names
                .iter()
                .enumerate()
                .map(
                    |(n, &(name, tempo, ago, key, signature, tracks, seconds))| {
                        let mut seed = 0x9e37_79b9u32.wrapping_mul(n as u32 + 7);
                        let mut next = move || {
                            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                            (seed >> 8) as f32 / (1u32 << 24) as f32
                        };
                        let lanes = (0..tracks)
                            .map(|i| {
                                let mut spans = Vec::new();
                                let mut at = if i < 2 { 0 } else { (next() * 4.0) as usize };
                                while at < 16 {
                                    if next() < 0.8 {
                                        let len = (2 + (next() * 4.0) as usize).min(16 - at);
                                        spans.push((at as f32 / 16.0, (at + len) as f32 / 16.0));
                                        at += len;
                                    } else {
                                        at += 1 + (next() * 2.0) as usize;
                                    }
                                }
                                (colors[i % colors.len()], spans)
                            })
                            .collect();
                        crate::pages::projects::Song {
                            dir: std::path::PathBuf::from(format!("/songs/{name}")),
                            name: name.to_string(),
                            modified: std::time::SystemTime::now()
                                - std::time::Duration::from_secs(ago),
                            tempo,
                            key: key.map(String::from),
                            signature,
                            tracks,
                            seconds,
                            lanes,
                        }
                    },
                )
                .collect();
            if page.ends_with("-grid") {
                ws.settings.songs_grid = true;
            }
            if page == "songs-first" {
                ws.songs.clear();
            }
            if page == "songs-new" || page == "songs-first" {
                ws.show_home(true, window, cx);
            }
            if page.ends_with("-picked") {
                ws.show_home(false, window, cx);
                if let Some(h) = ws.home.as_mut() {
                    h.picks = vec!["/songs/Warehouse".into(), "/songs/Tidepool".into()];
                }
            }
            ws.session.on_disk = page.starts_with("songs");
            ws.view = View::Songs;
            cx.notify();
        }
        // The Tool menu open on the Fade tool, the drums faded in and out.
        _ => return false,
    }
    true
}

/// A spectrogram of `x` drawn as a plotting tool exports one: a white margin with tick marks, a
/// black frame, magma colours, 40 Hz to 16 kHz up a log axis. `darker` takes the top end down
/// over its last half, as a darker reference would have it.
fn spectrogram_png(x: &[f32], rate: f32, file: &std::path::Path, darker: bool) {
    use pina_engine::{
        samples::dsp::Spectra,
        study::reference::{Colors, FreqScale, image},
    };
    let (w, h) = (900u32, 300u32);
    let (left, top, right, bottom) = (56u32, 24u32, 24u32, 44u32);
    let size = 4096usize;
    let mut spectra = Spectra::new(size);
    let mut power = vec![0f32; size / 2 + 1];
    let bin = rate / size as f32;
    let mut db = vec![0f32; (w * h) as usize];
    for col in 0..w {
        let at = (col as f32 + 0.5) / w as f32 * x.len() as f32;
        spectra.power(x, at as isize - (size / 2) as isize, &mut power);
        for row in 0..h {
            let up = 1.0 - (row as f32 + 0.5) / h as f32;
            let hz = FreqScale::Log.hz(40.0, 16_000.0, up);
            let k = (hz / bin).clamp(1.0, (size / 2 - 1) as f32);
            let (i, f) = (k.floor() as usize, k.fract());
            let p = power[i] * (1.0 - f) + power[i + 1] * f;
            let mut v = 10.0 * p.max(1e-12).log10();
            if darker && col > w / 2 && hz > 3000.0 {
                v -= 15.0;
            }
            db[(row * w + col) as usize] = v;
        }
    }
    let top_db = db.iter().copied().fold(f32::MIN, f32::max);
    let t = image::table(Colors::Magma);
    let (pw, ph) = (w + left + right, h + top + bottom);
    let mut pixels = vec![[255u8; 3]; (pw * ph) as usize];
    for row in 0..h {
        for col in 0..w {
            let v = ((db[(row * w + col) as usize] - (top_db - 70.0)) / 70.0).clamp(0.0, 1.0);
            pixels[((row + top) * pw + col + left) as usize] =
                t[(v * 63.0) as usize].map(|c| c as u8);
        }
    }
    for xx in left - 1..=left + w {
        pixels[((top - 1) * pw + xx) as usize] = [0; 3];
        pixels[((top + h) * pw + xx) as usize] = [0; 3];
    }
    for yy in top - 1..=top + h {
        pixels[(yy * pw + left - 1) as usize] = [0; 3];
        pixels[(yy * pw + left + w) as usize] = [0; 3];
    }
    // Tick marks, and blocks where a tool would print its labels.
    for k in 0..=10 {
        let xx = left + k * w / 10;
        for d in 0..6 {
            pixels[((top + h + 1 + d) * pw + xx.min(pw - 1)) as usize] = [0; 3];
        }
        for yy in top + h + 12..top + h + 20 {
            for dx in 0..14 {
                pixels[(yy * pw + (xx + dx).saturating_sub(7).min(pw - 1)) as usize] = [90; 3];
            }
        }
    }
    for k in 0..=5 {
        let yy = top + k * h / 5;
        for d in 0..6 {
            pixels[(yy.min(ph - 1) * pw + left - 2 - d) as usize] = [0; 3];
        }
        for dy in 0..8 {
            for xx in 14..36 {
                pixels[((yy + dy).saturating_sub(4).min(ph - 1) * pw + xx) as usize] = [90; 3];
            }
        }
    }
    image::save(
        &image::Picture {
            width: pw,
            height: ph,
            pixels,
        },
        file,
    )
    .unwrap();
}
