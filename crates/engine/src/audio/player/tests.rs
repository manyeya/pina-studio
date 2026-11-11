use super::*;
use crate::song::project::Track;

/// With a real instrument, which CI doesn't have:
/// `PINA_TEST_VST3="/path/Some Synth.vst3" cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn plugin_settings_are_saved_with_the_project_and_loaded_back() {
    let path = std::env::var("PINA_TEST_VST3").expect("set PINA_TEST_VST3");
    let class = crate::plugin::scan::inspect(Path::new(&path))
        .unwrap()
        .remove(0)
        .class;
    let dir = std::env::temp_dir().join(format!("pina-test-states-{}", std::process::id()));
    let plugin = PluginRef {
        path: path.into(),
        class,
        state: None,
        ..Default::default()
    };
    let mut project = Project::new(48_000);
    project.add_track(
        0,
        Track {
            name: "Synth".into(),
            instrument: Some(Instrument::Plugin(plugin)),
            ..Default::default()
        },
    );
    let mut player = Player::silent(&project, &dir);
    player.sync(&project).unwrap();
    player.save_states(&mut project).unwrap();
    let Some(Instrument::Plugin(r)) = &project.tracks[0].instrument else {
        unreachable!()
    };
    let file = r.state.clone().expect("a state file");
    assert!(dir.join(&file).exists());

    // A parameter the project sets is applied; when it stops setting it (an undo), it goes
    // back to what it was.
    let id = project.tracks[0].id;
    let param = player.params(id, Which::Instrument)[0].clone();
    let set = |project: &mut Project, v: Option<f64>| {
        let r = project.tracks[0].plugin_mut(Which::Instrument).unwrap();
        match v {
            Some(v) => drop(r.params.insert(param.id, v)),
            None => drop(r.params.remove(&param.id)),
        }
    };
    let other = if param.value > 0.5 { 0.1 } else { 0.9 };
    set(&mut project, Some(other));
    player.sync(&project).unwrap();
    assert!((player.param_value(id, Which::Instrument, param.id) - other).abs() < 0.02);
    set(&mut project, None);
    player.sync(&project).unwrap();
    let back = player.param_value(id, Which::Instrument, param.id);
    assert!((back - param.value).abs() < 0.02, "{back} {}", param.value);
    // A bounce of the saved project loads it.
    project.save(&dir).unwrap();
    let loaded = Project::load(&dir).unwrap();
    assert!(Engine::load(&loaded, &dir, 48_000.0).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn running_plugins_are_matched_to_where_they_are_now() {
    assert_eq!(
        matching(&['a', 'b', 'c'], &['c', 'a', 'b']),
        [Some(2), Some(0), Some(1)]
    );
    // The same plugin twice keeps the order they came in; one new is none.
    assert_eq!(
        matching(&['a', 'a', 'b'], &['b', 'a', 'a']),
        [Some(2), Some(0), Some(1)]
    );
    assert_eq!(
        matching(&['a', 'b'], &['a', 'c', 'b']),
        [Some(0), None, Some(1)]
    );
}

#[test]
fn a_song_opens_a_channel_at_a_time_and_its_files_read_off_the_main_thread() {
    use crate::song::project::{Content, Follow, PPQ, Region, StretchMode};
    let dir = std::env::temp_dir().join(format!("pina-test-opening-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    for name in ["a.wav", "b.wav"] {
        let mut w = hound::WavWriter::create(dir.join(name), spec).unwrap();
        (0..4800).for_each(|i| w.write_sample((i as f32 * 0.01).sin()).unwrap());
        w.finalize().unwrap();
    }
    let audio = |file: &str| Region {
        length: PPQ,
        content: Content::Audio {
            file: file.into(),
            offset: 0,
            stretch: 1.0,
            stretch_mode: StretchMode::default(),
            follow: Follow::Off,
            tempo: None,
            grid: None,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let mut project = Project::new(48_000);
    for files in [&["a.wav", "b.wav"][..], &["a.wav"]] {
        project.add_track(
            project.tracks.len(),
            Track {
                regions: files.iter().map(|f| audio(f)).collect(),
                ..Default::default()
            },
        );
    }
    let mut player = Player::silent(&project, &dir);
    // Each track, then the master, then done.
    let ids: Vec<u64> = std::iter::from_fn(|| player.load_next(&project)).collect();
    let want: Vec<u64> = project
        .tracks
        .iter()
        .map(|t| t.id)
        .chain([MASTER_ID])
        .collect();
    assert_eq!(ids, want);
    // Each file once, read elsewhere and kept: none left to read, nothing missing.
    let files = player.unread_files(&project);
    assert_eq!(files, [PathBuf::from("a.wav"), PathBuf::from("b.wav")]);
    let reader = player.file_reader();
    let read = std::thread::spawn(move || {
        files
            .into_iter()
            .map(|f| {
                let r = reader.read(&f).unwrap();
                (f, r)
            })
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    for (f, r) in read {
        player.keep_file(f, r);
    }
    assert!(player.unread_files(&project).is_empty());
    assert!(player.peaks(Path::new("b.wav")).is_some());
    player.sync(&project).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
