use super::*;

fn p(id: u32, name: &str) -> PluginParam {
    PluginParam {
        id,
        name: name.into(),
        value: 0.5,
        ..Default::default()
    }
}

#[test]
fn smart_controls_find_the_filter_and_the_amp() {
    let params = [
        p(1, "A Osc 1 Pitch"),
        p(2, "A Filter 1 Cutoff"),
        p(3, "A Filter 2 Cutoff"),
        p(4, "A Filter 1 Resonance"),
        p(5, "A Filter EG Attack"),
        p(6, "A Amp EG Attack"),
        p(7, "A Amp EG Release"),
    ];
    let picked: Vec<u32> = smart_params(&params).iter().map(|c| c.0).collect();
    assert_eq!(picked, [2, 4, 6, 7]);
    // Without names to go on, the first four.
    let plain: Vec<u32> = smart_params(&[p(9, "Gain"), p(8, "Mix")])
        .iter()
        .map(|c| c.0)
        .collect();
    assert_eq!(plain, [9, 8]);
    let mut p = Project::new(48_000);
    assert_eq!(position(&p, PPQ * 4 * 16 + PPQ), "17 2 1 1");
    assert_eq!(length(&p, PPQ * 4 * 4), "4 0 0 0");
    p.time_signature = [6, 8];
    assert_eq!(position(&p, PPQ * 3 + PPQ / 2), "2 2 1 1");
}
