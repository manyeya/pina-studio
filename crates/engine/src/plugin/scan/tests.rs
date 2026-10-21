use super::*;

#[test]
fn finds_bundles_in_vendor_folders_and_survives_a_bad_scanner() {
    let dir = std::env::temp_dir().join(format!("pina-test-scan-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Vendor/Thing.vst3/Contents")).unwrap();
    std::fs::create_dir_all(dir.join("Other.vst3")).unwrap();
    std::fs::create_dir_all(dir.join("not a plugin")).unwrap();
    let found = bundles(std::slice::from_ref(&dir));
    assert_eq!(
        found,
        [dir.join("Other.vst3"), dir.join("Vendor/Thing.vst3")]
    );

    // `false` stands in for a scanner that fails: no plugins, but the scan finishes and the
    // failure is cached.
    let cache = dir.join("cache.json");
    assert!(scan(Path::new("false"), &found, &cache).is_empty());
    let cached: HashMap<PathBuf, Cached> =
        serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
    assert_eq!(cached.len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}
