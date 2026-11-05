use super::*;
use crate::song::project::ChordShape;

const RATE: f64 = 48_000.0;
/// A beat at 120: half a second.
const BEAT: u64 = 24_000;
const BLOCK: usize = 512;

fn on(offset: u32, pitch: u8) -> Note {
    Note {
        offset,
        pitch,
        velocity: 0.8,
    }
}

fn off(offset: u32, pitch: u8) -> Note {
    Note {
        offset,
        pitch,
        velocity: 0.0,
    }
}

/// `samples` through `fx` a block at a time from the song's start, `played` going in at their
/// samples: what comes out, at its sample, as (sample, pitch, velocity out of 127).
fn through(
    fx: &[MidiFx],
    key: Option<Key>,
    played: &[(u64, Note)],
    samples: u64,
) -> Vec<(u64, u8, u8)> {
    let mut r = MidiFxRunner::new();
    let mut heard = Vec::new();
    let mut at = 0;
    while at < samples {
        let notes: Vec<Note> = played
            .iter()
            .filter(|(t, _)| (at..at + BLOCK as u64).contains(t))
            .map(|(t, n)| Note {
                offset: (t - at) as u32,
                ..*n
            })
            .collect();
        let clock = Clock {
            beats: Some(at as f64 / BEAT as f64),
            tempo: 120.0,
            rate: RATE,
        };
        for n in r.run(fx, key, &notes, BLOCK, clock) {
            heard.push((
                at + n.offset as u64,
                n.pitch,
                (n.velocity * 127.0).round() as u8,
            ));
        }
        at += BLOCK as u64;
    }
    heard
}

const C_MAJOR: Option<Key> = Some(Key {
    root: 0,
    minor: false,
});

#[test]
fn a_note_becomes_a_chord_and_its_note_off_ends_all_of_it() {
    let fx = [MidiFx::ChordTrigger {
        shape: ChordShape::InKey,
    }];
    let heard = through(&fx, C_MAJOR, &[(0, on(0, 62)), (1000, off(0, 62))], 2048);
    let ons: Vec<u8> = heard.iter().filter(|h| h.2 > 0).map(|h| h.1).collect();
    let offs: Vec<u8> = heard.iter().filter(|h| h.2 == 0).map(|h| h.1).collect();
    assert_eq!(ons, [62, 65, 69], "D minor in C major");
    assert_eq!(offs.len(), 3);
    assert!(heard.iter().filter(|h| h.2 == 0).all(|h| h.0 == 1000));
}

#[test]
fn a_note_off_ends_what_its_note_became_even_once_the_setting_has_moved() {
    let mut r = MidiFxRunner::new();
    let clock = Clock {
        beats: None,
        tempo: 120.0,
        rate: RATE,
    };
    let up = |by| [MidiFx::Transposer { semitones: by }];
    assert_eq!(
        r.run(&up(12), None, &[on(0, 60)], BLOCK, clock)[0].pitch,
        72
    );
    // Turned to a fifth while held: its note-off still ends the octave.
    let ended = r.run(&up(7), None, &[off(0, 60)], BLOCK, clock);
    assert_eq!((ended[0].pitch, ended[0].velocity), (72, 0.0));
}

#[test]
fn notes_kept_to_a_scale_velocities_kept_to_a_range() {
    let fx = [
        MidiFx::ScaleQuantizer {
            root: None,
            minor: false,
        },
        MidiFx::Velocity {
            scale: 50,
            add: 10,
            min: 1,
            max: 40,
        },
    ];
    let heard = through(
        &fx,
        C_MAJOR,
        &[(0, on(0, 61)), (10, on(0, 66))],
        BLOCK as u64,
    );
    // C♯ to C, F♯ to F; 0.8 × 127 halved and 10 added is 61, kept to 40.
    assert_eq!(heard, [(0, 60, 40), (10, 65, 40)]);
}

#[test]
fn the_arpeggiator_steps_up_what_is_held_on_the_beat() {
    let fx = [MidiFx::Arpeggiator {
        rate: 0.25,
        order: ArpOrder::Up,
        octaves: 2,
        gate: 0.5,
        latch: false,
    }];
    // C E G held from just before the beat, for a beat and a half.
    let played = [
        (0, on(0, 67)),
        (0, on(0, 60)),
        (0, on(0, 64)),
        (36_000, off(0, 60)),
    ];
    let heard = through(&fx, None, &played, BEAT * 2);
    let ons: Vec<(u64, u8)> = heard
        .iter()
        .filter(|h| h.2 > 0)
        .map(|h| (h.0, h.1))
        .collect();
    // A sixteenth apart, up C E G then an octave up; C let go, on with E and G.
    let step = BEAT / 4;
    assert_eq!(
        ons[..6],
        [
            (0, 60),
            (step, 64),
            (step * 2, 67),
            (step * 3, 72),
            (step * 4, 76),
            (step * 5, 79)
        ]
    );
    assert!(
        ons.iter().all(|&(t, p)| t < 36_000 || p != 60 && p != 72),
        "{ons:?}"
    );
    // Each half a step long.
    let first_off = heard.iter().find(|h| h.1 == 60 && h.2 == 0).unwrap().0;
    assert_eq!(first_off, step / 2);
}

#[test]
fn a_latched_arpeggiator_plays_on_until_new_keys_go_down() {
    let fx = [MidiFx::Arpeggiator {
        rate: 0.5,
        order: ArpOrder::Down,
        octaves: 1,
        gate: 0.5,
        latch: true,
    }];
    let played = [
        (0, on(0, 60)),
        (0, on(0, 64)),
        (100, off(0, 60)),
        (100, off(0, 64)),
        // A new chord, after the first was let go: it alone plays on.
        (BEAT * 2, on(0, 70)),
        (BEAT * 2 + 100, off(0, 70)),
    ];
    let heard = through(&fx, None, &played, BEAT * 3 - BLOCK as u64);
    let ons: Vec<u8> = heard.iter().filter(|h| h.2 > 0).map(|h| h.1).collect();
    // Half-beat steps: the first chord down, twice; the new one alone.
    assert_eq!(ons, [64, 60, 64, 60, 70, 70]);
}

#[test]
fn the_note_repeater_echoes_each_note_softer_and_the_randomizer_can_leave_it_out() {
    let fx = [MidiFx::NoteRepeater {
        count: 2,
        rate: 0.25,
        decay: 0.5,
        transpose: 12,
    }];
    let heard = through(&fx, None, &[(0, on(0, 60))], BEAT);
    let ons: Vec<(u64, u8, u8)> = heard.into_iter().filter(|h| h.2 > 0).collect();
    assert_eq!(ons, [(0, 60, 102), (BEAT / 4, 72, 51), (BEAT / 2, 84, 25)]);

    let never = [MidiFx::Randomizer {
        pitch: 0,
        velocity: 0,
        chance: 0,
    }];
    assert!(
        through(
            &never,
            None,
            &[(0, on(0, 60)), (10, off(0, 60))],
            BLOCK as u64
        )
        .is_empty()
    );
}

#[test]
fn stopping_ends_what_an_arpeggiator_was_playing() {
    let fx = [MidiFx::Arpeggiator {
        rate: 1.0,
        order: ArpOrder::Up,
        octaves: 1,
        gate: 1.0,
        latch: true,
    }];
    let mut r = MidiFxRunner::new();
    let clock = Clock {
        beats: Some(0.0),
        tempo: 120.0,
        rate: RATE,
    };
    let first = r.run(&fx, None, &[on(0, 60)], BLOCK, clock);
    assert_eq!(first.len(), 1);
    r.stop();
    let after = r.run(&fx, None, &[], BLOCK, clock);
    assert_eq!(after.len(), 1);
    assert_eq!((after[0].pitch, after[0].velocity), (60, 0.0));
}

#[test]
fn a_modifier_sends_one_controller_as_another() {
    let mut r = MidiFxRunner::new();
    let fx = [MidiFx::Modifier {
        from: 1,
        to: 74,
        scale: 50,
    }];
    let wheel = Control {
        offset: 3,
        kind: ControlKind::Cc {
            number: 1,
            value: 1.0,
        },
    };
    let out = r.controls(&fx, &[wheel]);
    assert!(matches!(out[0].kind, ControlKind::Cc { number: 74, value } if value == 0.5));
}
