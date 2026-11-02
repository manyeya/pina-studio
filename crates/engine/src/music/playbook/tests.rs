use super::*;

#[test]
fn every_playbook_is_whole() {
    let (all, problems) = load();
    assert!(problems.is_empty());
    assert_eq!(all.len(), 3);
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
    assert_eq!(find("Lo-fi hip-hop").unwrap().id, "lofi");
    assert!(find("polka").is_none());
}

#[test]
fn none_are_made_for_the_person_and_new_ones_save_whole() {
    // An empty folder, or none: no playbooks, and nothing written there.
    let dir = std::env::temp_dir().join(format!("pina-playbook-save-{}", std::process::id()));
    assert!(load_from(Some(&dir)).0.is_empty() && !dir.exists());
    let trap = find("trap").unwrap();
    let taken = vec!["trap".to_string(), "dark_trap".to_string()];
    assert_eq!(new_id("Dark Trap!", &taken), "dark_trap_2");
    assert_eq!(new_id("Gqom", &taken), "gqom");
    let file = save_in(&dir, &trap).unwrap();
    let back: Playbook = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(back, trap);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_persons_playbooks_are_read_and_bad_ones_said() {
    let dir = std::env::temp_dir().join(format!("pina-playbooks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let trap = find("trap").unwrap();
    let mut gqom = Playbook {
        id: "gqom".into(),
        name: "Gqom".into(),
        ..trap
    };
    std::fs::write(dir.join("gqom.json"), serde_json::to_string(&gqom).unwrap()).unwrap();
    std::fs::write(dir.join("broken.json"), "{ \"id\": ").unwrap();
    (gqom.id, gqom.groove) = ("bad".into(), "polka".into());
    std::fs::write(dir.join("bad.json"), serde_json::to_string(&gqom).unwrap()).unwrap();
    let (all, problems) = load_from(Some(&dir));
    assert_eq!(all.len(), 1);
    assert_eq!(find_in(all, "gqom").unwrap().name, "Gqom");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("polka")));
    std::fs::remove_dir_all(&dir).unwrap();
}
