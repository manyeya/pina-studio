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

#[test]
fn songs_are_filtered_and_sorted_as_asked() {
    let ago = |days: u64| SystemTime::now() - std::time::Duration::from_secs(days * 86_400 + 60);
    let song =
        |name: &str, tempo: f64, key: Option<&str>, signature: [u64; 2], seconds: f64, days| Song {
            dir: PathBuf::from(format!("/songs/{name}")),
            name: name.into(),
            modified: ago(days),
            tempo,
            key: key.map(String::from),
            signature,
            tracks: 3,
            seconds,
            lanes: Vec::new(),
        };
    let songs = [
        song("Night", 92.0, Some("A minor"), [4, 4], 200.0, 0),
        song("Rain", 74.0, Some("D minor"), [6, 8], 90.0, 3),
        song("Warehouse", 128.0, Some("A minor"), [4, 4], 360.0, 40),
        song("Night Two", 120.0, None, [3, 4], 10.0, 10),
    ];
    let names = |v: Vec<Song>| v.into_iter().map(|s| s.name).collect::<Vec<_>>();
    let mut shelf = Shelving::default();
    // Newest first, and the search narrows by name.
    assert_eq!(
        names(shelf.shown(&songs, "")),
        ["Night", "Rain", "Night Two", "Warehouse"]
    );
    assert_eq!(names(shelf.shown(&songs, "night")), ["Night", "Night Two"]);
    // Each filter, then all of them off again.
    shelf.change(Shelf::Pace(Pace::Mid));
    assert_eq!(names(shelf.shown(&songs, "")), ["Night", "Night Two"]);
    shelf.change(Shelf::Key(Some("A minor".into())));
    assert_eq!(names(shelf.shown(&songs, "")), ["Night"]);
    assert!(shelf.filtered());
    shelf.change(Shelf::Clear);
    shelf.change(Shelf::Time(Some([6, 8])));
    assert_eq!(names(shelf.shown(&songs, "")), ["Rain"]);
    shelf.change(Shelf::Clear);
    assert!(!shelf.filtered());
    // The other orders.
    shelf.change(Shelf::Sort(Sort::Name));
    assert_eq!(
        names(shelf.shown(&songs, "")),
        ["Night", "Night Two", "Rain", "Warehouse"]
    );
    shelf.change(Shelf::Sort(Sort::Tempo));
    assert_eq!(
        names(shelf.shown(&songs, "")),
        ["Rain", "Night", "Night Two", "Warehouse"]
    );
    shelf.change(Shelf::Sort(Sort::Length));
    assert_eq!(
        names(shelf.shown(&songs, "")),
        ["Warehouse", "Night", "Rain", "Night Two"]
    );
    // The list's headings.
    let eras: Vec<_> = songs.iter().map(|s| era(s.modified)).collect();
    assert_eq!(eras, ["Today", "This week", "Earlier", "This month"]);
}
