use super::*;

#[test]
fn what_tempo_kept_moves_to_pina_studio_once_and_its_old_paths_still_lead_there() {
    let home = std::env::temp_dir().join(format!("pina-test-paths-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let song = home.join("Music/tempo/Night/project.json");
    fs::create_dir_all(song.parent().unwrap()).unwrap();
    fs::write(&song, "{}").unwrap();
    // The installer's example song, in the new place first.
    fs::create_dir_all(home.join("Music/Pina Studio/Midnight Drive")).unwrap();
    let support = home.join("Library/Application Support/tempo");
    fs::create_dir_all(&support).unwrap();
    let old = home.join("Music/tempo").to_string_lossy().into_owned();
    fs::write(
        support.join("recent.json"),
        format!("[\"{old}/Night\", \"{old}\", \"/elsewhere/tempo\"]"),
    )
    .unwrap();
    let moved = migrate_in(&home);
    assert_eq!(moved.len(), 2);
    let new = home.join("Music/Pina Studio");
    assert!(new.join("Night/project.json").is_file() && new.join("Midnight Drive").is_dir());
    // The old place, a link to the new: its paths still work.
    assert!(
        song.is_file()
            && fs::symlink_metadata(home.join("Music/tempo"))
                .unwrap()
                .is_symlink()
    );
    let recent =
        fs::read_to_string(home.join("Library/Application Support/Pina Studio/recent.json"))
            .unwrap();
    let new = new.to_string_lossy();
    assert_eq!(
        recent,
        format!("[\"{new}/Night\", \"{new}\", \"/elsewhere/tempo\"]")
    );
    // Again: nothing more to do.
    assert!(migrate_in(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
}
