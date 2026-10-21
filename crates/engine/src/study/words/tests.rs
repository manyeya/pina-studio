use super::*;

#[test]
fn plain_lyrics_lose_stray_stamps_and_headings() {
    let lines = parse(
        None,
        Some("Hello, it's me\n[00:23.54] I was wondering\n[Chorus]\n\nHello"),
    );
    let texts: Vec<&str> = lines.iter().map(|l| l.1.as_str()).collect();
    assert_eq!(texts, ["Hello, it's me", "I was wondering", "Hello"]);
}

#[test]
fn rhymes_and_syllables_are_heard_in_the_spelling() {
    assert_eq!(syllables("beautiful"), 3);
    assert_eq!(syllables("time"), 1);
    assert_eq!(syllables("little"), 2);
    assert_eq!(rhyme_key("night"), rhyme_key("light"));
    assert_eq!(rhyme_key("away"), rhyme_key("today"));
    assert_eq!(rhyme_key("fire"), rhyme_key("desire"));
    assert_ne!(rhyme_key("night"), rhyme_key("day"));
    assert_eq!(
        scheme(&[
            "I see the light",
            "out in the night",
            "you went away",
            "I'll be here today"
        ]),
        "AABB"
    );
    assert_eq!(
        scheme(&[
            "walking in the rain",
            "under the stars",
            "calling out your name",
            "under the scars"
        ]),
        "ABAB"
    );
}

#[test]
fn synced_lyrics_land_in_their_sections() {
    let lrc = "[00:01.00] here we go\n[00:05.00] hold me now\n[00:09.00] hold me now\n[00:13.00] hold me now\n[00:20.00] [00:24.00] oh oh";
    let lines = parse(Some(lrc), None);
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5], (Some(24.0), "oh oh".to_string()));
    let l = read(&lines, &[(0.0, 4.0), (4.0, 16.0), (16.0, 30.0)], "test");
    assert_eq!(l.lines[1].section, Some(1));
    assert_eq!(l.hooks, ["hold me now"]);
    assert!(l.voice.starts_with("first person"));
}
