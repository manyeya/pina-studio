use super::*;

#[test]
fn displays_are_read_for_their_numbers() {
    assert_eq!(number("-6.5 dB"), Some(-6.5));
    assert_eq!(number("2.40 kHz"), Some(2.4));
    assert_eq!(scale("2.40 kHz"), 1e3);
    assert_eq!(scale("850 Hz"), 1.0);
    assert_eq!(scale("12 ms"), 1.0);
    assert_eq!(number("-inf dB"), Some(-1e9));
    assert_eq!(number("Sine"), None);
    assert_eq!(number("FM2"), None);
    assert_eq!(number("50.00 % (Center)"), Some(50.0));
}

#[test]
fn a_stepped_change_ends_the_blocks_edits() {
    let (tx, mut rx) = edit_queue(vec![7]);
    for (id, v) in [(1, 0.1), (7, 0.5), (2, 0.2)] {
        tx.push(id, v);
    }
    let mut got = Vec::new();
    rx.drain(|id, _| got.push(id));
    assert_eq!(got, [1, 7]);
    assert!(rx.pending());
    rx.drain(|id, _| got.push(id));
    assert_eq!(got, [1, 7, 2]);
    assert!(!rx.pending());
}
