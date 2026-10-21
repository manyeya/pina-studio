/// Faust's flush to zero writes `(-x.to_bits()…)`, negating the bits rather than the number:
/// the bits of -0.0 overflow, a panic in a debug build. `scripts/faust.sh` puts the minus
/// where it belongs; this keeps it there.
#[test]
fn the_generated_effects_negate_numbers_not_their_bits() {
    for (name, code) in [
        ("sat", include_str!("sat.rs")),
        ("comp", include_str!("comp.rs")),
    ] {
        for (at, _) in code.match_indices("(-fTemp") {
            let rest = code[at + 7..].trim_start_matches(|c: char| c.is_ascii_digit());
            assert!(
                !rest.starts_with(".to_bits()"),
                "{name}.rs negates a number's bits at byte {at}"
            );
        }
    }
}
