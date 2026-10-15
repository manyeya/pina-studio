# tempo commands

A project is a folder with `project.json` (read it with `tempo show <project>`). Edit it with one
command at a time:

    tempo do <project> '<command as JSON>'

If the project is open in the tempo app, the command goes to the app: it's applied live, can be
undone there, and is saved when the app saves (`tempo save <project>`). Otherwise the folder is
edited directly (and created, if it doesn't exist).

Time is in ticks: 960 to a quarter note, so in 4/4 bar `b` beat `k` starts at
`(b - 1) * 3840 + (k - 1) * 960`. Tracks, regions, sends and auxes are numbered from 0, in the
order `tempo show` lists them. Levels are in dB (0 is unity, -144 silent); pan is -1 (left) to 1
(right). A note is `{"start": ticks from the region's start, "length": ticks, "pitch": 0-127,
"velocity": 1-127}`; middle C is 60. An instrument is `"synth"` (the built-in sine) or
`{"vst3": {"path": "/path/to/Plugin.vst3", "class": "Plugin Name"}}` (see `tempo plugins`).

Each command answers with JSON: what it made (`{"track": 2}`, `{"region": [2, 0]}`,
`{"bus": 1}`), `{}`, or an error saying what's wrong.

## Commands

- `set_tempo` `{"bpm": 96}`
- `set_master` `{"volume_db": -3}`
- `add_track` `{"name": "Bass", "instrument": "synth", "index": 1}`: without `instrument` it's an
  audio track; `index` defaults to the end.
- `delete_track` `{"track": 1}`
- `set_track` `{"track": 1, "name": "Sub", "volume_db": -6, "pan": -0.3, "mute": false, "solo": false}`:
  any of them.
- `set_instrument` `{"track": 1, "instrument": {"vst3": {"path": "…", "class": "…"}}}`
- `set_insert` `{"channel": {"track": 1}, "index": 0, "plugin": {"path": "…", "class": "…"}}`:
  an effect into an insert slot (`{"aux": 0}` for an aux). An index past the last adds one;
  leaving out `plugin` removes the one there.
- `add_midi_region` `{"track": 0, "start": 0, "length": 3840, "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}]}`
- `set_notes` `{"track": 0, "region": 0, "notes": […]}`: replaces all of a region's notes.
- `add_audio_region` `{"track": 2, "file": "/path/take.wav", "start": 0, "offset": 0, "length": 7680}`:
  a WAV at the project's sample rate, copied into `Audio Files`; `offset` (samples into the file)
  and `length` (ticks; the rest of the file by default) are optional.
- `move_region` `{"track": 0, "region": 0, "start": 3840, "to_track": 1}`: MIDI regions go on
  instrument tracks, audio regions on audio tracks.
- `trim_region` `{"track": 0, "region": 0, "start": 960, "end": 7680}`: either edge; content
  stays where it is on the timeline.
- `split_region` `{"track": 0, "region": 0, "at": 1920}`: answers with the second half.
- `delete_region` `{"track": 0, "region": 1}`
- `comp` `{"track": 0, "region": 0, "from": 0, "to": 1920, "take": 1}`: in a take folder (a
  region with `"takes"`, made by recording over a region), take 1 plays for that range (ticks
  from the region's start). Takes are numbered from 0, oldest first; with no comp the newest plays.
- `flatten` `{"track": 0, "region": 0}`: replaces a take folder with what its comp plays: one
  MIDI region, or an audio region per piece.
- `add_send` `{"track": 0, "bus": 1, "level_db": -6}`: post-fader. Without `bus`, makes a new aux
  and sends to it.
- `set_send` `{"track": 0, "send": 0, "level_db": -12}`
- `remove_send` `{"track": 0, "send": 0}`
- `set_aux` `{"aux": 0, "name": "Reverb", "volume_db": -3, "pan": 0, "mute": false}`
- `set_automation` `{"track": 0, "param": "volume", "points": [{"tick": 0, "value": -12}, {"tick": 3840, "value": 0}]}`:
  `volume` in dB or `pan`; values change in straight lines between points. No points turns it off.

## Listening

An agent can't hear, so tempo measures:

    tempo analyze <project>          the mix, then each track alone
    tempo analyze <file.wav>

Each gives the length, peak (dBFS) and RMS per channel, integrated loudness (LUFS, as
ITU-R BS.1770), clipped samples, and how the energy splits between lows (< 250 Hz), mids and
highs (> 4 kHz), in dB below the total. `tempo render <project> <out.wav>` bounces it to listen to.

If the app is running: `tempo play`, `tempo stop`, `tempo locate <project> <tick>`,
`tempo save`, `tempo undo` and `tempo redo` (each with the project) drive it.
