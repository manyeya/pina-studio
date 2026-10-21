use super::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pina-installer-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A Pina Studio.app with only what the installer reads: its version and its program.
fn fake_app(at: &Path, v: &str) {
    fs::create_dir_all(at.join("Contents/MacOS")).unwrap();
    fs::write(
        at.join("Contents/Info.plist"),
        format!(
            "<plist><dict><key>CFBundleShortVersionString</key><string>{v}</string></dict></plist>"
        ),
    )
    .unwrap();
    fs::write(at.join("Contents/MacOS/pina"), v).unwrap();
}

#[test]
fn versions_compare_number_by_number() {
    assert_eq!(over(None, "0.2.0"), Over::Nothing);
    assert_eq!(
        over(Some("0.9.7".into()), "0.10.2"),
        Over::Older("0.9.7".into())
    );
    assert_eq!(over(Some("0.2.0".into()), "0.2.0"), Over::Same);
    assert_eq!(
        over(Some("0.3.0".into()), "0.2.9"),
        Over::Newer("0.3.0".into())
    );
    assert_eq!(
        over(Some("0.2.0-dev".into()), "0.2.0"),
        Over::Older("0.2.0-dev".into())
    );
}

#[test]
fn installing_puts_it_there_whole_and_an_update_replaces_it() {
    let root = scratch("install");
    let payload = root.join("payload").join(APP);
    fake_app(&payload, "0.2.0");
    let plan = Plan {
        payload: payload.clone(),
        examples: None,
        apps: root.join("Applications"),
        bin: root.join("bin"),
        songs: root.join("songs"),
    };
    assert_eq!(version(&plan.app()), None);
    let app = copy_app(&plan).unwrap();
    assert_eq!(version(&app).as_deref(), Some("0.2.0"));
    // A newer one over it: replaced, nothing left beside it.
    fake_app(&payload, "0.3.0");
    copy_app(&plan).unwrap();
    assert_eq!(version(&app).as_deref(), Some("0.3.0"));
    let left: Vec<_> = fs::read_dir(&plan.apps)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left, [APP]);
    // The command: linked to it; again, the same; someone else's, left alone.
    let link = command_line(&app, &plan.bin).unwrap();
    assert_eq!(
        fs::read_link(&link).unwrap(),
        app.join("Contents/MacOS/pina")
    );
    assert!(command_line(&app, &plan.bin).is_ok());
    let other = root.join("other-bin");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("pina"), "#!/bin/sh").unwrap();
    assert!(command_line(&app, &other).is_err());
    // tempo, as it was called, beside it: its app to the Trash, its command gone.
    let tempo = plan.apps.join("tempo.app");
    fake_app(&tempo, "0.1.0");
    let plist = tempo.join("Contents/Info.plist");
    fs::write(
        &plist,
        fs::read_to_string(&plist).unwrap() + "dev.tempo.daw",
    )
    .unwrap();
    std::os::unix::fs::symlink(tempo.join("Contents/MacOS/tempo"), plan.bin.join("tempo")).unwrap();
    copy_app(&plan).unwrap();
    command_line(&app, &plan.bin).unwrap();
    assert!(!tempo.exists() && fs::symlink_metadata(plan.bin.join("tempo")).is_err());
    // Out again: the app and its command; songs stay.
    fs::create_dir_all(&plan.songs).unwrap();
    uninstall(&plan, false).unwrap();
    assert!(!app.exists() && fs::symlink_metadata(&link).is_err());
    assert!(plan.songs.exists() && other.join("pina").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn example_songs_go_in_beside_yours_never_over_them() {
    let root = scratch("examples");
    let from = root.join("Examples");
    for name in ["Midnight Drive", "Not a song"] {
        fs::create_dir_all(from.join(name)).unwrap();
    }
    fs::write(from.join("Midnight Drive/project.json"), "{}").unwrap();
    let songs = root.join("songs");
    assert_eq!(examples(&from, &songs).unwrap(), 1);
    assert!(songs.join("Midnight Drive/project.json").is_file());
    assert!(!songs.join("Not a song").exists());
    // Yours, changed: left as it is.
    fs::write(songs.join("Midnight Drive/project.json"), "mine").unwrap();
    assert_eq!(examples(&from, &songs).unwrap(), 0);
    assert_eq!(
        fs::read_to_string(songs.join("Midnight Drive/project.json")).unwrap(),
        "mine"
    );
    fs::remove_dir_all(root).unwrap();
}
