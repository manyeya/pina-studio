//! The tempo map, and audio keeping time with it.

use super::*;

/// The song at 120, then 60 from bar 2 (tick 3840).
fn slowing() -> Project {
    let mut p = Project::new(48_000);
    p.tempo = 120.0;
    p.set_tempo_at(PPQ * 4, 60.0);
    p
}

#[test]
fn ticks_and_seconds_go_through_the_tempo_changes() {
    let p = slowing();
    // A bar at 120 is 2 s; each beat after it at 60 is 1 s.
    assert_eq!(p.seconds_at((PPQ * 4) as f64), 2.0);
    assert_eq!(p.seconds_at((PPQ * 6) as f64), 4.0);
    assert_eq!(p.samples(PPQ * 6, 48_000.0), 192_000);
    assert_eq!(p.ticks(192_000), PPQ * 6);
    assert_eq!(p.span(PPQ * 4, PPQ * 5, 48_000.0), 48_000);
    assert_eq!((p.tempo_at(0), p.tempo_at(PPQ * 4)), (120.0, 60.0));
    // Before the start, at the first tempo.
    assert_eq!(p.seconds_at(-(PPQ as f64)), -0.5);
    // The audio thread's map agrees: the beat at a sample, the sample of a beat.
    let map = TempoMap::new(&p, 48_000.0);
    assert_eq!(map.beats(144_000), 5.0);
    assert_eq!(map.sample(5.0), 144_000.0);
    assert_eq!(map.tempo(144_000), 60.0);
    // The whole song faster: its changes in proportion.
    let mut q = p.clone();
    q.set_tempo(240.0);
    assert_eq!(q.tempos[0].bpm, 120.0);
}

#[test]
fn changes_stay_tidy() {
    let mut p = slowing();
    // The same tempo again isn't a change; a change at the start is the song's tempo.
    p.set_tempo_at(PPQ * 8, 60.0);
    assert_eq!(p.tempos.len(), 1);
    p.set_tempo_at(0, 100.0);
    assert_eq!((p.tempo, p.tempos.len()), (100.0, 1));
    p.move_tempo(PPQ * 4, PPQ * 2);
    assert_eq!(p.tempos[0].tick, PPQ * 2);
    p.delete_tempo(PPQ * 2);
    assert!(p.tempos.is_empty());
}

/// Audio content: a file at its `tempo`, following as `follow` says.
fn track_of(follow: Follow, tempo: Option<f64>) -> Region {
    let mut r = audio(0, PPQ * 8, 0);
    if let Content::Audio {
        follow: f,
        tempo: t,
        ..
    } = &mut r.content
    {
        (*f, *t) = (follow, tempo);
    }
    r
}

#[test]
fn following_audio_keeps_its_beats_on_the_songs() {
    let p = slowing();
    let follows = track_of(Follow::On, Some(90.0));
    let t = Timing::of(&follows.content, 48_000.0).unwrap();
    // Its third beat (two beats in, at 90: 64000 frames) plays on the song's third beat, and
    // its sixth on the song's sixth, after the tempo has dropped.
    assert!((p.audio_frame(&t, 0.0, (PPQ * 2) as f64) - 64_000.0).abs() < 1e-6);
    assert!((p.audio_frame(&t, 0.0, (PPQ * 5) as f64) - 160_000.0).abs() < 1e-6);
    // Played as recorded it doesn't care: 3 s in (beat 5 here) is 3 s of the file.
    let plain = track_of(Follow::Off, None);
    let t = Timing::of(&plain.content, 48_000.0).unwrap();
    assert!((p.audio_frame(&t, 0.0, (PPQ * 5) as f64) - 144_000.0).abs() < 1e-6);
    // For the stretcher: a corner where the tempo changes, in the file's frame and in the song.
    let AudioFit { warps, zero, .. } = p
        .audio_fit(&follows.content, 0.0, 480_000, 48_000.0)
        .unwrap();
    assert_eq!(zero, 0.0);
    assert!(warps.contains(&(128_000, 96_000)), "{warps:?}");
}

#[test]
fn locked_to_its_beats_a_drifting_take_keeps_to_the_grid() {
    let p = Project::new(48_000);
    // Played at about 120, its fourth beat late by 2000 frames.
    let mut r = track_of(Follow::Beats, Some(120.0));
    if let Content::Audio { grid, .. } = &mut r.content {
        *grid = Some(Grid {
            beats: vec![0, 24_000, 48_000, 74_000, 96_000],
            meter: 4,
            downbeat: 0,
        });
    }
    let t = Timing::of(&r.content, 48_000.0).unwrap();
    // The late beat lands on the song's fourth beat all the same.
    assert!((p.audio_frame(&t, 0.0, (PPQ * 3) as f64) - 74_000.0).abs() < 1e-6);
}

#[test]
fn switching_follow_keeps_how_it_sounds_and_old_songs_upgrade() {
    let mut p = Project::new(48_000);
    p.tempo = 100.0;
    let mut r = audio(0, PPQ * 4, 0);
    if let Content::Audio { stretch, warps, .. } = &mut r.content {
        *stretch = 0.9;
        warps.push([1000, 900]);
    }
    p.tracks.push(Track {
        regions: vec![r],
        ..Default::default()
    });
    p.set_follow((0, 0), Follow::On);
    let Content::Audio { tempo, warps, .. } = &p.tracks[0].regions[0].content else {
        unreachable!()
    };
    // Its own tempo is what the stretch made it at 100; its warp in its own time.
    assert_eq!((*tempo, warps[0]), (Some(90.0), [1000, 1000]));
    p.set_follow((0, 0), Follow::Off);
    let Content::Audio { stretch, warps, .. } = &p.tracks[0].regions[0].content else {
        unreachable!()
    };
    assert_eq!((*stretch, warps[0]), (0.9, [1000, 900]));
    // A song saved before: following was true, its stretch for the song's tempo.
    let json = r#"{"tempo": 100, "sample_rate": 48000, "tracks": [{"name": "t", "regions": [
        {"start": 0, "length": 3840, "audio": {"file": "a.wav",
         "stretch": 0.9, "follow": true, "warps": [[1000, 900]]}}]}]}"#;
    let mut old: Project = serde_json::from_str(json).unwrap();
    old.upgrade_audio();
    let Content::Audio {
        follow,
        tempo,
        warps,
        ..
    } = &old.tracks[0].regions[0].content
    else {
        unreachable!()
    };
    assert_eq!(
        (*follow, *tempo, warps[0]),
        (Follow::On, Some(90.0), [1000, 1000])
    );
}

#[test]
fn locking_and_letting_go_gives_back_the_audio_it_had() {
    // A take at 107, its tempo found as half that (53.5), in a song at 107.
    let mut p = Project::new(48_000);
    p.tempo = 107.0;
    let mut r = track_of(Follow::Off, Some(53.5));
    if let Content::Audio { warps, grid, .. } = &mut r.content {
        warps.push([24_000, 24_000]);
        *grid = Some(Grid {
            beats: (0..16).map(|k| k * 53_832).collect(),
            meter: 4,
            downbeat: 0,
        });
    }
    let (start, length) = (r.start, r.length);
    p.tracks.push(Track {
        regions: vec![r.clone()],
        ..Default::default()
    });
    p.set_follow((0, 0), Follow::Bars);
    // Locked to that it plays twice as fast: half as long, all of it still in the region.
    let shorter = p.tracks[0].regions[0].length;
    assert!(
        (shorter as f64 / length as f64 - 0.5).abs() < 1e-3,
        "{shorter}"
    );
    p.set_follow((0, 0), Follow::Off);
    let back = &p.tracks[0].regions[0];
    assert_eq!((back.start, back.length), (start, length));
    assert_eq!(back.content, r.content);
}

#[test]
fn following_a_loop_keeps_whole_beats_and_as_recorded_is_as_recorded() {
    // Two bars at 95 played as recorded at 120: 5.05 s, longer than two bars.
    let mut p = Project::new(48_000);
    p.tempo = 120.0;
    let mut r = track_of(Follow::Off, Some(95.0));
    r.length = p.ticks_for(0, 8 * 48_000 * 60 / 95);
    p.tracks.push(Track {
        regions: vec![r],
        ..Default::default()
    });
    p.set_follow((0, 0), Follow::On);
    assert_eq!(p.tracks[0].regions[0].length, PPQ * 8);
    // Stretched by hand, then back as it was recorded.
    p.set_follow((0, 0), Follow::Off);
    p.time_stretch((0, 0), PPQ * 6).unwrap();
    p.unstretch((0, 0));
    let r = &p.tracks[0].regions[0];
    let Content::Audio {
        stretch, follow, ..
    } = &r.content
    else {
        unreachable!()
    };
    assert_eq!((*stretch, *follow), (1.0, Follow::Off));
    assert_eq!(r.length, p.ticks_for(0, 8 * 48_000 * 60 / 95));
}

#[test]
fn stretching_by_hand_through_a_tempo_change_fills_the_length_asked() {
    // Two seconds at 120 into a song that drops to 60 at bar 2: stretched to end at bar 3, it
    // lasts the 2 s to bar 2 and 4 s more.
    let mut p = slowing();
    p.tracks.push(Track {
        regions: vec![track_of(Follow::Off, None)],
        ..Default::default()
    });
    p.tracks[0].regions[0].length = PPQ * 4;
    p.time_stretch((0, 0), PPQ * 8).unwrap();
    let r = &p.tracks[0].regions[0];
    let Content::Audio { stretch, .. } = &r.content else {
        unreachable!()
    };
    assert!((stretch - 3.0).abs() < 1e-9, "{stretch}");
    assert_eq!(r.length, PPQ * 8);
}

/// A take at about 100 BPM that drifts: each beat's frame, at 48 kHz.
fn drifting() -> Vec<u64> {
    let mut beats = Vec::new();
    let mut at = 0.0f64;
    for k in 0..17 {
        beats.push(at.round() as u64);
        // 100 BPM, speeding up a little in its second bar and slowing in its third.
        let bpm = match k / 4 {
            1 => 104.0,
            2 => 97.0,
            _ => 100.0,
        };
        at += 60.0 / bpm * 48_000.0;
    }
    beats
}

fn take_on_track(p: &mut Project, beats: Vec<u64>) {
    let mut r = audio(0, PPQ * 16, 0);
    if let Content::Audio { grid, .. } = &mut r.content {
        *grid = Some(Grid {
            beats,
            meter: 4,
            downbeat: 0,
        });
    }
    p.tracks.push(Track {
        regions: vec![r],
        ..Default::default()
    });
}

#[test]
fn the_song_adapts_to_a_take_bar_by_bar() {
    let mut p = Project::new(48_000);
    let beats = drifting();
    take_on_track(&mut p, beats.clone());
    p.adapt_to((0, 0), false, true).unwrap();
    // Each of the take's beats plays on one of the song's.
    let r = p.region((0, 0)).clone();
    let t = Timing::of(&r.content, 48_000.0).unwrap();
    for (k, &b) in beats.iter().enumerate() {
        let frame = p.audio_frame(&t, r.start as f64, (k as u64 * PPQ) as f64);
        assert!((frame - b as f64).abs() < 2.0, "beat {k}: {frame} for {b}");
    }
    // The song's tempo is the take's, bar by bar: faster in the second, slower in the third.
    assert!(
        (p.tempo_at(PPQ * 4) - 104.0).abs() < 0.1,
        "{}",
        p.tempo_at(PPQ * 4)
    );
    assert!((p.tempo_at(PPQ * 8) - 97.0).abs() < 0.1);
    // And it's played as recorded: its seconds are the take's.
    assert!((p.seconds_at((PPQ * 16) as f64) - beats[16] as f64 / 48_000.0).abs() < 1e-3);
}

#[test]
fn the_song_takes_a_steady_loops_tempo() {
    let mut p = Project::new(48_000);
    let beats: Vec<u64> = (0..9).map(|k| k * 32_000).collect(); // 90 BPM
    take_on_track(&mut p, beats);
    p.adapt_to((0, 0), true, true).unwrap();
    assert!(
        (p.tempo - 90.0).abs() < 1e-6 && p.tempos.is_empty(),
        "{}",
        p.tempo
    );
    assert!(matches!(
        p.region((0, 0)).content,
        Content::Audio {
            follow: Follow::On,
            ..
        }
    ));
}

#[test]
fn the_tempo_fits_a_region_into_the_cycle() {
    let mut p = Project::new(48_000);
    // Two seconds of audio, played as recorded: a bar at 120.
    p.tracks.push(Track {
        regions: vec![audio(PPQ, PPQ * 4, 0)],
        ..Default::default()
    });
    // To fill two bars, the tempo doubles.
    let bpm = p.fit_tempo((0, 0), 0, PPQ * 8).unwrap();
    assert!((bpm - 240.0).abs() < 1e-6, "{bpm}");
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (0, PPQ * 8));
}

#[test]
fn a_click_tracks_tempo_is_found() {
    // A click at 100 BPM for 12 s.
    let rate = 48_000.0;
    let mut mono = vec![0f32; (12.0 * rate) as usize];
    let beat = (60.0 / 100.0 * rate) as usize;
    let n = mono.len();
    for start in (0..n).step_by(beat) {
        for (i, s) in mono[start..(start + 600).min(n)].iter_mut().enumerate() {
            *s = (i as f32 * 0.3).sin() * (-(i as f32) / 120.0).exp();
        }
    }
    let found = crate::study::timing::find(&[mono.clone(), mono], rate, rate, "click").unwrap();
    assert!((found.tempo - 100.0).abs() < 2.0, "{}", found.tempo);
    assert!(found.grid.beats.len() >= 16);
}
