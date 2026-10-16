# tempo commands

A project is a folder with `project.json` (read it with `tempo show <project>`). Edit it with one
command at a time:

    tempo do <project> '<command as JSON>'

If the project is open in the tempo app, the command goes to the app: it's applied live, can be
undone there, and is saved when the app saves (`tempo save <project>`). Otherwise the folder is
edited directly (and created, if it doesn't exist).

Time is in ticks: 960 to a quarter note, so in 4/4 bar `b` beat `k` starts at
`(b - 1) * 3840 + (k - 1) * 960` (in 3/4 a bar is 2880; in 6/8, 2880 with 480 to a beat). Tracks, regions, sends and auxes are numbered from 0, in the
order `tempo show` lists them. Levels are in dB (0 is unity, -144 silent); pan is -1 (left) to 1
(right). A note is `{"start": ticks from the region's start, "length": ticks, "pitch": 0-127,
"velocity": 1-127}`; middle C is 60. An instrument is `"synth"` (the built-in sine) or
`"drums"` (the built-in kit, on the General MIDI drum notes) or
`{"vst3": {"path": "/path/to/Plugin.vst3", "class": "Plugin Name"}}` (see `tempo plugins`) or
the built-in sampler: `{"sampler": {"zones": [{"file": "/path/Kick.wav", "name": "Kick", "note": 36}]}}`.
A zone is a sample on a note (a drum kit has one each), or with `"high"` a range of notes it's
played across, repitched from `"root"` (the note it sounds at: an 808 or a bass). Drum hits play
to their end; `"gate": true` stops it with its note (fading over `"release_ms"`). Zones with the
same `"choke"` group cut each other off, as a closed hat cuts an open one. `"gain_db"` and
`"pan"` set its level, `"tune_cents"` its fine tuning. Samples are copied into the project, and
any WAV works: other sample rates are converted.

Each command answers with JSON: what it made (`{"track": 2}`, `{"region": [2, 0]}`,
`{"bus": 1}`), `{}`, or an error saying what's wrong.

## Commands

- `set_tempo` `{"bpm": 96}`
- `set_master` `{"volume_db": -3}`
- `set_time_signature` `{"beats": 6, "unit": 8}`: beats to a bar, and the note that's a beat (1,
  2, 4, 8 or 16). The tempo stays in quarter notes a minute.
- `add_track` `{"name": "Bass", "instrument": "synth", "index": 1}`: without `instrument` it's an
  audio track; `index` defaults to the end.
- `delete_track` `{"track": 1}`
- `set_track` `{"track": 1, "name": "Sub", "volume_db": -6, "pan": -0.3, "mute": false, "solo": false}`:
  any of them.
- `set_instrument` `{"track": 1, "instrument": {"vst3": {"path": "…", "class": "…"}}}`
- `set_insert` `{"channel": {"track": 1}, "index": 0, "plugin": {"path": "…", "class": "…"}}`:
  an effect into an insert slot (`{"aux": 0}` for an aux, `"master"` for the master, after its
  built-in effects). An index past the last adds one; leaving out `plugin` removes the one there.
  A slot's automation goes with its plugin.
- `set_plugin_param` `{"channel": {"track": 1}, "plugin": "instrument", "param": 12, "value": 0.35}`:
  turns a plugin's parameter (0 to 1). `plugin` is `"instrument"` or `{"insert": 0}`; the ids
  and names are in `tempo params <project> <track>` (`--aux`, `--master`, `--insert N`). Without
  `value`, tempo stops setting it and it goes back to what it was.
- `add_midi_region` `{"track": 0, "start": 0, "length": 3840, "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}]}`
- `set_notes` `{"track": 0, "region": 0, "notes": […]}`: replaces all of a region's notes.
- `add_pattern_region` `{"track": 0, "start": 0, "length": 3840, "step": 240, "swing": 58, "rows": [{"pitch": 36, "cells": [110, 0, 0, 0, 100, 0, 0, 0, 110, 0, 0, 0, 100, 0, 0, 0]}]}`:
  a step-sequencer pattern: each row a note, each cell a step's velocity (0 rests), looping over
  the region. `step` is ticks per step (240, a sixteenth, by default); `swing` (50 straight, up
  to 75) makes every second step late, as an MPC does. On a `"drums"` track:
  36 kick, 38 snare, 37 side stick, 39 clap, 42 closed hat, 46 open hat, 41/45/48 toms, 49 crash.
- `set_pattern` `{"track": 0, "region": 0, "rows": […], "swing": 62}`: replaces a pattern's
  rows, or its swing.
- `add_audio_region` `{"track": 2, "file": "/path/take.wav", "start": 0, "offset": 0, "length": 7680}`:
  a WAV (at any sample rate: it's converted to the project's), copied into `Audio Files`; `offset`
  (samples into it, at the project's rate) and `length` (ticks; the rest of the file by default)
  are optional.
- `move_region` `{"track": 0, "region": 0, "start": 3840, "to_track": 1}`: MIDI regions go on
  instrument tracks, audio regions on audio tracks.
- `trim_region` `{"track": 0, "region": 0, "start": 960, "end": 7680}`: either edge; content
  stays where it is on the timeline.
- `split_region` `{"track": 0, "region": 0, "at": 1920}`: answers with the second half.
- `delete_region` `{"track": 0, "region": 1}`
- `set_region` `{"track": 0, "region": 0, "name": "Hook", "loop": true, "transpose": 12, "velocity": -10, "quantize": 240}`:
  any of them. A looped region plays over and over until the next region on its track (or the end
  of the song). `transpose` (semitones), `velocity` (added) and `quantize` (a grid in ticks; 0 is
  off) change how a MIDI or pattern region plays, not its notes, as Logic's region inspector does.
- `add_alias` `{"track": 0, "region": 0, "start": 7680, "to_track": 0}`: an alias of a MIDI or
  pattern region: a copy linked to it (both get the same `"link"` number), so editing the notes or
  steps of either edits both. Its place, length and playback settings are its own. Splitting it,
  or trimming its start, makes it a region of its own.
- `unlink` `{"track": 0, "region": 2}`: makes an alias a region of its own.
- `set_cell` `{"track": 1, "scene": 0, "region": {"length": 3840, "midi": {"notes": […]}}}`:
  puts a loop in a Live Loops cell (scenes numbered from 0); any region content works, `start`
  doesn't matter. Without `region`, empties the cell. Launch cells in the app: `tempo launch
  <project> <track> <scene>` starts a cell at the next bar (or now, starting playback), and
  `tempo launch <project> <track>` stops it, the track's own regions playing again.
- `flex_region` `{"track": 2, "region": 0, "length": 7680}`: Flex Time: stretches an audio region
  to that length, slower or faster but at the same pitch (up to four times either way).
- `comp` `{"track": 0, "region": 0, "from": 0, "to": 1920, "take": 1}`: in a take folder (a
  region with `"takes"`, made by recording over a region), take 1 plays for that range (ticks
  from the region's start). Takes are numbered from 0, oldest first; with no comp the newest plays.
- `flatten` `{"track": 0, "region": 0}`: replaces a take folder with what its comp plays: one
  MIDI region, or an audio region per piece.
- `add_send` `{"track": 0, "bus": 1, "level_db": -6, "pre": false}`: after the fader, or before
  it with `"pre": true`. Without `bus`, makes a new aux and sends to it.
- `set_send` `{"track": 0, "send": 0, "level_db": -12, "pre": true}`: either.
- `set_input` `{"track": 2, "input": {"channel": 0, "stereo": true}}`: what an audio track
  records: the input device's channel (from 0), and the next with `stereo`. Without `input`, the
  first two.
- `remove_send` `{"track": 0, "send": 0}`
- `set_output` `{"track": 0, "bus": 2}`: the track plays into that aux or stack instead of the
  master; without `bus`, back to the master.
- `add_stack` `{"tracks": [0, 1], "name": "Drums"}`: a summing stack, a channel (an aux, listed
  with the others) those tracks play into. Answers with its bus.
- `set_aux` `{"aux": 0, "name": "Reverb", "volume_db": -3, "pan": 0, "mute": false}`
- `set_automation` `{"track": 0, "param": "volume", "points": [{"tick": 0, "value": -12}, {"tick": 3840, "value": 0}]}`:
  `volume` in dB, `pan`, or `plugin` (with `"plugin": "instrument"` or `{"insert": 0}`, the
  parameter's `"id"`, and optionally its `"name"`) from 0 to 1; values change in straight lines
  between points. No points takes it out.
- `set_automation_mode` `{"track": 0, "mode": "latch"}`: `read` (the default) plays automation;
  `touch` and `latch` also record what's moved in the app while the song plays, until the control
  is let go (touch) or playback stops (latch); `off` ignores it.
- `set_eq` `{"channel": {"track": 1}, "eq": {"low_cut": 80, "low_mid": {"freq": 400, "gain_db": -2.5, "q": 1}, "high_mid": {"freq": 2500, "gain_db": 3.5, "q": 1}, "high_shelf": {"freq": 10000, "gain_db": 4}}}`:
  the channel's built-in EQ, before its inserts: a low cut (Hz; 20 or less is off), two bells and
  a high shelf. Leave out what you don't change; `"bypass": true` keeps it but stops it; no `eq`
  takes it out. `channel` is `{"track": i}`, `{"aux": i}` or `"master"`.
- `set_comp` `{"channel": {"track": 1}, "comp": {"threshold_db": -18, "ratio": 4, "attack_ms": 10, "release_ms": 120, "makeup_db": 3, "rms": false}}`:
  the built-in compressor, after the EQ: a 6 dB soft knee, both sides turned down together.
  `"rms": true` makes it listen to an RMS average over 15 ms rather than the peaks: gentler, more
  like the ear. Same rules as `set_eq`.
- `set_saturator` `{"channel": {"track": 1}, "sat": {"drive_db": 9, "tone": 0.2, "mix": 0.35, "output_db": -1}}`:
  the built-in saturator, after the compressor: `drive_db` 0-24 (quiet sounds keep their level,
  loud ones round off; oversampled and antialiased, so hard drive stays clean), `tone` -1 darker
  to 1 brighter, `mix` 0 dry to 1. Same rules as `set_eq`.
- `edit_notes` `{"track": 1, "region": 0, "edit": {"kind": "quantize", "grid": 240, "strength": 1, "swing": 50}}`:
  changes a MIDI region's notes in one go (or one note, with `"note": 3`). `kind` is `quantize`
  (`strength` 0-1, `swing` 50 straight to 75), `humanize` (`timing` ticks, `velocity`, `seed`),
  `legato`, `chop` (`grid`), `arpeggiate` (`grid`), `transpose` (`semitones`) or `velocity` (`by`).
- `add_marker` `{"tick": 15360, "name": "Chorus"}`: names a place in the song; answers with its
  index, markers being numbered in time order.
- `set_marker` `{"marker": 0, "name": "Verse 2", "tick": 7680}`, `delete_marker` `{"marker": 0}`
- `set_key` `{"key": "A minor", "move_notes": true}`: the song's key, which the piano roll shows
  as a scale and the score as its signature. With `"move_notes": true` the music moves into it:
  every note and pattern row on pitched instruments goes up or down to the new tonic (the nearer
  way), each step of the old scale landing on the same step of the new, so major to minor changes
  the mode; drums and audio stay. Without `key`, tempo works it out from the notes.
- `set_cycle` `{"start": 0, "end": 15360, "on": true}`: playback loops over that range (ticks);
  `"on": false` keeps the range without looping.

## Listening

An agent can't hear, so tempo measures:

    tempo analyze <project>          the mix, then each track alone
    tempo analyze <file.wav>

Each gives the length, peak (dBFS) and RMS per channel, integrated loudness (LUFS, as
ITU-R BS.1770), clipped samples, and how the energy splits between lows (< 250 Hz), mids and
highs (> 4 kHz), in dB below the total. `tempo render <project> <out.wav>` bounces it to listen to.

If the app is running: `tempo play`, `tempo stop`, `tempo locate <project> <tick>`,
`tempo save`, `tempo undo` and `tempo redo` (each with the project) drive it.
