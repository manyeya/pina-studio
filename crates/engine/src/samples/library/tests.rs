use super::*;

fn tone(path: &Path, seconds: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(44_100.0 * seconds) as usize {
        let s = (i as f32 * 0.05).sin() * 8000.0;
        w.write_sample(s as i16).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn a_scan_analyses_whats_new_and_forgets_whats_gone() {
    let base = std::env::temp_dir().join(format!("pina-test-library-{}", std::process::id()));
    let pack = base.join("Pack/Drums");
    std::fs::create_dir_all(&pack).unwrap();
    tone(&pack.join("Kick_01.wav"), 0.3);
    tone(&pack.join("Snare_01.wav"), 0.3);
    std::fs::write(pack.join("._Kick_01.wav"), b"resource fork").unwrap();
    std::fs::write(pack.join("notes.txt"), b"not audio").unwrap();
    let mut library = Library {
        folders: vec![base.clone()],
        samples: Vec::new(),
    };
    // SAFETY: tests touching PINA_LIBRARY run in this one test.
    unsafe { std::env::set_var("PINA_LIBRARY", base.join("config")) };
    let report = library.scan(&|_, _| {});
    assert_eq!((report.samples, report.analysed, report.removed), (2, 2, 0));
    let kick = library.get(&pack.join("Kick_01.wav")).unwrap();
    assert_eq!(
        (kick.name.as_str(), kick.folder.as_str()),
        ("Kick_01", "Pack/Drums")
    );
    // Nothing changed: nothing analysed again. One gone: forgotten.
    assert_eq!(library.scan(&|_, _| {}).analysed, 0);
    std::fs::remove_file(pack.join("Snare_01.wav")).unwrap();
    let report = library.scan(&|_, _| {});
    assert_eq!((report.samples, report.removed), (1, 1));
    // Saved, it loads back.
    let again = Library::load();
    assert_eq!(again.samples.len(), 1);
    assert_eq!(again.folders, std::slice::from_ref(&base));
    std::fs::remove_dir_all(base).unwrap();
}
