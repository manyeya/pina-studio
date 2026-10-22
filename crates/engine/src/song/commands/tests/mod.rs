mod mixing;
mod modulation;
mod plugins;
mod songs;

use super::*;
#[test]
fn a_drum_pattern() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Drums", "instrument": "drums"}"#,
    )
    .unwrap();
    let r = run(
        &mut p,
        r#"{"op": "add_pattern_region", "track": 0, "start": 0, "length": 3840,
            "rows": [{"pitch": 36, "cells": [100, 0, 0, 0, 100, 0, 0, 0]}]}"#,
    );
    assert_eq!(r, Ok(json!({"region": [0, 0]})));
    let set = r#"{"op": "set_pattern", "track": 0, "region": 0, "rows": [{"pitch": 38, "cells": [0, 0, 0, 0, 90]}]}"#;
    run(&mut p, set).unwrap();
    let notes = p.region((0, 0)).content.notes(3840).unwrap();
    // Five-step loop, a snare on step 5: sixteenths 4, 9 and 14 in a bar of 16.
    let starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
    assert_eq!(starts, [960, 2160, 3360]);
}

#[test]
fn a_patterns_settings_and_an_automation_row() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Bass", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_pattern_region", "track": 0, "start": 0, "length": 3840, "steps": 8,
            "rows": [{"pitch": 36, "cells": [100, 0, 0, 0]}]}"#,
    )
    .unwrap();
    let pattern = |p: &Project| match &p.region((0, 0)).content {
        Content::Pattern(pt) => pt.clone(),
        _ => unreachable!(),
    };
    assert_eq!(pattern(&p).steps(), 8);
    // Longer rows than it had: it's as long as they are.
    let set = r#"{"op": "set_pattern", "track": 0, "region": 0, "mode": "ping_pong",
        "scale": "dorian", "key": 2, "rows": [
            {"pitch": 38, "cells": [100, 0, 100, 0, 100, 0, 100, 0, 100, 0, 100, 0, 100, 0, 100, 0],
             "tie": [true]},
            {"target": {"cc": 74}, "pitch": 0, "cells": [100, 0, 0, 0], "value": [0.8], "slide": true}
        ]}"#;
    run(&mut p, set).unwrap();
    let pt = pattern(&p);
    assert_eq!(pt.steps(), 16);
    assert_eq!(
        (pt.mode, pt.scale, pt.key),
        (pina_mode(), Some(crate::song::project::Scale::Dorian), 2)
    );
    assert_eq!(
        pt.rows[1].target,
        Some(crate::song::project::RowTarget::Cc(74))
    );
    assert!(
        run(
            &mut p,
            r#"{"op": "set_pattern", "track": 0, "region": 0, "scale": "jazz"}"#
        )
        .is_err()
    );
    assert!(
        run(
            &mut p,
            r#"{"op": "set_pattern", "track": 0, "region": 0, "steps": 65}"#
        )
        .is_err()
    );
}

fn pina_mode() -> crate::song::project::PlayMode {
    crate::song::project::PlayMode::PingPong
}

fn run(p: &mut Project, json: &str) -> Result<Value, String> {
    let cmd: Command = serde_json::from_str(json).map_err(|e| e.to_string())?;
    apply(p, Path::new("/nonexistent"), cmd)
}

/// A plugin that only shows things: a cutoff from 0 to 1000 Hz, a three-way wave, two
/// presets. It reads no text itself, so `resolve` reads its display.
struct Shown;

impl PluginApi for Shown {
    fn name(&self) -> &str {
        "Shown"
    }
    fn params(&self) -> Vec<PluginParam> {
        let param = |id, name: &str, steps| PluginParam {
            id,
            name: name.into(),
            steps,
            ..Default::default()
        };
        vec![param(3, "Filter Cutoff", 0), param(9, "Wave", 2)]
    }
    fn set_param(&self, _: u32, _: f64) {}
    fn show_param(&self, _: u32, _: f64) {}
    fn param_value(&self, _: u32) -> f64 {
        0.0
    }
    fn param_text(&self, id: u32, v: f64) -> String {
        match id {
            3 => format!("{:.1} Hz", v * 1000.0),
            _ => ["Saw", "Square", "Sine"][(v * 2.0).round() as usize].into(),
        }
    }
    fn param_from_text(&self, _: u32, _: &str) -> Option<f64> {
        None
    }
    fn state(&self) -> Option<Vec<u8>> {
        None
    }
    fn set_state(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn presets(&self) -> Vec<Preset> {
        let program = |n| PresetSource::Program {
            list: 0,
            program: n,
        };
        vec![
            Preset {
                name: "Warm Pad".into(),
                source: program(0),
            },
            Preset {
                name: "Sub Bass".into(),
                source: program(1),
            },
        ]
    }
    fn load_preset(&self, _: &Preset) -> Result<(), String> {
        Ok(())
    }
    fn save_preset(&self, _: &str) -> Result<PathBuf, String> {
        Err("no".into())
    }
    fn open_editor(
        &self,
        _: crate::plugin::MainThreadMarker,
    ) -> Result<crate::plugin::Editor, String> {
        Err("no".into())
    }
}

impl PluginSource for Shown {
    fn plugin(&mut self, _: &Project, _: ChannelRef, _: Which) -> Result<&dyn PluginApi, String> {
        Ok(self)
    }
}
