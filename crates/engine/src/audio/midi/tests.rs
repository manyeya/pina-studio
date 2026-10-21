use super::*;

#[test]
fn parses_notes_on_any_channel() {
    let on = |pitch, velocity| Some(LiveNote { pitch, velocity });
    assert_eq!(parse(&[0x90, 60, 100]), on(60, 100));
    assert_eq!(parse(&[0x95, 61, 0]), on(61, 0)); // note on at velocity 0 is an off
    assert_eq!(parse(&[0x83, 62, 64]), on(62, 0));
    assert_eq!(parse(&[0xB0, 7, 100]), None); // a controller
    assert_eq!(parse(&[0x90]), None);
}
