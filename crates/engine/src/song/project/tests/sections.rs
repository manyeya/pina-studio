//! Sections: moved, swapped, copied and deleted with everything in them.

use super::*;

const BAR: u64 = PPQ * 4;

/// Three sections, A B C, a bar each: a note in each (60, 62, 64), a volume point in each, a
/// region across B and C (its 70 in B, its 72 on C's first beat), and B at 90 BPM while the
/// rest is at 120.
fn song() -> Project {
    let mut p = project(
        vec![midi(
            0,
            BAR * 3,
            &[(0, 60), (BAR as i64, 62), (2 * BAR as i64, 64)],
        )],
        true,
    );
    p.tracks[0]
        .regions
        .push(midi(BAR + BAR / 2, BAR, &[(0, 70), (BAR as i64 / 2, 72)]));
    for (k, v) in [(0u64, -1.0f32), (1, -2.0), (2, -3.0)] {
        p.tracks[0]
            .automation
            .add(Param::Volume, AutoPoint::new(k * BAR + PPQ, v));
    }
    p.markers = ["A", "B", "C"]
        .iter()
        .enumerate()
        .map(|(k, n)| Marker {
            tick: k as u64 * BAR,
            name: n.to_string(),
            ..Default::default()
        })
        .collect();
    p.set_tempo_at(BAR, 90.0);
    p.set_tempo_at(BAR * 2, 120.0);
    p
}

/// Every note played, in time order: (tick, pitch).
fn played(p: &Project) -> Vec<(u64, u8)> {
    let mut out: Vec<(u64, u8)> = p.tracks[0]
        .regions
        .iter()
        .flat_map(|r| {
            let notes: Vec<Note> = r.content.notes(r.length).unwrap_or_default().into_owned();
            notes
                .into_iter()
                .filter(|n| n.start >= 0 && (n.start as u64) < r.length)
                .map(|n| (r.start + n.start as u64, n.pitch))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

fn names(p: &Project) -> Vec<String> {
    p.markers.iter().map(|m| m.name.clone()).collect()
}

#[test]
fn a_section_moves_with_everything_in_it() {
    let mut p = song();
    // C before A: C A B.
    p.move_section(2, 0).unwrap();
    assert_eq!(names(&p), ["C", "A", "B"]);
    assert_eq!(
        p.markers.iter().map(|m| m.tick).collect::<Vec<_>>(),
        [0, BAR, BAR * 2]
    );
    assert_eq!(
        played(&p),
        [
            (0, 64),
            (0, 72),
            (BAR, 60),
            (BAR * 2, 62),
            (BAR * 2 + BAR / 2, 70)
        ]
    );
    // Its automation went with it.
    let vol = p.tracks[0].automation.points(Param::Volume);
    assert_eq!(value_at(vol, PPQ as f64), Some(-3.0));
    assert_eq!(value_at(vol, (BAR + PPQ) as f64), Some(-1.0));
    // B keeps its tempo where it is now; the rest is at 120.
    assert_eq!((p.tempo_at(0), p.tempo_at(BAR * 2)), (120.0, 90.0));
    assert_eq!(p.tempo_at(BAR), 120.0);
}

#[test]
fn sections_swap_copy_and_go() {
    let mut p = song();
    p.swap_sections(0, 2).unwrap();
    assert_eq!(names(&p), ["C", "B", "A"]);
    assert_eq!(played(&p).first(), Some(&(0, 64)));
    let mut p = song();
    // A copy of B before A: B A B C, a bar longer.
    p.copy_section(1, 0).unwrap();
    assert_eq!(names(&p), ["B", "A", "B", "C"]);
    assert_eq!(p.sections().last().unwrap().1, BAR * 4);
    assert_eq!(played(&p)[0], (0, 62));
    // B gone: A C, the gap closed, the region across B and C cut to its part in C.
    let mut p = song();
    p.delete_section(1).unwrap();
    assert_eq!(names(&p), ["A", "C"]);
    assert_eq!(played(&p), [(0, 60), (BAR, 64), (BAR, 72)]);
    assert_eq!(p.tempo_at(BAR), 120.0);
    // A section added after C: eight bars of its own.
    let mut p = song();
    let at = p.add_section("Outro", 8);
    assert_eq!(at, BAR * 3);
    assert_eq!(p.sections().last(), Some(&(BAR * 3, BAR * 11)));
}

#[test]
fn time_goes_in_and_out() {
    let mut p = song();
    p.insert_time(BAR, BAR);
    assert_eq!(played(&p)[1], (BAR * 2, 62));
    // The line holds across the gap.
    let vol = p.tracks[0].automation.points(Param::Volume);
    assert_eq!(value_at(vol, (BAR + PPQ) as f64), value_at(vol, BAR as f64));
    p.remove_time(BAR, BAR * 2);
    assert_eq!(played(&p), played(&song()));
}
