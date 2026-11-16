use super::*;

fn w(text: &str, start: f32, end: f32) -> Word {
    Word {
        text: text.into(),
        start,
        end,
    }
}

#[test]
fn what_whisper_makes_up_over_music_is_left_out() {
    let raw = vec![
        w("", 0.0, 0.0),
        w(" (singing", 0.0, 1.0),
        w(" in", 1.0, 1.2),
        w(" foreign", 1.2, 2.0),
        w(" language)", 2.0, 3.0),
        w(" ♪", 3.0, 3.5),
        w(" Ain't", 4.0, 4.5),
        w(" [music]", 5.0, 6.0),
        w(" no", 6.0, 6.3),
    ];
    let kept: Vec<String> = words(raw).into_iter().map(|w| w.text).collect();
    assert_eq!(kept, ["Ain't", "no"]);
    // Its loops over a beat: a word on and on, a stutter. Three times is a lyric.
    let mut raw: Vec<Word> = (0..16)
        .map(|i| w(" Yo", i as f32, i as f32 + 1.0))
        .collect();
    raw.push(w(" ca-ca-ca-ca-ca", 16.0, 17.0));
    raw.extend((0..3).map(|i| w(" Hey,", 17.0 + i as f32, 18.0 + i as f32)));
    let kept: Vec<String> = words(raw).into_iter().map(|w| w.text).collect();
    assert_eq!(kept, ["Hey,", "Hey,", "Hey,"]);
}

#[test]
fn words_make_lines_at_capitals_pauses_and_commas_and_repeats_are_hooks() {
    // As Whisper heard a chorus: each line starts with a capital, but "I" doesn't.
    let words: Vec<Word> = [
        ("Ain't", 0.0, 1.1),
        ("no", 1.1, 1.5),
        ("holding", 1.5, 3.0),
        ("back", 3.0, 3.9),
        ("Because", 5.0, 6.4),
        ("you", 6.4, 7.0),
        ("are", 7.0, 7.6),
        ("all", 7.6, 8.3),
        ("I", 8.3, 8.4),
        ("see,", 8.4, 9.0),
        ("Ain't", 9.0, 9.4),
        ("no", 9.4, 9.8),
        ("holding", 9.8, 10.5),
        ("back", 10.5, 11.0),
        // After a pause, a line of its own.
        ("oh", 13.0, 13.5),
        ("yeah", 13.5, 14.0),
    ]
    .iter()
    .map(|&(t, a, b)| w(t, a, b))
    .collect();
    // The voice starts a little before Whisper heard "Because"; an onset long before is ignored.
    let found = lines(&words, &[0.0, 4.2, 4.8, 9.0]);
    let got: Vec<(&str, f32, usize)> = found
        .iter()
        .map(|l| (l.text.as_str(), l.start, l.sung))
        .collect();
    assert_eq!(
        got,
        [
            ("Ain't no holding back", 0.0, 2),
            ("Because you are all I see", 4.8, 1),
            ("Ain't no holding back", 9.0, 2),
            ("oh yeah", 13.0, 1),
        ]
    );
    assert_eq!(found[1].end, 9.0);
    // Each keeps its words, where they're sung.
    let said: Vec<&str> = found[3].words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(said, ["oh", "yeah"]);
    assert_eq!(found[3].words[1].start, 13.5);
}

#[test]
fn audio_is_heard_as_one_channel_at_16_khz() {
    let left = vec![1.0f32; 48_000];
    let right = vec![0.0f32; 48_000];
    let m = mono_16k(&[left, right], 48_000.0);
    assert!((m.len() as i64 - 16_000).abs() <= 2, "{}", m.len());
    assert!((m[8_000] - 0.5).abs() < 0.01, "{}", m[8_000]);
}

#[test]
fn a_line_run_on_is_split_at_its_widest_gap() {
    // Twelve words: "a million hearts beat as one pulse" then, after a breath, "I came in heavy".
    let said = [
        ("A", 0.0),
        ("million", 0.3),
        ("hearts", 0.7),
        ("beat", 1.0),
        ("as", 1.3),
        ("one", 1.5),
        ("pulse", 1.8),
        ("I", 2.6),
        ("came", 2.8),
        ("in", 3.0),
        ("heavy", 3.2),
        ("tonight", 3.6),
    ];
    let words: Vec<Word> = said.iter().map(|&(t, a)| w(t, a, a + 0.25)).collect();
    let found = lines(&words, &[]);
    let texts: Vec<&str> = found.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "A million hearts beat as one pulse",
            "I came in heavy tonight"
        ]
    );
}
