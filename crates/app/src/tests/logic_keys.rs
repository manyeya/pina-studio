//! Logic Pro's keys, in the piano roll and the arrangement: they do here what they do there.

use super::*;
use pina_engine::song::project::{Cycle, Note};

fn roll_notes(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Vec<Note> {
    ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[0].regions[0].content {
            Content::Midi { notes, .. } => notes.clone(),
            _ => unreachable!(),
        }
    })
}

fn picked(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Vec<usize> {
    ws.read_with(cx, |ws, _| {
        let mut p = ws.picked.clone();
        p.sort_unstable();
        p
    })
}

#[gpui::test]
fn the_piano_roll_takes_logics_keys(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    ws.update(cx, |ws, _| ws.roll.in_key = false);
    let before = roll_notes(&ws, cx);
    assert!(before.len() > 3);
    let mut order: Vec<usize> = (0..before.len()).collect();
    order.sort_by_key(|&i| (before[i].start, before[i].pitch));

    // ← → walk the notes in time; ⇧ adds the next, and going back lets it go.
    cx.simulate_keystrokes("right");
    assert_eq!(picked(&ws, cx), [order[0]]);
    cx.simulate_keystrokes("right");
    assert_eq!(picked(&ws, cx), [order[1]]);
    cx.simulate_keystrokes("shift-right");
    let mut two = vec![order[1], order[2]];
    two.sort_unstable();
    assert_eq!(picked(&ws, cx), two);
    cx.simulate_keystrokes("shift-left");
    assert_eq!(picked(&ws, cx), [order[1]]);
    cx.simulate_keystrokes("left");
    let i = order[0];
    assert_eq!(picked(&ws, cx), [i]);

    // ⌥→ moves it by the nudge value: a grid step, then (⌃⌥B) a beat.
    let grid = ws.read_with(cx, |ws, _| ws.grid()) as i64;
    cx.simulate_keystrokes("alt-right");
    assert_eq!(roll_notes(&ws, cx)[i].start, before[i].start + grid);
    cx.simulate_keystrokes("ctrl-alt-b alt-right ctrl-alt-d");
    assert_eq!(
        roll_notes(&ws, cx)[i].start,
        before[i].start + grid + PPQ as i64
    );
    // ⌥⇧→ longer, ⌥↑ a semitone up, = louder, ⌃M muted; each undoes.
    cx.simulate_keystrokes("alt-shift-right alt-up = ctrl-m");
    let n = &roll_notes(&ws, cx)[i];
    assert_eq!(n.length, before[i].length + grid as u64);
    assert_eq!(n.pitch, before[i].pitch + 1);
    assert_eq!(n.velocity, (before[i].velocity + 1).min(127));
    assert!(n.muted);
    cx.simulate_keystrokes("cmd-z cmd-z cmd-z cmd-z cmd-z cmd-z");
    assert_eq!(roll_notes(&ws, cx), before);

    // ⇧E: every note of its pitch; ⇧I the others; ⇧D none.
    cx.simulate_keystrokes("shift-e");
    let same: Vec<usize> = (0..before.len())
        .filter(|&k| before[k].pitch == before[i].pitch)
        .collect();
    assert_eq!(picked(&ws, cx), same);
    cx.simulate_keystrokes("shift-i");
    assert_eq!(picked(&ws, cx).len(), before.len() - same.len());
    cx.simulate_keystrokes("shift-d");
    assert!(picked(&ws, cx).is_empty());

    // ⌘T cuts the note at the playhead; ⌘J joins the halves again.
    let start = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions[0].start);
    let middle = start as i64 + before[i].start + before[i].length as i64 / 2;
    ws.update(cx, |ws, cx| {
        ws.locate_tick(middle as u64, cx);
        ws.picked = vec![i];
    });
    cx.simulate_keystrokes("cmd-t");
    let cut = roll_notes(&ws, cx);
    assert_eq!(cut.len(), before.len() + 1);
    assert_eq!(cut[i].start + cut[i].length as i64, middle - start as i64);
    cx.simulate_keystrokes("cmd-j");
    assert_eq!(roll_notes(&ws, cx).len(), before.len());

    // ⌥D takes out a note doubled on top of another.
    ws.update(cx, |ws, cx| {
        let doubled = before[i].clone();
        ws.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut((0, 0)) {
                notes.push(doubled);
            }
        });
        ws.picked.clear();
    });
    assert_eq!(roll_notes(&ws, cx).len(), before.len() + 1);
    cx.simulate_keystrokes("alt-d");
    assert_eq!(roll_notes(&ws, cx).len(), before.len());
}

#[gpui::test]
fn a_click_and_tab_say_whether_the_keys_are_the_tracks_or_the_editors(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    // The roll just opened: its frame lit, the arrows walk its notes.
    assert!(cx.debug_bounds("editor-keys-true").is_some());
    cx.simulate_keystrokes("right");
    assert_eq!(picked(&ws, cx).len(), 1);
    // A drum region clicked: the keys go to the tracks, → picks the next region.
    cx.simulate_click(spot(&ws, cx, PPQ, 2), Modifiers::none());
    assert!(ws.read_with(cx, |ws, _| ws.tracks_keys));
    assert!(cx.debug_bounds("editor-keys-false").is_some());
    cx.simulate_keystrokes("right");
    assert_eq!(ws.read_with(cx, |ws, _| ws.region), Some((2, 1)));
    // ⇧→ adds the one after it; Tab gives the roll the keys back.
    cx.simulate_keystrokes("shift-right");
    assert_eq!(ws.read_with(cx, |ws, _| ws.selected_regions().len()), 2);
    cx.simulate_keystrokes("tab");
    assert!(!ws.read_with(cx, |ws, _| ws.tracks_keys));
}

#[gpui::test]
fn the_tracks_area_takes_logics_keys(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let drums = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            (ws.session.project.tracks[2].regions.iter())
                .map(|r| (r.start, r.length))
                .collect::<Vec<_>>()
        })
    };
    let before = drums(cx);
    assert_eq!(before.len(), 4);
    // The second drum region; ⇧F picks it and every region after it, on every track.
    cx.simulate_click(spot(&ws, cx, before[1].0 + PPQ, 2), Modifiers::none());
    cx.simulate_keystrokes("shift-f");
    let following = ws.read_with(cx, |ws, _| ws.selected_regions().len());
    assert_eq!(following, 3);
    // ⌥⇧→ makes them a grid step longer, overlapping the next; \ takes the overlaps off.
    let grid = ws.read_with(cx, |ws, _| ws.grid().max(PPQ / 16));
    cx.simulate_keystrokes("alt-shift-right");
    assert_eq!(drums(cx)[1].1, before[1].1 + grid);
    cx.simulate_keystrokes("\\");
    assert_eq!(drums(cx)[1].1, before[1].1);
    // ⇧D lets go of them all.
    cx.simulate_keystrokes("shift-d");
    assert!(ws.read_with(cx, |ws, _| ws.selected_regions().is_empty()));

    // Between the locators: ⌃⌘X cuts a bar out of the song, ⌃⌘I puts it back as silence.
    ws.update(cx, |ws, cx| {
        ws.session.project.cycle = Some(Cycle {
            start: BAR,
            end: BAR * 2,
            off: false,
        });
        cx.notify();
    });
    cx.simulate_keystrokes("ctrl-cmd-x");
    assert_eq!(drums(cx).len(), 3);
    assert_eq!(drums(cx)[1].0, BAR);
    cx.simulate_keystrokes("ctrl-cmd-i");
    assert_eq!(drums(cx)[1].0, BAR * 2);
    // ⇧⌘. moves the cycle on by its length; ⌃⇧. goes to its end.
    cx.simulate_keystrokes("shift-cmd-.");
    let cycle = ws.read_with(cx, |ws, _| ws.session.project.cycle.unwrap());
    assert_eq!((cycle.start, cycle.end), (BAR * 2, BAR * 3));
    cx.simulate_keystrokes("ctrl-shift-.");
    assert_eq!(ws.read_with(cx, |ws, _| ws.playhead()), BAR * 3);

    // ⌘D: a track with the selected one's settings, empty; ⌥⇧⌘D: with its regions.
    ws.update(cx, |ws, _| ws.select_track(2));
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    cx.simulate_keystrokes("cmd-d");
    let (n, empty) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.tracks.len(), p.tracks[3].regions.is_empty())
    });
    assert_eq!((n, empty), (tracks + 1, true));
    ws.update(cx, |ws, _| ws.select_track(2));
    cx.simulate_keystrokes("alt-shift-cmd-d");
    let copied = ws.read_with(cx, |ws, _| ws.session.project.tracks[3].regions.len());
    assert_eq!(copied, drums(cx).len());
    // ⌥⌘S: a software instrument track.
    cx.simulate_keystrokes("alt-cmd-s");
    let last = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks[ws.track].instrument.is_some()
    });
    assert!(last);
}

#[gpui::test]
fn logics_clicks_with_keys_held(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let muted = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            (ws.session.project.tracks.iter())
                .map(|t| t.mute)
                .collect::<Vec<_>>()
        })
    };
    // ⌘-click Mute: every track; ⌥-click: this one alone.
    click_on(cx, "mute-0", Modifiers::command(), MouseButton::Left);
    assert!(muted(cx).iter().all(|&m| m));
    click_on(cx, "mute-1", Modifiers::alt(), MouseButton::Left);
    assert_eq!(muted(cx), [false, false, false]);
    click_on(cx, "mute-1", Modifiers::alt(), MouseButton::Left);
    assert_eq!(muted(cx), [false, true, false]);
    // ⌥-click Solo: this one soloed alone.
    click_on(cx, "solo-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "solo-2", Modifiers::alt(), MouseButton::Left);
    let solo = ws.read_with(cx, |ws, _| {
        (ws.session.project.tracks.iter())
            .map(|t| t.solo)
            .collect::<Vec<_>>()
    });
    assert_eq!(solo, [false, false, true]);
    // ⇧-click an empty stretch of a lane: the playhead there.
    let end = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions[0].length);
    cx.simulate_click(spot(&ws, cx, end + BAR, 0), Modifiers::shift());
    assert_eq!(ws.read_with(cx, |ws, _| ws.playhead()), end + BAR);
    // ⇧-double-click a region: every region on its track.
    let at = spot(&ws, cx, PPQ, 2);
    cx.simulate_click(at, Modifiers::none());
    let (modifiers, button) = (Modifiers::shift(), MouseButton::Left);
    cx.simulate_event(MouseDownEvent {
        position: at,
        modifiers,
        button,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: at,
        modifiers,
        button,
        click_count: 2,
    });
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.selected_regions().len()), 4);
}
