use super::*;

#[test]
fn packs_are_the_first_folder() {
    assert_eq!(
        pack_of("MusicRadar Lo-Fi/ConstructionKits/01(100)"),
        "MusicRadar Lo-Fi"
    );
    assert_eq!(pack_of("Loose"), "Loose");
}
