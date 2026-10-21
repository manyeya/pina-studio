use super::*;

#[test]
fn model_ids_read_as_names() {
    assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
    assert_eq!(model_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
    assert_eq!(model_name("custom"), "Custom");
}
