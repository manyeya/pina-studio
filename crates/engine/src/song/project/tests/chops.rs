//! Flipping a chopped sample.

use super::*;

/// A sampler sliced into four chops (notes 36 to 39) and a bar of quarter notes playing them in
/// order.
fn chopped() -> Project {
    let notes: Vec<Note> = (0..4)
        .map(|k| Note {
            start: k * PPQ as i64,
            length: PPQ,
            pitch: 36 + k as u8,
            velocity: 100,
            ..Default::default()
        })
        .collect();
    let mut p = project(vec![midi(0, PPQ * 4, &[])], true);
    if let Content::Midi { notes: n, .. } = &mut p.tracks[0].regions[0].content {
        *n = notes;
    }
    let zones = (0..4)
        .map(|k| Zone {
            file: "loop.wav".into(),
            name: format!("Slice {}", k + 1),
            note: 36 + k,
            start: k as f32 / 4.0,
            end: Some((k + 1) as f32 / 4.0),
            ..Zone::default()
        })
        .collect();
    p.tracks[0].instrument = Some(Instrument::Sampler(Sampler {
        zones,
        voice: SamplerVoice::default(),
    }));
    p
}

fn played(p: &Project) -> Vec<(i64, u64, u8)> {
    let Content::Midi { notes, .. } = &p.tracks[0].regions[0].content else {
        panic!()
    };
    p.chop_order((0, 0))
        .iter()
        .map(|&k| (notes[k].start, notes[k].length, notes[k].pitch))
        .collect()
}

fn pitches(p: &Project) -> Vec<u8> {
    played(p).iter().map(|n| n.2).collect()
}

#[test]
fn chops_swap_take_anothers_place_stutter_and_go_silent() {
    let mut p = chopped();
    p.swap_chops((0, 0), 0, 3);
    assert_eq!(pitches(&p), [39, 37, 38, 36]);
    p.set_chop((0, 0), 1, 36);
    assert_eq!(pitches(&p), [39, 36, 38, 36]);
    // The third, three times in its beat.
    p.stutter_chop((0, 0), 2, 3);
    let q = PPQ as i64;
    let third = q / 3;
    assert_eq!(
        played(&p)[2..5],
        [
            (2 * q, PPQ / 3, 38),
            (2 * q + third, PPQ / 3, 38),
            (2 * q + 2 * third, PPQ / 3, 38)
        ]
    );
    p.silence_chop((0, 0), 0);
    assert_eq!(pitches(&p)[0], 36);
    assert_eq!(played(&p).len(), 5);
}

#[test]
fn a_chop_reversed_plays_a_reversed_copy_of_its_sound_made_once() {
    let mut p = chopped();
    let to = p.reverse_chop((0, 0), 1).unwrap();
    assert_eq!(to, 40, "the next free note");
    let Some(Instrument::Sampler(s)) = &p.tracks[0].instrument else {
        panic!()
    };
    let z = s.zones.iter().find(|z| z.note == 40).unwrap();
    assert!(z.reverse);
    assert_eq!((z.start, z.end), (0.25, Some(0.5)));
    assert_eq!(pitches(&p), [36, 40, 38, 39]);
    // The same chop elsewhere, reversed: the same copy; reversed again: forwards.
    p.set_chop((0, 0), 3, 37);
    assert_eq!(p.reverse_chop((0, 0), 3), Some(40));
    assert_eq!(p.reverse_chop((0, 0), 1), Some(37));
    let Some(Instrument::Sampler(s)) = &p.tracks[0].instrument else {
        panic!()
    };
    assert_eq!(s.zones.len(), 5);
}

#[test]
fn a_flip_keeps_the_one_on_the_downbeat_and_each_seed_is_its_own() {
    let mut p = chopped();
    p.flip_chops((0, 0), 1);
    let first = pitches(&p);
    assert_eq!(first[0], 36, "the one on the one");
    assert!(first.iter().all(|c| (36..40).contains(c)));
    let mut again = chopped();
    again.flip_chops((0, 0), 1);
    assert_eq!(pitches(&again), first, "a seed, the same flip");
    let flips: std::collections::HashSet<Vec<u8>> = (2..20)
        .map(|seed| {
            let mut q = chopped();
            q.flip_chops((0, 0), seed);
            pitches(&q)
        })
        .collect();
    assert!(flips.len() > 3, "seeds make different flips: {flips:?}");
    // Back as it was.
    p.chops_in_order((0, 0), &[36, 37, 38, 39]);
    assert_eq!(pitches(&p), [36, 37, 38, 39]);
}

#[test]
fn the_chops_turn_round_and_go_half_and_double_time() {
    let mut p = chopped();
    p.reverse_chop_order((0, 0));
    assert_eq!(pitches(&p), [39, 38, 37, 36]);
    // Half time: two fit the bar.
    p.time_chops((0, 0), true);
    let q = PPQ as i64;
    assert_eq!(played(&p), [(0, PPQ * 2, 39), (2 * q, PPQ * 2, 38)]);
    // Double time: back to quarters, played twice to fill it.
    p.time_chops((0, 0), false);
    assert_eq!(
        played(&p),
        [
            (0, PPQ, 39),
            (q, PPQ, 38),
            (2 * q, PPQ, 39),
            (3 * q, PPQ, 38)
        ]
    );
}

#[test]
fn a_chop_cut_by_hand_splits_its_notes_so_it_plays_the_same_and_joins_back() {
    let mut p = chopped();
    // The second chop (a quarter to a half) cut at three eighths.
    assert_eq!(p.cut_chop(0, 0.375), Some(38));
    let Some(Instrument::Sampler(s)) = &p.tracks[0].instrument else {
        panic!()
    };
    let cuts: Vec<(u8, f32, f32)> = cuts_of(s).iter().map(|c| (c.1, c.2, c.3)).collect();
    assert_eq!(
        cuts,
        [
            (36, 0.0, 0.25),
            (37, 0.25, 0.375),
            (38, 0.375, 0.5),
            (39, 0.5, 0.75),
            (40, 0.75, 1.0)
        ]
    );
    // Its note in two, the second on the new chop; the chops after it a note up.
    let q = PPQ as i64;
    assert_eq!(
        played(&p),
        [
            (0, PPQ, 36),
            (q, PPQ / 2, 37),
            (q + q / 2, PPQ / 2, 38),
            (2 * q, PPQ, 39),
            (3 * q, PPQ, 40)
        ]
    );
    // Moved: no further than its neighbours.
    p.move_cut(0, 2, 0.45);
    p.move_cut(0, 2, 0.1);
    let Some(Instrument::Sampler(s)) = &p.tracks[0].instrument else {
        panic!()
    };
    let c = cuts_of(s);
    assert!((c[2].2 - 0.252).abs() < 1e-4, "{}", c[2].2);
    assert_eq!(c[1].3, c[2].2);
    // Joined back: one chop again, what played the second part playing the first's.
    p.join_chops(0, 2);
    assert_eq!(pitches(&p), [36, 37, 37, 38, 39]);
    let Some(Instrument::Sampler(s)) = &p.tracks[0].instrument else {
        panic!()
    };
    assert_eq!(s.zones.len(), 4);
    assert_eq!(cuts_of(s)[1].3, 0.5);
}
