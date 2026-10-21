use super::*;

#[test]
fn undo_and_redo() {
    let mut s = Session::open(Path::new("/nonexistent")).unwrap();
    s.checkpoint();
    s.project.tempo = 90.0;
    s.checkpoint();
    s.project.tempo = 100.0;
    assert!(s.undo());
    assert_eq!(s.project.tempo, 90.0);
    assert!(s.undo());
    assert_eq!(s.project.tempo, 120.0);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.project.tempo, 90.0);
    s.checkpoint(); // a new edit drops what could be redone
    assert!(!s.redo());

    let mut s = Session::open(Path::new("/nonexistent")).unwrap();
    s.checkpoint();
    s.cancel(); // an edit that changed nothing
    assert!(!s.edited && !s.undo());
}

#[test]
fn unsaved_edits_are_kept_and_restored() {
    let dir = std::env::temp_dir().join(format!("pina-test-keep-{}", std::process::id()));
    let mut s = Session::open(&dir).unwrap();
    s.save().unwrap();
    s.checkpoint();
    s.project.tempo = 95.0;
    assert!(!s.kept && s.kept_at().is_none());
    s.keep(&s.project.clone()).unwrap();
    assert!(s.kept);

    // Pina Studio stopped without saving: the project opens as saved, with the edits on offer.
    let mut s = Session::open(&dir).unwrap();
    assert_eq!(s.project.tempo, 120.0);
    assert!(s.kept_at().is_some());
    s.restore().unwrap();
    assert!(s.edited && s.project.tempo == 95.0);
    assert!(s.undo() && s.project.tempo == 120.0);

    s.forget_kept();
    assert!(s.kept_at().is_none());
    assert!(!dir.join(format!("{FILE}.part")).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_new_project_is_on_disk_once_saved() {
    let dir = std::env::temp_dir().join(format!("pina-test-session-{}", std::process::id()));
    let mut s = Session::open(&dir).unwrap();
    assert!(!s.on_disk && !s.edited);
    s.save().unwrap();
    assert!(s.on_disk);
    assert!(Session::open(&dir).unwrap().on_disk);
    std::fs::remove_dir_all(dir).unwrap();
}
