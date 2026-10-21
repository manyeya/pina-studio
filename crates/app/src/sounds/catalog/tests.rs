use super::*;
use std::path::Path;

fn plugin(
    format: Format,
    class: &str,
    vendor: &str,
    instrument: bool,
    cats: &[&str],
    uid: Option<&str>,
) -> PluginInfo {
    PluginInfo {
        format,
        path: Path::new("/p").join(class),
        class: class.into(),
        uid: uid.map(str::to_string),
        vendor: vendor.into(),
        instrument,
        categories: cats.iter().map(|c| c.to_string()).collect(),
    }
}

#[test]
fn plugins_go_where_they_say_once_each() {
    let all = [
        plugin(
            Format::Vst3,
            "Dragonfly Hall Reverb",
            "Michael Willis",
            false,
            &["Fx", "Reverb"],
            None,
        ),
        plugin(
            Format::Clap,
            "Dragonfly Hall Reverb",
            "Michael Willis",
            false,
            &["audio-effect", "reverb"],
            Some("x"),
        ),
        plugin(
            Format::Au,
            "AUDelay",
            "Apple",
            false,
            &[],
            Some("aufx dely appl"),
        ),
        plugin(
            Format::Clap,
            "Surge XT",
            "Surge Synth Team",
            true,
            &["instrument", "synthesizer"],
            Some("s"),
        ),
        plugin(
            Format::Vst3,
            "Tape Echo Thing",
            "Someone",
            false,
            &["Fx"],
            None,
        ),
        plugin(Format::Vst3, "Glue Box", "Someone", false, &[], None),
    ];
    let d = devices(&all);
    let find = |name: &str| d.iter().find(|x| x.name == name).unwrap();
    let hall = find("Dragonfly Hall Reverb");
    assert_eq!(hall.cat, Cat::Reverb);
    // Both formats, one entry, CLAP first.
    assert_eq!(hall.badges(), ["CLAP", "VST3"]);
    assert_eq!(
        d.iter()
            .filter(|x| x.name == "Dragonfly Hall Reverb")
            .count(),
        1
    );
    assert_eq!(find("AUDelay").cat, Cat::Delay);
    assert_eq!(find("Surge XT").cat, Cat::Synth);
    // Silent categories: its name says.
    assert_eq!(find("Tape Echo Thing").cat, Cat::Delay);
    assert_eq!(find("Glue Box").cat, Cat::Dynamics);
    // Searches go by words that mean it: "echo" finds the delays, "space" the reverb.
    let found = |q: &str| -> Vec<&str> {
        let mut s: Vec<(u32, &str)> = d
            .iter()
            .filter_map(|x| score(x, q).map(|n| (n, x.name.as_str())))
            .collect();
        s.sort_by_key(|a| std::cmp::Reverse(a.0));
        s.into_iter().map(|x| x.1).collect()
    };
    assert_eq!(found("echo")[0], "Tape Echo Thing");
    assert!(found("echo").contains(&"AUDelay"));
    assert_eq!(found("space"), ["Dragonfly Hall Reverb"]);
    assert_eq!(found("surge")[0], "Surge XT");
    assert!(found("zzz").is_empty());
}
