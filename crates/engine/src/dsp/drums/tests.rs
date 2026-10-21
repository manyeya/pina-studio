use super::*;

#[test]
fn each_hit_sounds_and_ends() {
    for &(pitch, _) in SOUNDS {
        let mut d = Drums::new(48_000.0);
        let n = 48_000 * 4;
        let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
        let hit = Note {
            offset: 10,
            pitch,
            velocity: 1.0,
        };
        d.process([&mut l, &mut r], n, &[hit]);
        let loudest = l.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(loudest > 0.05, "{} is silent", name(pitch));
        assert_eq!(l[..10], [0.0; 10]);
        assert_eq!(*l.last().unwrap(), 0.0, "{} rings forever", name(pitch));
    }
}
