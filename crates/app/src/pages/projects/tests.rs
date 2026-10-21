use super::*;

#[test]
fn songs_are_made_listed_copied_renamed_and_trashed() {
    let root = std::env::temp_dir().join(format!("pina-test-songs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let a = create(
        &root,
        "Night",
        92.0,
        Key::parse("A minor"),
        [4, 4],
        Template::Beat,
    )
    .unwrap();
    let b = create(&root, "Night", 120.0, None, [3, 4], Template::Empty).unwrap();
    assert_eq!(b.file_name().unwrap(), "Night 2");
    assert!(create(&root, "Bad", 5.0, None, [4, 4], Template::Empty).is_err());
    let p = Project::load(&a).unwrap();
    assert_eq!(
        (p.tempo, p.key.as_deref(), p.tracks.len()),
        (92.0, Some("A minor"), 5)
    );
    let songs = list(&root, &[]);
    assert_eq!(songs.len(), 2);
    let night = songs.iter().find(|s| s.dir == a).unwrap();
    assert_eq!((night.tracks, night.lanes.len()), (5, 5));
    let copy = duplicate(&a).unwrap();
    assert_eq!(copy.file_name().unwrap(), "Night copy");
    assert!(Project::load(&copy).is_ok());
    let renamed = rename(&copy, "Morning").unwrap();
    assert!(renamed.join(FILE).is_file() && !copy.exists());
    assert!(rename(&renamed, "Night").is_err());
    assert!(rename(&renamed, "a/b").is_err());
    assert_eq!(Template::from_name("lofi"), Some(Template::LoFi));
    std::fs::remove_dir_all(root).unwrap();
}
