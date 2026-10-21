use super::*;

#[test]
fn range_spans_blocks() {
    let mut s = vec![0f32; BLOCK * 4];
    s[10] = 0.5;
    s[BLOCK * 3 + 1] = -0.25;
    let p = Peaks::from_samples(&s);
    assert_eq!(p.range(0, BLOCK), (0.0, 0.5));
    assert_eq!(p.range(BLOCK, BLOCK * 3), (0.0, 0.0));
    assert_eq!(p.range(0, BLOCK * 4), (-0.25, 0.5));
    assert_eq!(p.range(BLOCK * 10, BLOCK * 11), (0.0, 0.0));
}
