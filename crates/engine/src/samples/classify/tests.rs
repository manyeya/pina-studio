use super::*;

fn top(path: &str) -> Option<Category> {
    hints(path)
        .categories
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|c| c.0)
}

#[test]
fn names_say_what_they_are() {
    assert_eq!(
        top("/x/90s hip hop/drum_hits/kick_03.wav"),
        Some(Category::Kick)
    );
    assert_eq!(
        top("/x/trap/drum_hits/closedhat_01.wav"),
        Some(Category::HatClosed)
    );
    assert_eq!(
        top("/x/808/hits/clap [cp]/e808_cp-01.wav"),
        Some(Category::Clap)
    );
    assert_eq!(
        top("/x/808/hits/bass drum [bd]/e808_bd[long]-01.wav"),
        Some(Category::Kick)
    );
    assert_eq!(top("/x/kit/tr 808 kick 01.wav"), Some(Category::Kick));
    assert_eq!(top("/x/hits/sub_01.wav"), Some(Category::Bass808));
    assert_eq!(
        top("/x/loops/horn_stabs_01(95bpm).wav"),
        Some(Category::Brass)
    );
    assert_eq!(
        top("/x/beat_01/beat_01_bassstabs.wav"),
        Some(Category::Bass)
    );
    assert_eq!(top("/x/stuff/whatever.wav"), None);
    let h = hints("/x/loops/keys_01(95bpm).wav");
    assert_eq!(h.kind, Some((Kind::Loop, 1.0)));
    assert_eq!(h.bpm, Some((95.0, false)));
    assert_eq!(
        hints("/x/drum_hits/snare_02.wav").kind,
        Some((Kind::OneShot, 0.45))
    );
}
