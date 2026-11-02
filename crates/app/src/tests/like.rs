//! Songs like a reference: a spectrogram picture studied, read with its axes, and made into a new
//! song for the agent to write (Pina writing no notes of it); a song compared with its recorded
//! reference, a revision asked for, judged and stopped; and the same through the agents' tools.

use super::*;
use crate::agent::{Kind, Readiness};
use pina_engine::study::reference::{
    Calibration, Closeness, Colors, FreqScale, Source,
    image::{Picture, save, table},
};
use serde_json::{Value, json};

/// A spectrogram drawn as a plotting tool exports one: a white margin, a black frame, and in
/// viridis a held line at 1 kHz (on a log axis from 50 Hz to 10 kHz) and a kick low down twice a
/// second, over twenty seconds.
fn plot_png(file: &Path) {
    let (w, h, m) = (600u32, 240u32, 36u32);
    let t = table(Colors::Viridis);
    let line = ((1.0 - FreqScale::Log.up(50.0, 10_000.0, 1000.0)) * h as f32) as u32;
    let (pw, ph) = (w + 2 * m, h + 2 * m);
    let mut pixels = vec![[255u8; 3]; (pw * ph) as usize];
    for y in 0..h {
        for x in 0..w {
            let at = x as f32 / w as f32 * 20.0;
            let v: f32 = 0.12
                + if y.abs_diff(line) <= 1 { 0.7 } else { 0.0 }
                + if (at * 2.0).fract() < 0.06 && y > h * 3 / 4 {
                    0.75
                } else {
                    0.0
                };
            let c = t[(v.min(1.0) * 63.0) as usize];
            pixels[((y + m) * pw + x + m) as usize] = c.map(|c| c as u8);
        }
    }
    for x in m - 1..=m + w {
        pixels[((m - 1) * pw + x) as usize] = [0; 3];
        pixels[((m + h) * pw + x) as usize] = [0; 3];
    }
    for y in m - 1..=m + h {
        pixels[(y * pw + m - 1) as usize] = [0; 3];
        pixels[(y * pw + m + w) as usize] = [0; 3];
    }
    save(
        &Picture {
            width: pw,
            height: ph,
            pixels,
        },
        file,
    )
    .unwrap();
}

/// A reference recording: a kick on each beat and hats between at 120 BPM over a held C, eight
/// bars.
fn reference_wav(file: &Path) {
    let rate = 44_100usize;
    let n = rate * 16;
    let mut x = vec![0f32; n];
    let mut seed = 7u32;
    for b in 0..32 {
        let at = b * rate / 2;
        for i in 0..rate / 5 {
            let s = i as f32 / rate as f32;
            if at + i < n {
                x[at + i] += 0.7 * (-s * 18.0).exp() * (std::f32::consts::TAU * 60.0 * s).sin();
            }
        }
        let off = at + rate / 4;
        for i in 0..rate / 25 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let w = (seed >> 9) as f32 / (1u32 << 23) as f32 - 1.0;
            if off + i < n {
                x[off + i] += 0.25 * (-(i as f32 / rate as f32) * 60.0).exp() * w;
            }
        }
    }
    for (i, v) in x.iter_mut().enumerate() {
        *v += 0.15 * (std::f32::consts::TAU * 523.25 * i as f32 / rate as f32).sin();
    }
    pina_engine::music::analysis::write_wav(file, &x, &x, rate as u32).unwrap();
}

/// No agent on hand: a question is noted, not sent.
fn no_agent(ws: &mut Workspace) {
    ws.chat.kind = Kind::ClaudeCode;
    ws.chat.ready = vec![
        (Kind::ClaudeCode, Readiness::Missing),
        (Kind::Codex, Readiness::Missing),
    ];
}

fn asked(ws: &Workspace) -> Vec<String> {
    ws.chat
        .items
        .iter()
        .filter_map(|i| match i {
            crate::agent::chat::Item::You(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn a_spectrogram_picture_is_read_given_its_axes_and_made_into_a_song_like_it(
    cx: &mut TestAppContext,
) {
    let studies = test_studies();
    let songs = test_songs();
    let png = studies.join("Reference plot.png");
    plot_png(&png);
    cx.executor().allow_parking();
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        no_agent(ws);
        ws.study_file(png.clone(), cx);
    });
    until(cx, &ws, |ws| ws.opened_picture().is_some());
    // Read on its own: its plot and colours found, its axes asked for, no pitch claimed.
    let bp = ws.read_with(cx, |ws, _| ws.opened_picture().unwrap());
    let c = bp.calibration.clone().unwrap();
    assert_eq!(c.colors, Some(Colors::Viridis));
    assert!(c.crop.is_some_and(|[x, y, w, h]| x.abs_diff(36) <= 2
        && y.abs_diff(36) <= 2
        && w.abs_diff(600) <= 4
        && h.abs_diff(240) <= 4));
    assert_eq!(bp.needs.len(), 2, "{:?}", bp.needs);
    assert!(bp.contours.iter().all(|c| c.hz.is_none()));
    cx.run_until_parked();
    for sel in [
        "like-picture",
        "like-needs",
        "like-spectrum",
        "like-facts-measured",
        "like-facts-estimated",
        "like-facts-unknown",
    ] {
        assert!(cx.debug_bounds(sel).is_some(), "{sel}");
    }
    assert!(cx.debug_bounds("picture-0").is_some(), "in the rail");
    // Its axes given: it says when and at what pitch, and asks for nothing more.
    ws.update(cx, |ws, cx| {
        let r = ws.studies.like.reading.as_ref().unwrap();
        let (s, l, h) = (r.seconds.clone(), r.low.clone(), r.high.clone());
        s.update(cx, |t, cx| t.set_text("20", cx));
        l.update(cx, |t, cx| t.set_text("50", cx));
        h.update(cx, |t, cx| t.set_text("10000", cx));
    });
    click_on(cx, "like-scale-log", Modifiers::none(), MouseButton::Left);
    click_on(cx, "like-reread", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.opened_picture()
            .is_some_and(|b| b.seconds == Some(20.0) && b.needs.is_empty())
    });
    let bp = ws.read_with(cx, |ws, _| ws.opened_picture().unwrap());
    let line = bp.contours.iter().max_by_key(|c| c.end - c.start).unwrap();
    assert!(
        line.hz.is_some_and(|hz| (hz / 1000.0 - 1.0).abs() < 0.08),
        "{line:?}"
    );
    // Kept: studying it again finds the reading given.
    let again = crate::cli::reference::blueprint(&png, &Calibration::default()).unwrap();
    assert_eq!(again.seconds, Some(20.0));
    // Its length taken out of the reading and read again: unknown again, and asked for.
    ws.update(cx, |ws, cx| {
        let s = ws.studies.like.reading.as_ref().unwrap().seconds.clone();
        s.update(cx, |t, cx| t.set_text("", cx));
    });
    click_on(cx, "like-reread", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.opened_picture()
            .is_some_and(|b| b.seconds.is_none() && b.needs.len() == 1)
    });
    // And given back ("20 s", as people type it).
    ws.update(cx, |ws, cx| {
        let s = ws.studies.like.reading.as_ref().unwrap().seconds.clone();
        s.update(cx, |t, cx| t.set_text("20 s", cx));
    });
    click_on(cx, "like-reread", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.opened_picture().is_some_and(|b| b.seconds == Some(20.0))
    });
    // Create a song like it: the choices kept with it, the agent asked, and no notes written.
    cx.run_until_parked();
    click_on(cx, "like-create", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("like-create-sheet").is_some());
    click_on(
        cx,
        "like-rhythm-tight",
        Modifiers::none(),
        MouseButton::Left,
    );
    ws.update(cx, |ws, cx| {
        let style = ws.studies.like.create.as_ref().unwrap().style.clone();
        style.update(cx, |t, cx| t.set_text("dark half-time", cx));
    });
    click_on(cx, "like-create-go", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| ws.session.project.like.is_some());
    let (dir, p) = ws.read_with(cx, |ws, _| {
        (ws.session.dir.clone(), ws.session.project.clone())
    });
    assert!(
        dir.starts_with(&songs),
        "made in the tests' songs, not the person's"
    );
    let like = p.like.unwrap();
    assert_eq!(
        (like.source, like.rhythm, like.form, like.style.as_str()),
        (
            Source::Image,
            Closeness::Tight,
            Closeness::Close,
            "dark half-time"
        )
    );
    assert_eq!(like.calibration.unwrap().seconds, Some(20.0));
    assert!(
        p.tracks.iter().all(|t| t.regions.is_empty()),
        "Pina writes no notes: the agent writes the song"
    );
    until(cx, &ws, |ws| !asked(ws).is_empty());
    let prompt = ws.read_with(cx, |ws, _| asked(ws).pop().unwrap());
    assert!(prompt.contains("every note yourself") && prompt.contains("reference_compare"));
    assert!(prompt.contains("dark half-time") && prompt.contains("tightly"));
}

#[gpui::test]
fn a_song_is_compared_with_its_reference_and_a_revision_is_judged_and_stops(
    cx: &mut TestAppContext,
) {
    let studies = test_studies();
    let songs = test_songs();
    let wav = studies.join("Groove reference.wav");
    reference_wav(&wav);
    let (dir, _) = crate::cli::reference::create(
        &wav,
        &Calibration::default(),
        &crate::cli::reference::Choices::default(),
        &songs,
    )
    .unwrap();
    // What an agent might write: kicks alone, four bars of verse then four of chorus.
    let edit = |c: Value| crate::cli::edit(&dir, &c.to_string()).unwrap();
    edit(json!({"op": "add_track", "name": "Drums", "instrument": "drums"}));
    edit(
        json!({"op": "add_pattern_region", "track": 0, "start": 0, "length": 15_360,
        "rows": [{"pitch": 36, "cells": [110, 0, 0, 0, 110, 0, 0, 0, 110, 0, 0, 0, 110, 0, 0, 0]}]}),
    );
    edit(json!({"op": "add_marker", "tick": 0, "name": "Verse"}));
    edit(json!({"op": "add_marker", "tick": 7_680, "name": "Chorus"}));
    cx.executor().allow_parking();
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| {
        no_agent(ws);
        ws.compare_like(cx);
    });
    until(cx, &ws, |ws| ws.studies.like.compared.is_some());
    let report = ws.read_with(cx, |ws, _| {
        ws.studies.like.compared.as_ref().unwrap().report.clone()
    });
    assert!(!report.sections.is_empty());
    assert!(report.overall.groove.is_some() && report.overall.energy.is_some());
    // Noted in the agent's conversation too.
    assert!(ws.read_with(cx, |ws, _| {
        ws.chat
            .items
            .iter()
            .any(|i| matches!(i, crate::agent::chat::Item::Compared { .. }))
    }));
    // Another song made like the same reference has none of this one's comparison.
    let (other, _) = crate::cli::reference::create(
        &wav,
        &Calibration::default(),
        &crate::cli::reference::Choices::default(),
        &songs,
    )
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.session = Session::open(&other).unwrap();
        assert!(ws.song_compared().is_none());
        ws.revise_like("closer drums".into(), None, cx);
        assert!(
            ws.studies.like.revising.is_none(),
            "not on another song's comparison"
        );
        ws.session = Session::open(&dir).unwrap();
        assert!(ws.song_compared().is_some());
    });
    // On the Song Study page, the song opens on its reference, the song against it.
    ws.update(cx, |ws, cx| ws.open_study(cx));
    until(cx, &ws, |ws| {
        ws.opened_study().is_some_and(|s| s.file == wav)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("like-compare-card").is_some());
    // "Closer drums" (a chip on the card, below the window here): the agent asked to change
    // only what it aims at.
    assert!(cx.debug_bounds("like-quick-closer-drums").is_some());
    ws.update(cx, |ws, cx| ws.revise_like("closer drums".into(), None, cx));
    cx.run_until_parked();
    let (asked_now, waiting) = ws.read_with(cx, |ws, _| {
        (
            asked(ws).pop().unwrap_or_default(),
            ws.studies
                .like
                .revising
                .as_ref()
                .map(|r| (r.pass, r.waiting)),
        )
    });
    assert!(
        asked_now.contains("closer drums") && asked_now.contains("Change only"),
        "{asked_now}"
    );
    // No agent on hand: the revision stops rather than waiting for ever.
    assert!(
        waiting
            .is_some_and(|(pass, waiting)| pass > crate::pages::study::like_passes() && !waiting)
    );
    // With one (its turn over once it's written the hats), the pass is judged: better, or done.
    ws.update(cx, |ws, _| {
        let report = ws.studies.like.compared.as_ref().unwrap().report.clone();
        ws.studies.like.revising = Some(crate::pages::study::Revision {
            dir: ws.session.dir.clone(),
            request: "closer drums".into(),
            pass: 1,
            target: pina_engine::study::reference::compare::target("closer drums", &report),
            before: report,
            history: Vec::new(),
            waiting: true,
            stopped: None,
        });
    });
    edit(
        json!({"op": "set_pattern", "track": 0, "region": 0, "rows": [
        {"pitch": 36, "cells": [110, 0, 0, 0, 110, 0, 0, 0, 110, 0, 0, 0, 110, 0, 0, 0]},
        {"pitch": 42, "cells": [0, 0, 90, 0, 0, 0, 90, 0, 0, 0, 90, 0, 0, 0, 90, 0]}]}),
    );
    ws.update(cx, |ws, cx| {
        ws.session = Session::open(&dir).unwrap();
        ws.agent_turn_over(cx);
    });
    until(cx, &ws, |ws| {
        ws.studies
            .like
            .revising
            .as_ref()
            .is_some_and(|r| !r.history.is_empty())
    });
    let p = ws.read_with(cx, |ws, _| {
        ws.studies.like.revising.as_ref().unwrap().history[0].clone()
    });
    assert!(p.after > p.before, "{p:?}");
}

#[test]
fn agents_study_set_up_and_compare_a_reference_through_their_tools() {
    let studies = test_studies();
    let songs = test_songs();
    let png = studies.join("Tool plot.png");
    plot_png(&png);
    let studied = crate::agent::mcp::call(
        "reference_study",
        &json!({"file": png, "seconds": 20, "low_hz": 50, "high_hz": 10000, "scale": "log"}),
    )
    .unwrap();
    assert_eq!(studied["source"], "image");
    assert_eq!(studied["needs"].as_array().unwrap().len(), 0);
    assert!(
        studied["facts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["what"] == "Chords" && f["how"] == "unknown")
    );
    let made = crate::agent::mcp::call(
        "reference_create",
        &json!({"file": png, "folder": songs, "title": "Tool song", "sound": "loose",
            "seconds": 20, "low_hz": 50, "high_hz": 10000, "scale": "log"}),
    )
    .unwrap();
    let dir = PathBuf::from(made["project"].as_str().unwrap());
    assert!(
        made["next"]
            .as_str()
            .unwrap()
            .contains("every note yourself")
    );
    // Nothing written yet: there's nothing to compare.
    assert!(crate::agent::mcp::call("reference_compare", &json!({"project": dir})).is_err());
    let edit = |c: Value| crate::cli::edit(&dir, &c.to_string()).unwrap();
    edit(json!({"op": "add_track", "name": "Keys", "instrument": "synth"}));
    edit(
        json!({"op": "add_midi_region", "track": 0, "start": 0, "length": 76_800,
        "notes": [{"start": 0, "length": 76_800, "pitch": 84, "velocity": 90}]}),
    );
    let compared = crate::agent::mcp::call(
        "reference_compare",
        &json!({"project": dir, "request": "closer energy"}),
    )
    .unwrap();
    assert_eq!(compared["from"], "image");
    assert!(compared["relative"].as_bool().unwrap());
    assert!(compared["likeness"]["energy"].is_number());
    assert_eq!(compared["target"]["measures"][0], "energy");
    // Its full critique compares it with the reference every time, not only when asked.
    let heard = crate::agent::produce::critique(&dir, false, None).unwrap();
    assert!(heard["like_reference"]["energy"].is_number(), "{heard}");
    // A reading an agent got wrong is said, not dropped.
    let wrong = crate::agent::mcp::call(
        "reference_study",
        &json!({"file": png, "seconds": "twenty", "low_hz": 50}),
    );
    assert!(wrong.is_err_and(|e| e.contains("reading")));
}

#[test]
fn a_pictures_reading_comes_from_the_command_lines_flags() {
    use crate::cli::reference::calibration_from_args;
    let c = calibration_from_args(&[
        "--seconds",
        "180",
        "--hz",
        "20-22050",
        "--scale",
        "mel",
        "--colors",
        "magma",
        "--crop",
        "10,20,300,100",
        "--db",
        "80",
        "--offline",
    ])
    .unwrap();
    assert_eq!(
        c,
        Calibration {
            crop: Some([10, 20, 300, 100]),
            seconds: Some(180.0),
            low_hz: Some(20.0),
            high_hz: Some(22_050.0),
            scale: Some(FreqScale::Mel),
            colors: Some(Colors::Magma),
            db_range: Some(80.0),
        }
    );
    assert!(calibration_from_args(&["--crop", "10,20,300"]).is_err());
    assert!(calibration_from_args(&["--scale", "sideways"]).is_err());
    assert!(calibration_from_args(&["--hz", "20"]).is_err());
}
