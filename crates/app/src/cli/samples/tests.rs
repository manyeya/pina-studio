use super::*;

#[test]
fn the_palette_says_what_each_sound_is_its_part_and_whether_its_in() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo");
    let dir = std::env::temp_dir().join(format!("pina-test-palette-tool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo.join(f), dir.join(f)).unwrap();
    }
    let drums = dir.join("Audio Files/drums.wav");
    let other = dir.join("loop.wav");
    std::fs::copy(&drums, &other).unwrap();
    // Built as JSON, not text: a Windows path's backslashes would need escaping.
    let set = serde_json::json!({
        "op": "set_palette",
        "sounds": [{ "path": drums, "role": "drum loop" }, { "path": other }],
        "devices": [
            { "name": "Room", "plugin": { "class": "Room", "path": "/p/Room.vst3" }, "role": "vocal" },
            { "name": "Drum Kit", "instrument": true, "own": "drums" },
        ],
        "only": true,
    });
    crate::cli::edit_with(&dir, serde_json::from_value(set).unwrap()).unwrap();
    let v = palette(&dir).unwrap();
    assert_eq!(v["only"], true);
    assert!(v["how"].as_str().unwrap().contains("no other"));
    let sounds = v["sounds"].as_array().unwrap();
    assert_eq!(sounds[0]["role"], "drum loop");
    assert_eq!(sounds[0]["in_song"], true); // the demo's drums play it
    assert_eq!(sounds[1]["in_song"], false);
    assert!(sounds[1]["role"].is_null() && sounds[1]["kind"].is_string());
    // Instruments and effects: how each goes in, and whether it has (the demo has drums).
    let (inst, fx) = (&v["instruments"][0], &v["effects"][0]);
    assert_eq!(inst["use"], "Pina Studio's own: \"instrument\": \"drums\"");
    assert_eq!(fx["role"], "vocal");
    assert_eq!(fx["in_song"], false);
    assert!(fx["use"].as_str().unwrap().contains(r#""class":"Room""#));
    let _ = std::fs::remove_dir_all(&dir);
}
