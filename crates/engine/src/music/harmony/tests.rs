use super::*;
use crate::music::theory::Quality;
use crate::song::project::PPQ;

const C: Key = Key {
    root: 0,
    minor: false,
};

#[test]
fn a_keys_chords_and_those_it_borrows() {
    let names: Vec<String> = in_key(C, false).iter().map(|(_, c)| c.name()).collect();
    assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
    let sevenths: Vec<String> = in_key(C, true).iter().map(|(_, c)| c.name()).collect();
    assert_eq!(sevenths[4], "G7");
    let a_minor = Key {
        root: 9,
        minor: true,
    };
    assert_eq!(in_key(a_minor, false)[0].1.name(), "Am");
    let lent: Vec<String> = borrowed(C).iter().map(|(_, c)| c.name()).collect();
    assert!(
        lent.contains(&"Fm".to_string()) && lent.contains(&"Bb".to_string()),
        "{lent:?}"
    );
}

#[test]
fn a_progression_reads_bar_by_bar_and_the_library_by_mood() {
    let p = Progression {
        numerals: "ii9 | V13 | Imaj9 V7",
        minor: false,
        suits: &[],
    };
    let chords = progression(&p, C);
    let shares: Vec<f32> = chords.iter().map(|c| c.1).collect();
    assert_eq!(shares, [1.0, 1.0, 0.5, 0.5]);
    assert_eq!(chords[0].0.root, 2);
    assert!(!progressions(true, None).is_empty());
    let mood = moods()[0];
    assert!(progressions(false, Some(mood)).len() + progressions(true, Some(mood)).len() > 0);
}

#[test]
fn chords_are_voiced_close_open_spread_or_led_by_the_fewest_steps() {
    let chord = |root, quality| Chord {
        root,
        quality,
        bass: None,
    };
    let walk = [
        chord(0, Quality::Maj),
        chord(5, Quality::Maj),
        chord(7, Quality::Maj),
    ];
    let close = voice(&walk, Voicing::Close, 60);
    assert_eq!(close[0], [60, 64, 67]);
    let open = voice(&walk, Voicing::Open, 60);
    assert_eq!(open[0], [52, 60, 67]);
    let spread = voice(&walk, Voicing::Spread, 60);
    assert_eq!(spread[0], [48, 64, 67]);
    // Led: each chord's notes near the last's, so the hand moves less than it does close.
    let moves = |v: &[Vec<u8>]| -> i32 {
        v.windows(2)
            .map(|w| {
                let c = |x: &Vec<u8>| x.iter().map(|&p| p as i32).sum::<i32>();
                (c(&w[1]) - c(&w[0])).abs()
            })
            .sum()
    };
    let led = voice(&walk, Voicing::Led, 60);
    assert!(moves(&led) < moves(&close), "{led:?} {close:?}");
    assert!(
        led.iter()
            .all(|v| v.iter().all(|&p| (48..=80).contains(&p)))
    );
}

#[test]
fn voiced_chords_are_played_held_arpeggiated_or_in_a_rhythm_with_a_bass() {
    let voiced = vec![vec![60, 64, 67], vec![57, 60, 64]];
    let bar = PPQ * 4;
    let held = perform(&voiced, &[bar, bar], Performance::Block, PPQ, false);
    assert_eq!(held.len(), 6);
    assert_eq!(held[3].start, bar as i64);
    let arp = perform(&voiced, &[bar, bar], Performance::Arp, PPQ, false);
    assert_eq!(arp.len(), 32);
    assert_eq!(
        arp[..4].iter().map(|n| n.pitch).collect::<Vec<_>>(),
        [60, 64, 67, 60]
    );
    let with_bass = perform(&voiced, &[bar, bar], Performance::Pulse, PPQ, true);
    assert!(with_bass.iter().any(|n| n.pitch == 36 && n.length == bar));
    let rhythm = perform(&voiced[..1], &[bar], Performance::Rhythm, PPQ, false);
    assert_eq!(rhythm.len(), 6 * 3);
}

#[test]
fn the_chords_some_notes_play_are_read_back() {
    let bar = (PPQ * 4) as i64;
    let n = |start: i64, pitch: u8| Note {
        start,
        length: bar as u64,
        pitch,
        velocity: 90,
        ..Default::default()
    };
    let notes = [
        n(0, 60),
        n(0, 64),
        n(0, 67),
        n(bar, 57),
        n(bar, 60),
        n(bar, 64),
    ];
    let read: Vec<(i64, String)> = read_chords(&notes, (0, bar * 3), PPQ)
        .iter()
        .map(|(s, _, c)| (*s, c.name()))
        .collect();
    assert_eq!(read, [(0, "C".to_string()), (bar, "Am".to_string())]);
}
