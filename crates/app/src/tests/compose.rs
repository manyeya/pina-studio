//! Compose: the tracks and bars it writes, the sheet in each of its states, a take heard in the
//! song and kept with one undo, and Settings › Composers. No model runs: the composers' folder
//! and the Hugging Face token are the tests' own, never the person's.

use pina_engine::song::project::{Cycle, Note};

use super::*;
use crate::{
    composer::{Job, Stage, Take, Target},
    pages::settings::Section,
};

/// A composers' folder of the tests' own, with Yellow Small "downloaded" (an empty file: no
/// test runs it), and a Hugging Face folder of their own.
fn test_composers() {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("pina-test-composers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let model = d.join("models").join("midigpt-yellow-small");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("model.safetensors"), b"").unwrap();
        std::fs::create_dir_all(d.join("hf")).unwrap();
        unsafe {
            std::env::set_var("PINA_COMPOSERS", d.join("models"));
            std::env::set_var("HF_HOME", d.join("hf"));
            std::env::remove_var("HF_TOKEN");
        }
        d
    });
}

/// A selector made up as the test goes, for a lookup that keeps it.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The first two tracks that play notes.
fn note_tracks(ws: &Entity<Workspace>, cx: &VisualTestContext) -> (usize, usize) {
    ws.read_with(cx, |ws, _| {
        let mut t = (0..ws.session.project.tracks.len())
            .filter(|&i| ws.session.project.tracks[i].instrument.is_some());
        (t.next().unwrap(), t.next().unwrap())
    })
}

#[gpui::test]
fn it_writes_the_tracks_picked_over_the_bars_looped(cx: &mut TestAppContext) {
    test_composers();
    let (ws, cx) = open(cx);
    let (a, b) = note_tracks(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.track = a;
        ws.tracks_picked.clear();
        ws.region = None;
        ws.session.project.cycle = Some(Cycle {
            start: 8 * BAR + 100,
            end: 12 * BAR,
            off: false,
        });
    });
    // The loop, to whole bars; the selected track.
    let (bars, tracks) = ws.read_with(cx, |ws, _| (ws.compose_bars(), ws.compose_tracks()));
    assert_eq!(bars, ((8, 12), "the loop"));
    assert_eq!(tracks, [a]);
    // ⌘-clicked, both.
    ws.update(cx, |ws, _| {
        let ids: Vec<u64> = ws.session.project.tracks.iter().map(|t| t.id).collect();
        ws.tracks_picked = vec![ids[a], ids[b]];
    });
    assert_eq!(ws.read_with(cx, |ws, _| ws.compose_tracks()), [a, b]);
    // No loop and no region: four bars from the playhead's.
    ws.update(cx, |ws, _| ws.session.project.cycle = None);
    let (bars, from) = ws.read_with(cx, |ws, _| ws.compose_bars());
    assert_eq!((bars.1 - bars.0, from), (4, "the playhead"));
}

#[gpui::test]
fn a_take_is_heard_in_the_song_and_kept_with_one_undo(cx: &mut TestAppContext) {
    test_composers();
    let (ws, cx) = open(cx);
    let (a, _) = note_tracks(&ws, cx);
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[a].id);
    let take = Take {
        seed: 9,
        notes: vec![(
            id,
            vec![Note {
                start: (40 * BAR + 240) as i64,
                length: 480,
                pitch: 61,
                velocity: 90,
                ..Default::default()
            }],
        )],
    };
    let has_take = |p: &pina_engine::song::project::Project| {
        p.tracks[a]
            .regions
            .iter()
            .any(|r| r.name == "Composed" && r.start == 40 * BAR)
    };
    ws.update(cx, |ws, cx| {
        ws.show_bottom(Bottom::Compose, cx);
        ws.composer.target = Some(Target {
            tracks: vec![id],
            bars: (40, 42),
        });
        ws.composer.takes = vec![take.clone(), take];
        ws.compose_pick(1, cx);
    });
    cx.run_until_parked();
    // Picked, it plays in the song; the song itself isn't changed.
    ws.read_with(cx, |ws, _| {
        assert!(has_take(&ws.engine_project()));
        assert!(!has_take(&ws.session.project));
    });
    assert!(cx.debug_bounds("compose-take-1").is_some());
    assert!(cx.debug_bounds(leak(format!("compose-lane-{a}"))).is_some());
    // Compose closed, the song plays as it is.
    click_on(cx, "close-editor", Modifiers::none(), MouseButton::Left);
    assert!(!ws.read_with(cx, |ws, _| has_take(&ws.engine_project())));
    ws.update(cx, |ws, cx| ws.show_bottom(Bottom::Compose, cx));
    cx.run_until_parked();
    // Kept: in the song, and one undo takes it back out.
    click_on(cx, "compose-keep", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        assert!(has_take(&ws.session.project));
        assert_eq!(ws.composer.kept, Some(1));
        assert_eq!(ws.composer.picked, None);
    });
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert!(!ws.read_with(cx, |ws, _| has_take(&ws.session.project)));
}

#[gpui::test]
fn the_sheet_shows_where_it_is_at_every_step(cx: &mut TestAppContext) {
    test_composers();
    let (ws, cx) = open(cx);
    let (a, _) = note_tracks(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.track = a;
        ws.tracks_picked.clear();
        ws.show_bottom(Bottom::Compose, cx);
    });
    cx.run_until_parked();
    // A model's there: Write, its controls, the part's dropdown.
    assert!(cx.debug_bounds("compose-write").is_some());
    assert!(cx.debug_bounds("compose-model").is_some());
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[a].id);
    assert!(
        cx.debug_bounds(leak(format!("compose-part-{id}")))
            .is_some()
    );
    // The part heard as something else, from its menu.
    click_on(
        cx,
        leak(format!("compose-part-{id}")),
        Modifiers::none(),
        MouseButton::Left,
    );
    pick(cx, &ws, "Strings");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.composer.parts.get(&id).copied()),
        Some(pina_engine::composer::score::Part::Strings)
    );
    // Writing: where it's got, the notes so far, and Stop.
    ws.update(cx, |ws, cx| {
        ws.composer.job = Some(Job {
            stage: Stage::Writing { take: 2, of: 4 },
            started: Instant::now(),
            stop: Default::default(),
            run: 0,
            tokens: 412,
            live: vec![(
                id,
                Note {
                    start: 0,
                    length: 240,
                    pitch: 40,
                    velocity: 90,
                    ..Default::default()
                },
            )],
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("compose-stop").is_some());
    assert!(cx.debug_bounds("compose-writing").is_some());
    assert!(cx.debug_bounds("compose-write").is_none());
    click_on(cx, "compose-stop", Modifiers::none(), MouseButton::Left);
    let stopped = ws.read_with(cx, |ws, _| {
        ws.composer
            .job
            .as_ref()
            .is_some_and(|j| j.stop.load(std::sync::atomic::Ordering::Relaxed))
    });
    assert!(stopped);
    // Its Python being made: uv's line said.
    ws.update(cx, |ws, cx| {
        if let Some(j) = ws.composer.job.as_mut() {
            j.stage = Stage::Python("Downloading torch (73.2MiB)".into());
        }
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("compose-stage").is_some());
    ws.update(cx, |ws, _| ws.composer.job = None);
    // No model downloaded: the steps to get one, in place.
    let model = std::env::var_os("PINA_COMPOSERS")
        .map(PathBuf::from)
        .unwrap();
    let file = model.join("midigpt-yellow-small").join("model.safetensors");
    let _ = std::fs::rename(&file, file.with_extension("away"));
    ws.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let setup = cx.debug_bounds("compose-download").is_some();
    let _ = std::fs::rename(file.with_extension("away"), &file);
    assert!(setup);
}

#[gpui::test]
fn settings_signs_in_and_lists_the_models(cx: &mut TestAppContext) {
    test_composers();
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.show_settings(Section::Composers, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("composer-download-1").is_some());
    assert!(cx.debug_bounds("composers-device").is_some());
    // The device: the CPU, as its menu (below the fold here) sets it.
    ws.update(cx, |ws, cx| {
        ws.compose_act(crate::composer::Act::OnCpu(true), cx)
    });
    assert!(ws.read_with(cx, |ws, _| ws.settings.compose_on_cpu));
    // A token kept, in the tests' own Hugging Face folder.
    ws.update(cx, |ws, cx| ws.compose_save_token("hf_test", cx));
    assert!(ws.update(cx, |ws, _| ws.composer.signed_in()));
    let hf = std::env::var_os("HF_HOME").map(PathBuf::from).unwrap();
    assert_eq!(
        std::fs::read_to_string(hf.join("token")).unwrap(),
        "hf_test"
    );
}
