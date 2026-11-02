//! The tempo: its lane (changes added, dragged and taken out), its menu in the top bar, and the
//! song keeping time with a take.

use pina_engine::song::project::{Content, Follow, Grid, TempoMode};

use super::*;
use crate::arrange::tempo::TempoOp;

#[gpui::test]
fn the_tempo_lane_adds_moves_and_takes_out_changes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    assert!(!ws.read_with(cx, |ws, _| ws.tempo_lane_shown()));
    cx.simulate_keystrokes("g");
    assert!(ws.read_with(cx, |ws, _| ws.tempo_lane_shown()));
    // Double-clicked high in the lane at bar 3: a faster tempo from there.
    let lane = cx.debug_bounds("tempo-lane").unwrap();
    let x = spot(&ws, cx, BAR * 2, 0).x;
    double_click(cx, point(x, lane.origin.y + px(8.)));
    let (tempo, changes) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.tempo, p.tempos.clone())
    });
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].tick, BAR * 2);
    assert!(changes[0].bpm > tempo, "{changes:?}");
    // Its point dragged a bar later and lower.
    let dot = cx.debug_bounds("tempo-point-1").unwrap().center();
    let bar = ws.read_with(cx, |ws, _| ws.px_per_tick() * BAR as f32);
    drag(cx, dot, dot + point(px(bar), px(4.)));
    let moved = ws.read_with(cx, |ws, _| ws.session.project.tempos.clone());
    assert_eq!(moved[0].tick, BAR * 3);
    assert!(moved[0].bpm < changes[0].bpm);
    // Undo puts it back; a double-click on it takes it out.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tempos[0].tick),
        BAR * 2
    );
    let dot = cx.debug_bounds("tempo-point-1").unwrap().center();
    double_click(cx, dot);
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tempos.is_empty()));
}

#[gpui::test]
fn a_click_on_the_tempo_opens_its_menu_and_a_drag_sets_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "tempo", Modifiers::none(), MouseButton::Left);
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().map(|m| m.items.len()));
    assert!(items.unwrap_or(0) >= 5);
    // Adapt.
    click_on(cx, "menu-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tempo_mode),
        TempoMode::Adapt
    );
    // Dragged up 40 px: 10 BPM faster.
    let before = ws.read_with(cx, |ws, _| ws.session.project.tempo);
    let at = cx.debug_bounds("tempo").unwrap().center();
    drag(cx, at, at - point(px(0.), px(40.)));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tempo),
        before + 10.0
    );
}

#[gpui::test]
fn the_song_keeps_time_with_a_take(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    // The drums' beats, as listening would find them: 88 BPM, a little late in bar 2.
    let rate = ws.read_with(cx, |ws, _| ws.session.project.sample_rate as f64);
    let beat = 60.0 / 88.0 * rate;
    let beats: Vec<u64> = (0..9)
        .map(|k| (k as f64 * beat + if (4..8).contains(&k) { 900.0 } else { 0.0 }) as u64)
        .collect();
    ws.update(cx, |ws, _| {
        if let Content::Audio { grid, .. } =
            &mut ws.session.project.tracks[drums].regions[0].content
        {
            *grid = Some(Grid {
                beats: beats.clone(),
                meter: 4,
                downbeat: 0,
            });
        }
    });
    ws.update(cx, |ws, cx| {
        ws.tempo_op(TempoOp::Adapt((drums, 0), false), cx)
    });
    let (tempo, changes, follow) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let follow = match &p.tracks[drums].regions[0].content {
            Content::Audio { follow, .. } => *follow,
            _ => unreachable!(),
        };
        (p.tempo, p.tempos.len(), follow)
    });
    assert!((tempo - 88.0).abs() < 1.5, "{tempo}");
    assert!(changes >= 1);
    assert_eq!(follow, Follow::Beats);
    // The lane shows, and undo takes it all back at once.
    assert!(ws.read_with(cx, |ws, _| ws.tempo_lane_shown()));
    cx.simulate_keystrokes("cmd-z");
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tempos.is_empty()));
}
