use std::sync::atomic::AtomicBool;

use super::*;

/// A sound with something in every band: a few tones and a click train.
fn sound(len: usize, seed: f32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32 / 44_100.0;
            let tones = [110.0, 440.0, 3_000.0, 9_000.0]
                .iter()
                .map(|f| (std::f32::consts::TAU * f * t + seed).sin())
                .sum::<f32>();
            0.2 * tones + if i % 4_410 == 0 { 0.5 } else { 0.0 }
        })
        .collect()
}

fn about(input: Input, stems: &[&str]) -> About {
    About {
        name: "test".into(),
        file: "model.onnx".into(),
        stems: stems.iter().map(|s| s.to_string()).collect(),
        rest: None,
        rate: 44_100,
        input,
        segment: 0,
        overlap: 2,
        n_fft: 512,
        hop: 128,
        bins: None,
        gpu: false,
        licence: String::new(),
        source: String::new(),
    }
}

fn worst(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn the_stft_and_its_inverse_give_the_sound_back() {
    let x = sound(10_001, 0.3);
    let stft = stft::Stft::new(2048, 512);
    let spec = stft.forward(&x);
    assert_eq!((spec.len(), spec[0].len()), (10_001 / 512 + 1, 1025));
    let y = stft.inverse(&spec, x.len());
    assert!(worst(&x, &y) < 1e-4, "{}", worst(&x, &y));
    // A frame of 4 s at 512 a hop: the 345 BS-RoFormer's ONNX export is traced at.
    assert_eq!(stft.frames(176_400), 345);
}

#[test]
fn a_model_that_keeps_what_its_given_gives_the_sound_back_whichever_way_it_takes_it() {
    let mix = [sound(20_000, 0.0), sound(20_000, 1.0)];
    let stop = AtomicBool::new(false);
    for input in [Input::Wave, Input::Spec, Input::SpecSplit] {
        let mut a = about(input, &["all"]);
        a.rest = Some("rest".into());
        // 31 hops a run.
        let segment = 128 * 31;
        let mut runs = 0;
        let mut last = 0.0;
        let stems = demix(
            &a,
            segment,
            &mix,
            |t| {
                runs += 1;
                Ok(t)
            },
            |p| last = p,
            &stop,
        )
        .unwrap();
        assert_eq!(stems.len(), 2, "{input:?}");
        assert_eq!(stems[0].0, "all");
        for (c, m) in mix.iter().enumerate() {
            let e = worst(&stems[0].1[c], m);
            assert!(e < 1e-3, "{input:?} channel {c}: {e}");
            // What's left: nothing.
            let rest = stems[1].1[c].iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(rest < 1e-3, "{input:?} rest {rest}");
        }
        // Half overlapping: about two runs a segment's length, and all of it done.
        assert!(runs >= 20_000 / segment * 2, "{input:?}: {runs} runs");
        assert_eq!(last, 1.0);
    }
}

#[test]
fn each_stem_comes_from_its_place_in_the_outputs() {
    let mix = [sound(9_000, 0.5), sound(9_000, 2.0)];
    let stop = AtomicBool::new(false);
    for input in [Input::Wave, Input::Spec, Input::SpecSplit] {
        let a = about(input, &["quiet", "loud"]);
        // Two stems: a quarter of what it's given, then three quarters, each output laid out
        // as the model's would be (stems first).
        let stems = demix(
            &a,
            128 * 15,
            &mix,
            |t| {
                Ok(t.into_iter()
                    .map(|(mut shape, v)| {
                        shape.insert(1, 2);
                        let q: Vec<f32> = v.iter().map(|x| x * 0.25).collect();
                        let l: Vec<f32> = v.iter().map(|x| x * 0.75).collect();
                        (shape, [q, l].concat())
                    })
                    .collect())
            },
            |_| {},
            &stop,
        )
        .unwrap();
        for (c, m) in mix.iter().enumerate() {
            let quiet: Vec<f32> = m.iter().map(|x| x * 0.25).collect();
            let loud: Vec<f32> = m.iter().map(|x| x * 0.75).collect();
            assert!(worst(&stems[0].1[c], &quiet) < 1e-3, "{input:?}");
            assert!(worst(&stems[1].1[c], &loud) < 1e-3, "{input:?}");
        }
    }
}

#[test]
fn a_spectrogram_model_reading_its_lower_bins_leaves_the_highs_silent() {
    let mix = [sound(8_000, 0.0), sound(8_000, 0.0)];
    let mut a = about(Input::Spec, &["low"]);
    // 64 of 257 bins: up to about 5.5 kHz at 44.1 kHz.
    a.bins = Some(64);
    let mut shape = Vec::new();
    let stems = demix(
        &a,
        128 * 15,
        &mix,
        |t| {
            shape = t[0].0.clone();
            Ok(t)
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(shape, [1, 4, 64, 16]);
    // The 9 kHz tone and the clicks' highs are gone; the 110 Hz one is there.
    let spec = stft::Stft::new(512, 128).forward(&stems[0].1[0]);
    let energy = |f: usize| spec.iter().map(|fr| fr[f].norm()).sum::<f32>();
    let bin = |hz: f32| (hz / 44_100.0 * 512.0).round() as usize;
    assert!(energy(bin(9_000.0)) < energy(bin(110.0)) * 0.01);
}

#[test]
fn a_stop_stops_it_and_a_model_giving_too_little_says_so() {
    let mix = [vec![0.1; 6_000], vec![0.1; 6_000]];
    let a = about(Input::Wave, &["a", "b"]);
    let stopped = demix(&a, 1_000, &mix, Ok, |_| {}, &AtomicBool::new(true));
    assert_eq!(stopped.unwrap_err(), "Stopped");
    // Asked for two stems, given one.
    let e = demix(&a, 1_000, &mix, Ok, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(e.contains("2 stems"), "{e}");
}

#[test]
fn models_are_folders_with_a_model_json_and_their_onnx_file() {
    let dir = std::env::temp_dir().join(format!("pina-models-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let make = |name: &str, json: &str, onnx: bool| {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("model.json"), json).unwrap();
        if onnx {
            std::fs::write(d.join("model.onnx"), b"").unwrap();
        }
    };
    make(
        "roformer",
        r#"{"name": "BS-RoFormer SW", "stems": ["bass", "drums", "other", "vocals", "guitar", "piano"],
            "input": "spec_split", "n_fft": 2048, "hop": 512, "licence": "unknown"}"#,
        true,
    );
    make(
        "demucs",
        r#"{"name": "HTDemucs drums", "stems": ["drums", "bass", "other", "vocals"], "input": "wave"}"#,
        true,
    );
    // No hop for a spectrogram; no ONNX file.
    make(
        "broken",
        r#"{"name": "x", "stems": ["v"], "input": "spec", "n_fft": 2048}"#,
        true,
    );
    make(
        "empty",
        r#"{"name": "y", "stems": ["v"], "input": "wave"}"#,
        false,
    );
    let (found, wrong) = models(&dir);
    let names: Vec<&str> = found.iter().map(|m| m.about.name.as_str()).collect();
    assert_eq!(names, ["BS-RoFormer SW", "HTDemucs drums"]);
    assert_eq!(found[0].about.overlap, 2);
    assert_eq!(found[1].about.rate, 44_100);
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong.iter().any(|e| e.contains("n_fft and hop")),
        "{wrong:?}"
    );
    assert_eq!(
        find(&dir, Some("htdemucs")).unwrap().about.name,
        "HTDemucs drums"
    );
    assert_eq!(find(&dir, None).unwrap().about.name, "BS-RoFormer SW");
    let e = find(&dir, Some("mdx")).unwrap_err();
    assert!(e.contains("BS-RoFormer SW, HTDemucs drums"), "{e}");
    // One copied straight into a Models folder, its files not in a folder of their own, counts.
    let loose = dir.join("loose");
    std::fs::create_dir_all(&loose).unwrap();
    let json = r#"{"name": "Loose", "stems": ["v"], "input": "wave"}"#;
    std::fs::write(loose.join("model.json"), json).unwrap();
    std::fs::write(loose.join("model.onnx"), b"").unwrap();
    assert_eq!(find(&loose, None).unwrap().about.name, "Loose");
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        find(&dir, None)
            .unwrap_err()
            .contains("No separation model")
    );
}

#[test]
fn the_known_models_read_and_each_is_its_own() {
    use catalog::KNOWN;
    for k in KNOWN {
        let a = k.about();
        assert!(!a.stems.is_empty(), "{}", k.id);
        assert!(
            a.input == Input::Wave || (a.n_fft > 0 && a.hop > 0),
            "{}",
            k.id
        );
        assert_eq!(catalog::Known::sized(k.bytes).unwrap().id, k.id);
    }
    let mut ids: Vec<&str> = KNOWN.iter().map(|k| k.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), KNOWN.len());
}

#[test]
fn a_model_comes_down_whole_into_a_folder_of_its_own_or_not_at_all() {
    let dir = std::env::temp_dir().join(format!("pina-download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.onnx");
    std::fs::write(&source, vec![7u8; 10_000]).unwrap();
    let url: &'static str = Box::leak(format!("file://{}", source.display()).into_boxed_str());
    let known = |bytes| catalog::Known {
        id: "test",
        json: r#"{"name": "Test", "stems": ["all"], "input": "wave"}"#,
        url,
        bytes,
        heard: "",
        licence: "",
        open: true,
        speed: 1.0,
    };
    let models = dir.join("Models");
    let mut seen = 0;
    let folder = catalog::download(
        &known(10_000),
        &models,
        |b| seen = b,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(seen, 10_000);
    assert_eq!(Model::read(&folder).unwrap().about.name, "Test");
    // Not as long as it should be: nothing's left.
    std::fs::remove_dir_all(&models).unwrap();
    let e = catalog::download(&known(10_001), &models, |_| {}, &AtomicBool::new(false));
    assert!(e.unwrap_err().contains("not its 10001"));
    assert!(!models.join("test").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_added_is_known_by_its_size_or_given_a_model_json_to_finish() {
    let dir = std::env::temp_dir().join(format!("pina-add-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("My Splitter.onnx");
    std::fs::write(&file, b"not a model").unwrap();
    let models = dir.join("Models");
    let (folder, known) = catalog::add(&file, &models).unwrap();
    assert!(!known);
    assert_eq!(folder, models.join("My Splitter"));
    assert!(folder.join("model.onnx").is_file());
    // Its stems are for the person to name: until then it isn't one.
    let e = Model::read(&folder).unwrap_err();
    assert!(e.contains("no stems"), "{e}");
    // Added again: a folder of its own.
    let (again, _) = catalog::add(&file, &models).unwrap();
    assert_eq!(again, models.join("My Splitter 2"));
    std::fs::remove_dir_all(&dir).unwrap();
}
