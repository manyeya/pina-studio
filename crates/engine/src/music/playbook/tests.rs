use super::*;

#[test]
fn every_playbook_is_whole() {
    let (all, problems) = load();
    assert!(problems.is_empty());
    assert_eq!(all.len(), BUILT_IN.len());
    for p in &all {
        check(p).unwrap();
        let roles: Vec<&str> = p.parts.iter().map(|x| x.role.as_str()).collect();
        // Every section's parts are among the genre's parts.
        for s in &p.sections {
            for part in &s.parts {
                assert!(
                    roles.contains(&part.as_str()),
                    "{}: {} in {}",
                    p.id,
                    part,
                    s.name
                );
            }
            assert!(s.bars % 4 == 0);
        }
        // The harmony words find progressions of its kind.
        let key = crate::music::Key {
            root: 9,
            minor: p.minor,
        };
        assert_eq!(
            crate::music::theory::suggest(&key, &p.harmony, 0).minor,
            p.minor
        );
    }
    assert_eq!(find("Lo-Fi").unwrap().id, "lofi");
    assert_eq!(find("UK drill").unwrap().id, "drill");
    assert!(find("polka").is_none());
}

#[test]
fn playbooks_know_where_they_come_from_and_save_whole() {
    let trap = builtin("trap").unwrap();
    assert_eq!(origin(&trap), Origin::Pina);
    let mut edited = trap.clone();
    edited.swing = 56.0;
    assert_eq!(origin(&edited), Origin::Edited);
    let mine = Playbook {
        id: "gqom".into(),
        ..trap.clone()
    };
    assert_eq!(origin(&mine), Origin::Yours);
    let taken = vec!["trap".to_string(), "dark_trap".to_string()];
    assert_eq!(new_id("Dark Trap!", &taken), "dark_trap_2");
    assert_eq!(new_id("Gqom", &taken), "gqom");
    let dir = std::env::temp_dir().join(format!("pina-playbook-save-{}", std::process::id()));
    let file = save_in(&dir, &edited).unwrap();
    let back: Playbook = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(back, edited);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_persons_playbooks_replace_and_add() {
    let dir = std::env::temp_dir().join(format!("pina-playbooks-{}", std::process::id()));
    let (all, _) = load_from(Some(&dir));
    // The folder is made, with Pina Studio's in it to edit.
    assert!(dir.join("trap.json").exists() && dir.join("README.txt").exists());
    assert_eq!(all.len(), BUILT_IN.len());
    let file = dir.join("trap.json");
    let mut trap: Playbook =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    trap.tempo = (120.0, 130.0);
    std::fs::write(&file, serde_json::to_string(&trap).unwrap()).unwrap();
    let mut gqom = Playbook {
        id: "gqom".into(),
        name: "Gqom".into(),
        ..trap.clone()
    };
    std::fs::write(dir.join("gqom.json"), serde_json::to_string(&gqom).unwrap()).unwrap();
    std::fs::write(dir.join("broken.json"), "{ \"id\": ").unwrap();
    (gqom.id, gqom.groove) = ("bad".into(), "polka".into());
    std::fs::write(dir.join("bad.json"), serde_json::to_string(&gqom).unwrap()).unwrap();
    let (all, problems) = load_from(Some(&dir));
    assert_eq!(
        all.iter().find(|p| p.id == "trap").unwrap().tempo,
        (120.0, 130.0)
    );
    assert_eq!(find_in(all, "gqom").unwrap().name, "Gqom");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("polka")));
    std::fs::remove_dir_all(&dir).unwrap();
}
