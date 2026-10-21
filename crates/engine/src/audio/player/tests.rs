use super::*;
use crate::song::project::Track;

/// With a real instrument, which CI doesn't have:
/// `PINA_TEST_VST3="/path/Some Synth.vst3" cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn plugin_settings_are_saved_with_the_project_and_loaded_back() {
    let path = std::env::var("PINA_TEST_VST3").expect("set PINA_TEST_VST3");
    let class = crate::plugin::scan::inspect(Path::new(&path))
        .unwrap()
        .remove(0)
        .class;
    let dir = std::env::temp_dir().join(format!("pina-test-states-{}", std::process::id()));
    let plugin = PluginRef {
        path: path.into(),
        class,
        state: None,
        ..Default::default()
    };
    let mut project = Project::new(48_000);
    project.add_track(
        0,
        Track {
            name: "Synth".into(),
            instrument: Some(Instrument::Plugin(plugin)),
            ..Default::default()
        },
    );
    let mut player = Player::silent(&project, &dir);
    player.sync(&project).unwrap();
    player.save_states(&mut project).unwrap();
    let Some(Instrument::Plugin(r)) = &project.tracks[0].instrument else {
        unreachable!()
    };
    let file = r.state.clone().expect("a state file");
    assert!(dir.join(&file).exists());

    // A parameter the project sets is applied; when it stops setting it (an undo), it goes
    // back to what it was.
    let id = project.tracks[0].id;
    let param = player.params(id, Which::Instrument)[0].clone();
    let set = |project: &mut Project, v: Option<f64>| {
        let r = project.tracks[0].plugin_mut(Which::Instrument).unwrap();
        match v {
            Some(v) => drop(r.params.insert(param.id, v)),
            None => drop(r.params.remove(&param.id)),
        }
    };
    let other = if param.value > 0.5 { 0.1 } else { 0.9 };
    set(&mut project, Some(other));
    player.sync(&project).unwrap();
    assert!((player.param_value(id, Which::Instrument, param.id) - other).abs() < 0.02);
    set(&mut project, None);
    player.sync(&project).unwrap();
    let back = player.param_value(id, Which::Instrument, param.id);
    assert!((back - param.value).abs() < 0.02, "{back} {}", param.value);
    // A bounce of the saved project loads it.
    project.save(&dir).unwrap();
    let loaded = Project::load(&dir).unwrap();
    assert!(Engine::load(&loaded, &dir, 48_000.0).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}
