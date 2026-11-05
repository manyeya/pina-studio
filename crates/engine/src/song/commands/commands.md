# Pina Studio commands

A project is a folder with `project.json` (read it with `pina show <project>`). Edit it with one
command at a time:

    pina do <project> '<command as JSON>'

If the project is open in the Pina Studio app, the command goes to the app: it's applied live, can be
undone there, and is saved when the app saves (`pina save <project>`). Otherwise the folder is
edited directly (and created, if it doesn't exist).

Time is in ticks: 960 to a quarter note, so in 4/4 bar `b` beat `k` starts at
`(b - 1) * 3840 + (k - 1) * 960` (in 3/4 a bar is 2880; in 6/8, 2880 with 480 to a beat). Tracks, regions, sends and auxes are numbered from 0, in the
order `pina show` lists them. Levels are in dB (0 is unity, -144 silent); pan is -1 (left) to 1
(right). A note is `{"start": ticks from the region's start, "length": ticks, "pitch": 0-127,
"velocity": 1-127}`; middle C is 60. A note may also have `"chance"` (1-100, the percent of times
it plays; 100 if left out), `"spread"` (0-127, how far its velocity strays either way each time)
and `"muted": true` (kept but not played). An instrument is `"synth"` (the built-in sine) or
`"drums"` (the built-in kit, on the General MIDI drum notes) or a plugin,
`{"plugin": {"format": "clap", "path": "/path/to/Plugin.clap", "class": "Plugin Name", "uid": "…"}}`,
exactly as `pina plugins` lists it (`format` is `vst3`, the default, `clap`, `vst2` or `au`;
an Audio Unit is known by its `uid`, like `"aumu samp appl"`, and may have no `path`), or
the built-in sampler: `{"sampler": {"zones": [{"file": "/path/Kick.wav", "name": "Kick", "note": 36}]}}`.
A zone is a sample on a note (a drum kit has one each), or with `"high"` a range of notes it's
played across, repitched from `"root"` (the note it sounds at: an 808 or a bass). `"from"` is the
library file its sample was brought in from (set by itself when `file` is outside the song), so
the Drum Machine can find what sounds like it. Drum hits play
to their end; `"gate": true` stops it with its note (fading over `"release_ms"`). Zones with the
same `"choke"` group cut each other off, as a closed hat cuts an open one. `"gain_db"` and
`"pan"` set its level, `"tune_cents"` its fine tuning. `"start"` and `"end"` (shares of the
sample, 0 to 1) play part of it, as a slice; `"reverse": true` backwards; `"loop_mode"` `forward`
or `ping_pong` loops it while held (between `"loop_start"` and `"loop_end"`, its play range by
default; `"loop_fade"` 0 to 0.5 of the loop crossfades its seam). `"velocities": [1, 63]` plays it
for those velocities only (layers; `"velocity_fade": 20` blends over 20 velocities either side);
zones on the same notes and velocities take turns (round robins), unless on different `"layer"`s
(`"layer": 1`), which play together, stacked; `"trigger": "release"` plays a zone as its note is
let go. How every voice plays is the sampler's `"voice"`:
`{"attack_ms": 5, "decay_ms": 300, "sustain": 0.6, "release_ms": 200, "filter": "low_pass",
"cutoff_hz": 2500, "resonance": 0.3, "drive": 0.2, "filter_env": 2, "filter_decay_ms": 400,
"key_track": 0.5, "velocity": 1, "mono": true, "legato": true, "glide_ms": 80, "lfo_hz": 5,
"vibrato_cents": 15, "lfo_filter": 0, "tremolo": 0, "transpose": 0, "gain_db": 0}` (any left out
stay as they are by default: no envelope shaping, no filter, polyphonic). `"filter"` is also
`notch`, `ladder` (24 dB, fat), `comb` (tuned to the cutoff) or `formant` (the cutoff sweeps the
vowels). `"mode"`: `repitch` (faster is higher), `stretch` (grains keep each note's pitch while
`"speed"` sets how fast it plays through: 0 holds still, under 0 backwards) or `grains` (a cloud:
`"grain_ms"`, `"density"` a second, `"spray"` 0-1, `"grain_pitch"` cents at random,
`"grain_reverse"` 0-1, `"grain_spread"` 0-1, `"grain_shape"` 0 smooth to 1 hard; `"position"`
0-1 where in the sample, `"freeze": true` holds it there). `"character"`: `clean`, `tape`
(`"color"` 0-1: warmth, saturation, wow) or `vintage` (`"crush_khz"` and `"bits"`, the old
samplers' grit: 26.04 and 12 for the classic drum machine, 40 and 12 for the classic beat machine;
`"crush_filter": true` their output filter). `"unison"` 1-8 voices, `"detune_cents"`, `"spread"`
0-1. Drum-machine extras: `"pitch_env"` semitones dropping over `"pitch_decay_ms"` (an 808's
knock), `"sub"` 0-1 (a sine an octave under), `"noise"` 0-1 with `"noise_color"` -1 dark to 1
bright, `"fm"` and `"ring"` 0-1 with `"osc_ratio"`, `"punch"` 0-1. `"lfo_wave"` (`sine`,
`triangle`, `saw_up`, `saw_down`, `square`, `random`, `drift`), `"lfo_beats"` (a cycle that many
beats long in time with the song; 0 uses `lfo_hz`), `"lfo_retrigger"`; a second LFO `"lfo2":
{"wave": "drift", "hz": 0.5, "beats": 0, "retrigger": false}`; a modulation envelope `"mod_env":
{"attack_ms": 0, "decay_ms": 500, "sustain": 0, "release_ms": 300}`; and up to 8 routes
`"matrix": [{"source": "velocity", "dest": "cutoff", "amount": 0.5}]` (sources `lfo1`, `lfo2`,
`mod_env`, `amp_env`, `filter_env`, `velocity`, `key`, `wheel`, `pressure`, `random`, `bend`;
dests `pitch` ±24 st, `cutoff` ±6 oct, `resonance`, `level`, `pan`, `start`, `position`, `size`,
`density`, `spray`, `speed`, `drive`, `fm`, `sub`, `noise`; amount -1 to 1). Samples are copied
into the project, and any WAV works: other sample rates are converted.

Each command answers with JSON: what it made (`{"track": 2}`, `{"region": [2, 0]}`,
`{"bus": 1}`), `{}`, or an error saying what's wrong.

## Commands

- `set_tempo` `{"bpm": 96}`: the song's tempo, its tempo changes scaled along (the whole song
  faster or slower). Audio regions that follow the tempo (`"follow"`) keep their beats on the
  song's.
- `set_tempo_at` `{"tick": 15360, "bpm": 90}`: the tempo from that tick on (0 is the song's own),
  until the next change: a song that slows for its bridge. Everything after it moves in time, not
  in bars and beats. The project's `"tempos"` lists the changes.
- `delete_tempo` `{"tick": 15360}`: takes the tempo change at that tick out.
- `set_tempo_mode` `{"mode": "keep"}`: what audio recorded or brought into the app does to the
  tempo: `"keep"` (the tempo stays; loops follow it), `"adapt"` (the song takes the audio's
  tempo, bar by bar) or `"auto"` (the default: adapts only where nothing else in the song keeps
  time yet).
- `set_master` `{"volume_db": -3, "pan_law": "minus3"}`: either. The pan law is how loud a sound
  panned to the middle is against one hard to a side, for every track: `"flat"` (the default),
  `"minus3"`, `"minus3_compensated"` or `"minus6"`.
- `set_time_signature` `{"beats": 6, "unit": 8}`: beats to a bar, and the note that's a beat (1,
  2, 4, 8 or 16). The tempo stays in quarter notes a minute.
- `add_track` `{"name": "Bass", "instrument": "synth", "index": 1}`: without `instrument` it's an
  audio track; `index` defaults to the end.
- `delete_track` `{"track": 1}`
- `set_track` `{"track": 1, "midi_fx": [{"type": "arpeggiator", "rate": 0.25, "order": "up_down", "octaves": 2, "gate": 0.8, "latch": false}]}`:
  an instrument track's MIDI effects, in order, before its instrument hears its notes (live and
  from the song), in place of what it has (`[]` takes them all out); up to 8. Each has a `type`
  and its settings, and `"bypass": true` keeps it but out of the way. Rates are in beats (0.25 a
  sixteenth, 1/3 an eighth-note triplet). `arpeggiator` (`rate`, `order`: up, down, up_down,
  played, random or chord, `octaves` 1-4, `gate` 0-1, `latch`), `chord_trigger` (`shape`: major,
  minor, seventh, major_seventh, minor_seventh, sus2, sus4, power, octave, in_key,
  in_key_seventh), `scale_quantizer` (`root` 0-11 or null for the song's key, `minor`),
  `transposer` (`semitones`), `velocity` (`scale` percent, `add`, `min`, `max`),
  `note_repeater` (`count`, `rate`, `decay` 0-1, `transpose` per repeat), `randomizer` (`pitch`
  and `velocity` ranges either way, `chance` percent), `modifier` (controller `from` sent as
  `to`, 0 drops it, `scale` percent).
- `set_track` `{"track": 1, "name": "Sub", "volume_db": -6, "pan": -0.3, "mute": false, "solo": false}`:
  any of them. `"pan_mode"`: `"balance"` (the default: turns one side down),
  `"stereo"` (places each side, `"spread"` -1 to 1 apart: 1 as recorded, 0 both in the middle,
  below 0 swapped) or `"binaural"` (round the listener's head, for headphones: its `pan` is
  then the way round, 0 in front, 0.5 right, -0.5 left, ±1 behind; `"elevation"` is degrees above
  (-90 to 90) and `"distance"` metres away (0.25 to 3, 1 by default)).
  `"color": "#d97757"` and `"icon": "808"` choose how it looks (`"auto"` for each
  goes back to what it plays saying: drums warm, bass violet, keys teal, vocals pink, each track a
  shade of its family). Icons: kick, snare, clap, hihat, cymbal, tom, drums, sticks, conga, shaker, tambourine, cowbell, 808, bass, synth-bass, piano, keys, organ, chords, guitar, guitar-electric, strings, harp, brass, sax, flute, synth, lead, arp, pluck, pad, bells, mallets, vocals, mic, choir, chop, fx, riser, impact, reverse, vinyl, texture, foley, audio, midi, sampler, loop, reference, click, star, heart, flame, bolt.
- `set_instrument` `{"track": 1, "instrument": {"plugin": {"path": "…", "class": "…"}}}`
- `set_synth` `{"track": 1, "patch": "Warm Pad", "settings": {"params": {"filter1_cutoff": 1800, "env1_attack": 600}, "filters": ["ladder", "none"]}}`:
  Synth-0, Pina Studio's synth, on a track. `patch` is one of Init, Sub Bass, Saw Bass, Analog Bass, Reese,
  Acid, Growl, Wobble, Supersaw, Trance Lead, Square Lead, Soft Lead, Ladder Lead, Sync Lead, Warm
  Pad, Glass Pad, Choir, Evolving Pad, Strings, Brass, Pluck, House Stab, Arp Pulse, Bell, FM Keys,
  Organ, Metal Hit, Noise Sweep, Drone, Tape Keys (its sound as it comes). `settings` is laid over
  it, field by field:
  - `params`: any of its numbers, in their own units, by id. `osc1_` to `osc3_`: `level`, `pan`,
    `octave`, `semi`, `fine`, `position` (through the wavetable, 0-1), `warp_a`, `warp_b` (0-1),
    `unison` (1-16), `detune` (cents), `blend`, `width`, `phase`, `random`. `sub_level`, `sub_pan`,
    `noise_level`, `noise_pan`, `noise_color`. `filter1_`, `filter2_`: `cutoff` (Hz), `resonance`,
    `drive`, `bass`, `key_track`, `spread`, `mix`. `env1_` to `env4_` (1 is the amp; times in ms):
    `delay`, `attack`, `hold`, `decay`, `sustain`, `release`, `curve`. `lfo1_` to `lfo8_`: `rate`
    (Hz), `phase`, `rise`, `smooth`. `macro1` to `macro8`. `glide` (ms), `drift` (how much old parts
    wander), `velocity`, `master` (dB).
  - `tables`: three wavetables, one an oscillator: Basic (sine, triangle, saw, square, swept),
    Analog, PWM, Harmonics, Odd, Soft Saw, Stack, Formant, Choir, FM 2:1, FM 3:1, FM 7:2, Sync,
    Fold, Bend, Organ, Bell, Digital, Growl, Metal, Comb, Glass, Pluck, Air.
  - `warps`: two for each oscillator, of `none`, `sync`, `bend_up`, `bend_down`, `bend_both`, `pwm`,
    `asym`, `mirror`, `flip`, `quantize`, `fold`, `phase_dist`, `fm_from_a`, `fm_from_b`,
    `ring_from_a`, `ring_from_b`, `am_from_a`, `am_from_b`, `fm_from_noise`, `fm_from_sub`.
  - `filters`: two of `none`, `ladder`, `ladder2`, `cascade`, `diode`, `snarl`, `low12`, `low24`,
    `high12`, `high24`, `band12`, `band24`, `notch`, `peak`, `comb_up`, `comb_down`, `formant`,
    `phaser`, `ring`, `crush`. `routing`: `series`, `parallel` or `stereo`. `to_filter`: five pairs
    of true/false, which filters each oscillator, the sub and the noise go through.
  - `routes`, the matrix, each `{"source": {"env": 1}, "dest": {"param": 75}, "depth": 0.4}`:
    `source` is `{"env": 0-3}`, `{"lfo": 0-7}`, `{"macro": 0-7}`, `velocity`, `note`, `wheel`,
    `pressure`, `bend` or `random`; `depth` is a share of the knob's travel, -1 to 1; `by` another
    source that scales it. `dest` is `{"param": n}`, or `{"effect": [place, knob]}` to ride an effect
    on the track, so its LFO can move the reverb.
  - `lfos`: eight, each `{"wave": "sine", "beats": 1, "retrigger": true, "one_shot": false}`; `wave`
    is `sine`, `triangle`, `saw_up`, `saw_down`, `square`, `hold`, `smooth`, `lorenz`, `rossler` or
    `{"drawn": [[0, 0], [0.5, 1], [1, 0]]}`; `beats` a cycle in time with the song.
  - `voicing`: `polyphony`, `mono`, `legato`, `glide_held`, `bend_semitones`. `arp`: `mode` (`off`,
    `up`, `down`, `up_down`, `played`, `random`, `chord`), `beats`, `octaves`, `gate`, `swing`.
    `quality`: `draft`, `good` or `best`. `sub_wave` 0-3, `sub_octave`. `macros`: the eight
    macros' names, in order. Every patch comes with the same eight, each already moving what its
    name says and leaving the sound as it was at 0: Bright, Bite, Motion, Width, Weight, Air,
    Attack, Tail (`macro1` to `macro8`).
  Its numbers can be automated with `set_automation`, `"param": "synth"` and `"knob"` an id, in its
  own units. A track without the synth gets it.
- `set_insert` `{"channel": {"track": 1}, "index": 0, "plugin": {"format": "au", "class": "AUDelay", "uid": "aufx dely appl"}}`:
  an effect into an insert slot (`{"aux": 0}` for an aux, `"master"` for the master, after its
  built-in effects). An index past the last adds one; leaving out `plugin` removes the one there.
  A slot's automation goes with its plugin.
- `move_effect` `{"channel": {"track": 1}, "from": 4, "to": 0}`: moves an effect to another place
  in the channel's chain, which runs its built-in EQ, compressor and saturator and its plugins in
  any order (by default the built-ins, then the plugins; `"chain"` in the song lists it, a
  `"plugin"` the next of its inserts). The places count every effect in that list: with no order
  set, 0 to 2 are the EQ, compressor and saturator (had or not) and 3 on the plugins, then any
  stacked built-ins (`"more"`, each its own `"more"` place in the chain), then the limiter. A
  plugin keeps its settings and automation; a stacked built-in its settings.
- `set_plugin_param` `{"channel": {"track": 1}, "plugin": "instrument", "param": "Filter Cutoff", "text": "850 Hz"}`:
  turns a plugin's parameter: `param` by its id or its name, set to `value` (0 to 1) or to
  `text`, a value as the plugin shows it (`"850 Hz"`, `"2.4 kHz"`, `"-6 dB"`, `"Sine"`).
  `plugin` is `"instrument"` or `{"insert": 0}`. `pina params <project> <track>` (`--aux`,
  `--master`, `--insert N`) lists them all: id, name, value and how it's shown, default, unit,
  steps, and for a choice, `options` (what each is called, and a value for it). Parameters are
  applied in the order they're set: set an effect's type before what belongs to it. Without
  `value` or `text`, Pina Studio stops setting it and it goes back to what it was.
- `set_preset` `{"channel": {"track": 1}, "plugin": "instrument", "preset": "Warm Pad"}`: loads a
  preset: by name, or `{"program": 3}` (one of the plugin's own; with `"list"` for VST3) or
  `{"file": "/path/Some.vstpreset"}`. `pina presets <project> <track>` lists them: the plugin's
  programs and the preset files of its format (`.vstpreset`, `.aupreset`, `.fxp`, CLAP's), from
  `~/Library/Audio/Presets/<vendor>/<plugin>` and `/Library/Audio/Presets/…`; `pina presets …
  --save "My Bass"` saves its settings as a new one. A preset replaces the plugin's settings and
  what was set over them; without `preset`, it goes back to its settings from before (in the
  app; from the command line, its defaults).
- `set_sidechain` `{"channel": {"track": 2}, "insert": 0, "from": 0}`: what a plugin with a
  sidechain input (a compressor ducking the bass under the kick, a vocoder) hears there: track
  `from`'s output, after its fader. Without `from`, nothing.
- `add_midi_region` `{"track": 0, "start": 0, "length": 3840, "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}], "controls": […]}`:
  `controls`, if given, as `set_controls` takes them.
- `set_notes` `{"track": 0, "region": 0, "notes": […]}`: replaces all of a region's notes.
- `set_controls` `{"track": 0, "region": 0, "controls": [{"start": 0, "type": "cc", "number": 1, "value": 64}, {"start": 480, "type": "pitch_bend", "value": 8191}]}`:
  replaces a MIDI region's controller moves, each at `start` ticks from the region's start and
  holding until the next of its kind: `cc` (`number` 0-127: 1 mod wheel, 7 volume, 10 pan, 11
  expression, 64 sustain; `value` 0-127), `pitch_bend` (`value` -8192 to 8191; how far that bends
  is the instrument's range), `aftertouch` (`value` 0-127), `poly_pressure` (`pitch`, `value`
  0-127), and `expression` for one sounding note (`pitch`, `kind`, `value`): `tuning` in
  semitones (an 808 glide), `volume` (gain, 1 as played), `pan` (-1 to 1), `brightness`,
  `vibrato`, `expression` (0 to 1). Plugins get them all (VST3 through the plugin's MIDI mapping;
  Audio Units and VST2 have no per-note expression); the built-in sampler follows pitch bend
  (two semitones either way) and each note's `tuning` and `volume`, gliding smoothly, so a few
  tuning moves make an 808 slide. A region starting part-way, or looping, starts with each
  controller where it stood.
- `add_pattern_region` `{"track": 0, "start": 0, "length": 3840, "step": 240, "swing": 58, "rows": [{"pitch": 36, "cells": [110, 0, 0, 0, 100, 0, 0, 0, 110, 0, 0, 0, 100, 0, 0, 0]}]}`:
  a step-sequencer pattern: each row a note, each cell a step's velocity (0 off), looping over
  the region. `step` is ticks per step (240, a sixteenth, by default; 480 an eighth, 160 a
  sixteenth triplet, 360 a dotted sixteenth); `swing` (50 straight, up to 75) makes every
  second sixteenth late (`"swing_eighths": true`: every second eighth). On a
  `"drums"` track: 36 kick, 38 snare, 37 side stick, 39 clap, 42 closed hat, 46 open hat,
  41/45/48 toms, 49 crash.
  The pattern's own settings, all optional: `"steps"` (how many it has, 1 to 64; as long as its
  longest row by default), `"mode"` (how its rows run through their steps: `"forward"`,
  `"backward"`, `"ping_pong"`, `"pendulum"`, `"wander"`, `"shuffle"`, `"random"`), `"legato"`
  (a step held its whole length runs into the next), `"key"` (0 C to 11 B) and `"scale"`
  (`"major"`, `"minor"`, `"harmonic_minor"`, `"melodic_minor"`, `"dorian"`, `"phrygian"`,
  `"lydian"`, `"mixolydian"`, `"locrian"`, `"major_pentatonic"`, `"minor_pentatonic"`,
  `"blues"`, or `"chromatic"`).
  A row's own settings: `"mute"`, `"solo"`, `"name"`; its loop, `"loop_start"` and `"loop_end"`
  (steps from 0, the end not played: a row of 16 looping on its first 3 is a polymeter; a row
  shorter than the others loops on its own length too); its own `"rate"` (ticks) and `"mode"`.
  `"melodic": true` makes a row whose steps each play their own note. A row with a `"target"`
  is an automation row: its steps set a parameter instead of playing notes, `"value"` 0 to 1
  each (`{"target": "volume"}`, `"pan"`, `{"target": {"send": {"bus": 1}}}`,
  `{"target": {"plugin": {"plugin": "instrument", "param": 4}}}`, `{"target": {"cc": 74}}`,
  `"pitch_bend"`, `"pressure"`), held to the next step that's on, or sliding to it with
  `"slide": true`.
  Each step's settings, as lanes as long as `cells` (left out, as usual): `"chance"` (0 to 100
  that it plays, rolled afresh each time round), `"transpose"` (its note, -12 to 12 semitones)
  and `"octave"` (octaves up or down), `"repeat"` (hits in the step, 1 to 16), `"gate"` (how
  long it holds, % of a step, 1 to 100; 100 by default), `"tie"` (true: it holds on through
  the next step, which doesn't play again), `"nudge"` (its offset, % of a step off the grid,
  -50 to 50), `"rates"` (its own length in ticks, 0 the row's), `"skip"` (true: the row jumps
  over it), `"condition"` (`"always"`, `"recurrence"`, `"first_loop"`, `"last_loop"`) with
  `"every"` (`[2, 3]`: the second pass of every three), and for repeats `"ramp"` and `"curve"`
  (velocity across them, %) and `"spread"` (+% the first longest, -% the last): `{"pitch": 42,
  "cells": [...], "chance": [100, 60, 100, 40], "repeat": [1, 1, 1, 3], "tie": [false, true,
  false, false]}`.
- `set_pattern` `{"track": 0, "region": 0, "rows": […], "swing": 62, "steps": 32, "mode":
  "ping_pong"}`: replaces a pattern's rows, or sets its swing, `step`, or any of the settings
  above.
- `add_audio_region` `{"track": 2, "file": "/path/take.wav", "start": 0, "offset": 0, "length": 7680}`:
  a WAV, AIFF, FLAC or MP3 (at any sample rate: it's converted to the project's), copied into
  `Audio Files`; `offset`
  (samples into it, at the project's rate) and `length` (ticks; the rest of the file by default)
  are optional.
- `move_region` `{"track": 0, "region": 0, "start": 3840, "to_track": 1}`: MIDI regions go on
  instrument tracks, audio regions on audio tracks.
- `trim_region` `{"track": 0, "region": 0, "start": 960, "end": 7680}`: either edge; content
  stays where it is on the timeline.
- `split_region` `{"track": 0, "region": 0, "at": 1920}`: answers with the second half.
- `delete_region` `{"track": 0, "region": 1}`
- `set_region` `{"track": 0, "region": 0, "name": "Hook", "loop": true, "transpose": 12, "velocity": -10, "quantize": 240}`:
  any of them. A looped region's `length` is one pass, its notes; `"loop_length"` is how far all
  its passes run, ticks from its start (0 for none), as dragging out its top right corner does.
  To play a 4-bar region through bar 32: `{"op": "set_region", "track": 0, "region": 0,
  "loop_length": 122880}`, or `trim_region` its `end` out to 122880: trimmed out, a looped region
  runs its loops further and keeps its pass. With `"loop": true` alone it plays until the next
  region on its track, or the end of the song, which other regions set: it never makes the song
  longer by itself. The answer says how far its loops run: `{"loops_to": 122880, "bars": "1-32"}`,
  with a `"note"` if they don't repeat, or if each pass ends in silence. `show` gives each looped region its `loops_to`, and its `length`
  is one pass, so the bars up to `loops_to` are full, not empty. A region added or moved into a
  loop's run plays in place of it, and the loops carry on after it, in time. `"fade_in"` and `"fade_out"` (ticks) fade an audio
  region in and out (an equal-power curve). `transpose` (semitones), `velocity` (added) and `quantize` (a grid in ticks; 0 is
  off) change how a MIDI or pattern region plays, not its notes;
  `transpose` moves an audio region's pitch too, keeping its length (a loop into the song's key).
  For audio, `"stretch_mode"` is how it's stretched: `"polyphonic"` (the default: Rubber Band's finer
  engine, with the recording kept around every hit, so drums stay sharp), `"slicing"` (cut at the
  hits, each played as recorded at its new time) or `"speed"` (a turntable: faster is higher);
  `"follow"` is how it keeps time: `"off"` (played as recorded, whatever the tempo), `"on"`
  (`true`: its beats on the song's, through every tempo change, as loops from the sample browser
  are), `"bars"` or `"beats"` (as on, with the downbeats or every beat its analysis found pinned
  to the song's, for a take that drifts). `"tempo"` is its own tempo in BPM, what following keeps
  to the song's (found when it's added; typed when that's wrong). `"formants": true` keeps a
  voice's formants when it's transposed. `"warps": [[48000, 52800], …]` are warp markers: a frame
  of the file and where it plays (both at the project's rate): in the stretched audio, or when it
  follows, in its own time at its tempo; the audio between is stretched to fit, in every stretch mode.
- `add_alias` `{"track": 0, "region": 0, "start": 7680, "to_track": 0}`: an alias of a MIDI or
  pattern region: a copy linked to it (both get the same `"link"` number), so editing the notes or
  steps of either edits both. Its place, length and playback settings are its own. Splitting it,
  or trimming its start, makes it a region of its own.
- `unlink` `{"track": 0, "region": 2}`: makes an alias a region of its own.
- `set_cell` `{"track": 1, "scene": 0, "region": {"length": 3840, "midi": {"notes": […]}}}`:
  puts a loop in a Live Loops cell (scenes numbered from 0); any region content works, `start`
  doesn't matter. Without `region`, empties the cell. Launch cells in the app: `pina launch
  <project> <track> <scene>` starts a cell at the next bar (or now, starting playback), and
  `pina launch <project> <track>` stops it, the track's own regions playing again.
- `stretch_region` `{"track": 2, "region": 0, "length": 7680}`: time-stretches a region to that
  length. Audio plays slower or faster at the same pitch (up to four times either way), as its
  `"stretch_mode"` says; a MIDI region's notes and controllers, or a pattern's steps, spread out or close
  up.
- `analyze_tempo` `{"track": 2, "region": 0}`: listens to an audio region for its tempo, each
  beat and its downbeats, kept on the region (`"tempo"`, `"grid"`) for following and locking;
  answers `{"tempo": 92.4, "beats": 64, "meter": 4, "loop": false}`. A loop (short, whole bars)
  is even; a take is followed beat by beat.
- `adapt_tempo` `{"track": 2, "region": 0, "steady": false, "align": true}`: the song keeps time
  with an analysed audio region: from its first downbeat (moved onto the nearest bar unless
  `"align": false`) the song's tempo is the take's, bar by bar, or its average with `"steady":
  true` (the whole song's, if the tempo doesn't change elsewhere). The region then follows the
  song locked to its beats, so it plays as recorded. Answers the song's tempo and how many
  tempo changes it has.
- `fit_tempo` `{"track": 2, "region": 0}`: the tempo set so the region, as it plays now, fills the
  cycle; it moves to the cycle's start, played as recorded. Answers `{"bpm": 87.5}`.
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
- `add_stack` `{"tracks": [0, 3], "name": "Drums"}`: a summing stack, a channel (an aux, listed
  with the others) those tracks play into, the tracks moved together under the first of them.
  Answers with its bus, where its tracks are now (`"tracks"`) and where every track went
  (`"moved"`, by where it was). `flatten_stack` `{"aux": 2}` undoes one: its tracks play into
  the master again.
- `add_kit` `{"name": "Trap kit", "pads": [{"file": "/samples/Kick 01.wav", "steps": [0, 8]},
  {"file": "Audio Files/Hat.wav", "name": "Closed hat", "steps": [2, 6, 10, 14]}], "index": 3,
  "at": 0, "pattern": true}`: a kit as a stack. A kit track (instrument `"kit"`, which sounds
  nothing itself) whose notes play its pads, and a track a pad: its own sampler with its sound
  (copied into the project), channel and effects, playing its note from the kit track (and its own
  regions). A pad's note comes from its name's drum (kick 36, snare 38, clap 39, closed hat 42,
  open hat 46…) unless given, else the next free one from 36; hats choke each other (an open hat
  cut off by a closed one) unless a `"choke"` group is given. The kit's tracks go at `"index"`
  (the end if not given) in a stack named for the kit; with `"pattern"` (the default) the kit
  track gets a starter pattern at `"at"`: a bar looping four, a row a pad, on its `"steps"`
  (sixteenths). Answers `{"kit": 3, "pads": [4, 5], "aux": 1}`. A pad track's `"pad"` is
  `{"kit": <the kit track's id>, "note": 36, "choke": 1}`.
- `import_midi` `{"file": "/Packs/Afro/PML BS Kila - G#m.mid", "at": 7680, "fit": true}`: a MIDI
  file brought in, a track a part (named for it), its notes, controllers and bends in a region at
  `at` (ticks; 0 by default), from `index` on (the end by default): drums on the kit, every other
  part on General MIDI's instrument for its program (Apple's General MIDI synth). `transpose`
  moves it by semitones (drums never), or `fit` into the song's key from its own (its name's or
  its notes'). A song with nothing in it yet takes its tempo, meter and key. Answers the tracks
  made, its parts, the transposition and what it is.
- `split_kit` `{"track": 3}`: a kit track's notes moved onto its pads' own tracks, each pad's in
  a region of its own where they were (a pattern's row a pattern of its own); anything that's no
  pad's stays. Answers how many regions it made.
- `set_aux` `{"aux": 0, "name": "Reverb", "volume_db": -3, "pan": 0, "mute": false}`
- `set_automation` `{"track": 0, "param": "volume", "points": [{"tick": 0, "value": -12}, {"tick": 3840, "value": 0}]}`:
  `volume` in dB, `pan`, `mute` (1 muted, 0 not; always steps), `send` (with `"send": i`, which
  of the track's sends, in dB) or `plugin` (with `"plugin": "instrument"` or `{"insert": 0}`, and
  the parameter as `"id"`: its id or its name) from 0 to 1, or as the plugin shows it (a point's
  `"text"`, like `"2 kHz"`, instead of `"value"`), or an EQ band's `eq_freq` (Hz), `eq_gain` (dB)
  or `eq_q` (with the band's place in the EQ as `"id"`): a filter sweep, a boost that grows into
  the chorus; or `modulator`, one of a modulator's settings (`"modulator"`, its place, and
  `"setting"`: a macro's `value`, an LFO's `phase`, steps' `glide`, an envelope's `attack`,
  `decay`, `sustain` and `release`, a follower's `attack` and `release`; times in ms): a macro
  swept through the build; or the limiter's `limit_gain` (dB), `limit_ceiling` (dB),
  `limit_release` (ms) or `limit_clip` (0 to 1): pushed harder in the chorus; or `fx`, a knob of a
  stacked effect (`"fx"`: `filter`, `motion`, `shaper`, `gate`, `de_esser`, `stereo`, `multiband`,
  `pitch`, `tape_stop`, `tune`, `reverb`, `delay`, `eq`, `comp`, `sat` or `limit`; `"which"`, of
  those, from 0; `"knob"` by its name: `Cutoff`, `Mix`, `Decay`, `Band 1 drive`) in the knob's own
  units; `"own": true` with `comp` or `sat` takes the track's own instead of a stacked one: a filter opening into the drop, a reverb's mix thrown on the
  last word. `show` lists those lanes by the effect's place and the knob's, 0 to 1. Lines between points are straight unless the
  point they start from has a `"curve"` (-1 to 1: towards 1 it starts slowly and ends fast,
  towards -1 the other way round), `"ease": true` (an S: slow out of it and slow into the next)
  or `"step": true` (it holds its value, then jumps to the next).
  No points takes it out.
- `shape_automation` `{"track": 0, "param": "volume", "shape": "sine", "from": 3840, "to": 7680, "every": 960, "low": -12, "high": 0}`:
  draws a shape over what was there from `from` to `to`, a cycle every `every` ticks (a beat
  without it), between `low` and `high`; the line either side stays as it was. Shapes: `sine`,
  `triangle`, `saw_up`, `saw_down`, `square`, `random` (a new value each cycle), and `ramp_up` and
  `ramp_down` (once across the whole span, for a fade or a build). Takes the parameter as
  `set_automation` does (a plugin's by its id).
- `smooth_automation` `{"track": 0, "param": "volume", "from": 0, "to": 7680, "width": 480}`:
  smooths the line from `from` to `to` (all of it without them): its corners and wobbles
  rounded off over about `width` ticks (an eighth without; more is smoother), its ends where
  they were. Takes the parameter as `shape_automation` does. Mute, all steps, can't be smoothed.
- `set_palette` `{"sounds": [{"path": "/Samples/Kick 01.wav", "role": "kick"}, {"path": "/Samples/Dusty Keys.wav"}], "only": true}`:
  the sounds the person picked for the song, for an agent to make it with (each audio file, and
  what it's for if they said: kick, snare, hats, bass, pad, lead…); `"only": true` asks for these
  and no others. `sounds` replaces the list; leave it out to change only `only`. `"devices"`
  are the instruments and effects picked, the same way: `{"name": "Wavetable Synth", "instrument": true,
  "plugin": {…as set_instrument takes it}, "role": "bass"}`, or Pina Studio's own by `"own"`
  (`synth`, `drums`, `sampler`, `eq`, `comp`, `sat`). The `palette` tool (in `pina mcp`) lists
  them with what each is and whether it's in the song yet.
- `set_automation_mode` `{"track": 0, "mode": "latch"}`: `read` (the default) plays automation;
  `touch` and `latch` also record what's moved in the app while the song plays, until the control
  is let go (touch) or playback stops (latch); `trim` records as touch does but moves the line
  that's there up or down by how far the control is moved, keeping its shape; `write` records
  every automated control from where playback starts, over what was there; `off` ignores it.
- `set_eq` `{"channel": {"track": 1}, "eq": {"bands": [{"shape": "low_cut", "freq": 80, "slope": 24}, {"shape": "bell", "freq": 350, "gain_db": -3, "q": 1.2}, {"shape": "high_shelf", "freq": 10000, "gain_db": 2}]}}`:
  the channel's built-in EQ, before its inserts: up to 24 bands, in order. A band's `shape` is
  `bell`, `low_shelf`, `low_cut`, `high_shelf`, `high_cut`, `notch`, `band_pass`, `tilt_shelf`
  (half its gain down below its frequency, half up above), `flat_tilt` (a straight tilt from 20 Hz
  to 20 kHz, level at its frequency, its gain the difference end to end) or `all_pass` (moves the
  phase round its frequency, not the level: to line up two takes); `freq` 10 to 30000 Hz;
  `gain_db` -30 to 30 (bells, shelves, tilts); `q` 0.025 to 40, the width (a cut's or shelf's bump
  at its corner; left out, its shape's own: none for a cut or shelf); `slope` in dB an octave, 12
  if left out, fractions too (20.5): how steep its sides are, for every shape but a flat tilt.
  Cuts and band passes take 0 to 96 (a cut 144 too: a brickwall), shelves and tilt shelves 6 to
  96, bells 12 to 96 (steeper: the same gain and width, its skirts falling away faster), notches
  and all passes 12 to 96 in twelves. At 6 or less a cut or shelf has no width to set. `"gain_q": true` narrows it as its gain grows, as an analog EQ's bands do.
  `"placement"` puts it on part of the sound: `stereo` (the default), `left`, `right`, `mid` (what
  both sides share: vocals, bass, kick) or `side` (the edges: a low cut on the side keeps the low
  end mono). `"dynamic": {"range_db": -4}` makes it move with the sound (bells, shelves and
  tilts): as what it covers gets louder than its threshold, its gain goes by up to `range_db` more
  (below 0 down, to tame harshness or boom only when it's there; above 0 up). It sets its own
  threshold (a little over what it usually hears, so it moves on what stands out) unless given
  `"threshold_db"` (-80 to 0), and its own attack and release (by its frequency) unless given
  `"attack_ms"` and `"release_ms"`; `"trigger": [5000, 8000]` has it listen between two
  frequencies rather than to what it covers, `"external": true` to the EQ's sidechain, and
  `"bypass": true` keeps it still. `"spectral": true` moves only what stands out in the band,
  frequency by frequency (resonances, harsh peaks, a masking clash), the rest left alone, linear
  phase (adding the latency of its window); its `"density"` (0 to 1, 0.5 if left out) is how
  wide a stretch moves together (0 wide, 1 each frequency alone), and `"tilt": false` stops it
  hearing the highs 3 dB an octave louder, as a mix's spectrum needs. `"off": true` keeps a band
  but stops it. The EQ's `"scale"` (0
  to 2, 1 as set) scales every band's gain; `"output_db"` is its level after them, `"auto_gain":
  true` makes up for how much louder or quieter its bands make it, `"pan"` (-1 to 1) balances
  left and right (with `"pan_ms": true` mid against side), `"invert": true` turns it upside down;
  `"character"` is `clean`, `subtle` or `warm` (an analog EQ's saturation, more where it boosts);
  `"sidechain"` is the track (numbered as everywhere here) its dynamic bands hear with
  `external`, after its fader: a bass dipping at the kick's frequency as it hits, a pad ducking
  under the kick, the chords dipping their presence while the lead plays. `"processing"` is
  how it treats phase: `zero_latency` (the default: as an analog EQ, no latency), `natural` (an
  analog EQ's phase right up to the top, about 1 ms late) or `linear` (no phase shift at all, the
  bands' timing kept together, its every dynamic band moving as spectral ones do; tens to
  hundreds of ms late, more with mid or side bands). `"resolution"` (0 low to 3 very high, 1 if
  left out) is how finely linear phase and spectral dynamics work: truer low down, more latency.
  The engine waits for that latency, so every channel stays in time. `bands` replaces
  them all; to change one,
  `set_eq_band`. The old names still work, on the first of their kind: `"low_cut": 80` (Hz; 20 or
  less is off), `"low_mid"` and `"high_mid"` (`{"freq", "gain_db", "q"}`, the first and second
  bells) and `"high_shelf"`. Leave out what you don't change; `"bypass": true` keeps it but stops
  it; no `eq` takes it out. `channel` is `{"track": i}`, `{"aux": i}` or `"master"`.
- `set_eq_band` `{"channel": {"track": 1}, "band": 2, "freq": 3500, "gain_db": -3, "q": 2, "dynamic": {"range_db": -4, "threshold_db": -28}}`:
  one band of the channel's EQ, by its place (0 first): only what's given changes (`shape`,
  `freq`, `gain_db`, `q`, `slope`, `placement`, `gain_q`, `dynamic` (`false` makes it still),
  `off`). The
  place after the last adds a band (a 1 kHz bell unless it says); `"remove": true` takes the band
  out, the ones after moving up. The channel gets an EQ if it hasn't one.
- `set_comp` `{"channel": {"track": 1}, "comp": {"threshold_db": -18, "ratio": 4, "attack_ms": 10, "release_ms": 120, "makeup_db": 3, "style": "punchy", "mix": 1, "spectral": 0.3}}`:
  the built-in compressor, after the EQ, both sides turned down together. `style` is `clean` (a
  6 dB soft knee; `"rms": true` makes it hear an RMS average over 15 ms rather than the peaks),
  `smooth` (RMS, a 12 dB knee: gentle), `punchy` (peaks, a hard knee, a touch of colour) or `bus`
  (console-style glue: ratio 2, 4 or 10, attack 0.1 to 30 ms and release 100 to 1200 ms in their
  steps). `mix` 0 dry to 1 (parallel compression); `"auto_release": true` lets go quickly after a
  hit and slowly after a long squeeze; `sidechain_hz` high-passes what it hears (90 keeps a mix's
  lows from pumping it; 0 none); `spectral` 0 to 1 gives the turning down to the part of the
  spectrum that caused it (a kick turns down the lows, not the hats over it). Same rules as
  `set_eq`. A compressor added in the app sets itself by listening (`pina learn`).
- `set_saturator` `{"channel": {"track": 1}, "sat": {"bands": [{"style": "warm_tube", "drive_db": 9}, {"style": "transformer", "drive_db": 6, "tone": 0.2}], "crossovers": [250], "mix": 0.8, "output_db": -1}}`:
  the built-in saturator, after the compressor: up to six bands split at `crossovers` (Hz, one
  fewer than the bands; `slope` 12, 24 or 48), each with its own `style` (`warm_tape`,
  `clean_tape`, `warm_tube`, `clean_tube`, `transformer`, `crunch`, `heavy`, `rectify`, `foldback`,
  `lofi`), `drive_db` 0-24 (quiet sounds keep their level, loud ones take on the style),
  `tone` -1 darker to 1 brighter, `dynamics` -1 (louder hits) to 1 (squeezed), `feedback` 0-1
  ringing at `feedback_hz`, `mix`, `level_db`, `mute`, `solo`. `quality` 2, 4 or 8 times
  oversampled (the harder styles want more); `mix` 0 dry to 1. `drive_db` or `tone` given at the
  top set every band's. `bands` replaces the list. Same rules as `set_eq`.
- Stacking: a channel has any number of each built-in. `set_eq`, `set_eq_band`, `set_comp`,
  `set_saturator` and `set_limiter` take `"which"`: 0 (or left out) is the channel's own, 1 on
  the others of that kind in the order they run, and one past the last adds another, last before
  a limiter that's last: `{"op": "set_comp", "channel": {"track": 1}, "which": 1, "comp":
  {"style": "punchy", "ratio": 8}}` puts a second, harder compressor after the first; no value
  takes that one out. `show` lists a channel's stacked ones as `"more"`; `move_effect` moves them.
  A stacked one's settings aren't automated or modulated (the channel's own are).
- `set_limiter` `{"channel": "master", "limit": {"style": "modern", "gain_db": 6, "ceiling_db": -1}}`:
  the built-in limiter, last in the chain unless moved: `gain_db` 0-30 drives it in, and nothing
  goes over `ceiling_db` (-30 to 0; with `true_peak`, on by default, not even between samples).
  `style` is how it moves: `transparent` (clean, slow, a soft knee), `punchy` (grabs late, lets go
  fast, the very tops clipped), `dynamic` (short peaks let go quickly, long squeezes slowly),
  `allround`, `aggressive` (as loud as it gets), `modern` (a smooth clipper before a careful
  limiter: dense, still clean), `bus` (glue), `safe` (never distorts: no clipping, all of its
  lookahead), `classic` (the plain digital brickwall) or `multiband` (lows, mids and highs held
  apart first, so a kick doesn't duck the cymbals). `lookahead_ms` 0.1-10 is how far ahead it
  reads (the channel is that much late, compensated); `attack_ms` how much of it it takes to come
  down; `release_ms` 1-2000, with `auto_release` (on) a fast release for short peaks and a slow
  one for long squeezes around it; `link` 0 (sides apart) to 1; `clip` 0-1 rounds peaks off
  before it limits, at `oversample` 1, 2, 4 or 8 times the rate; `dc` takes out a DC offset.
  `unity` (hear it at the level it came in) and `delta` (hear only what it takes away) are for
  listening and never in a bounce. Same rules as `set_eq`. `pina master` sets it to a loudness.
- `set_reverb` `{"channel": {"aux": 0}, "reverb": {"style": "plate", "decay_s": 1.8, "predelay_beats": 0.125, "mix": 1}}`:
  a reverb stacked on the channel (most often an aux tracks send to: `mix` 1 there, all
  reverb; on a track, `mix` 0.2-0.4). `which` is its place among the channel's reverbs (one
  past the last adds another; no `reverb` takes it out). `style` is `room`, `chamber`, `hall`,
  `plate`, `ambience`, `cathedral` or `shimmer` (a new style starts as that style usually is,
  then takes the fields given); `predelay_ms` (0-500) or `predelay_beats` (in time with the
  song), `size` 0-1, `decay_s` (0.1-30: how long its tail takes to fall 60 dB), `early` (its
  first reflections, 0-1), `diffusion` 0-1, `damping_hz` (the tail darkens above it), `bass`
  (how much longer the lows ring, 0.25-4), `modulation` 0-1 (lusher, never metallic),
  `low_cut_hz`, `high_cut_hz`, `width` 0-2, `ducking` 0-1 (the tail ducks while the sound
  plays, blooming in the gaps), `shimmer` 0-1 (the tail climbs an octave), `freeze`, `mix`,
  `output_db`, `bypass`.
- `set_delay` `{"channel": {"aux": 1}, "delay": {"style": "tape", "beats": [0.75, 0.75], "feedback": 0.4, "mix": 1}}`:
  a delay stacked on the channel, as `set_reverb`. `style` is `digital`, `tape` (wow, flutter,
  warm saturated repeats), `analog` (darker each time round), `ping_pong`, `dub` (long, dark,
  thinned, piling up) or `slapback`; `sync` with `beats` per side (0.75 a dotted eighth, 0.5
  an eighth, 0.333 an eighth triplet), else `ms` per side; `feedback` 0-1.1 (past 1 it grows,
  held by its drive), `cross` 0-1 (1 ping-pongs), `low_cut_hz` and `high_cut_hz` (each time
  round), `drive` 0-1, `wow` and `flutter` 0-1, `diffusion` 0-1 (the echoes blur), `ducking`
  0-1, `width` 0-2, `freeze`, `mix`, `output_db`, `bypass`.
- `set_fx` `{"channel": {"track": 1}, "fx": "filter", "settings": {"mode": "ladder", "cutoff_hz": 800, "resonance": 0.5, "lfo_depth": 2, "lfo_beats": 4}}`:
  a tool stacked on the channel, as `set_reverb` (`which` its place among that tool's; one past
  the last adds another; no `settings` takes it out). `fx` is:
  - `filter`: `mode` (`low_pass`, `high_pass`, `band_pass`, `notch`, `ladder`, `comb`,
    `formant`), `cutoff_hz`, `resonance` and `drive` 0-1; an LFO on the cutoff: `lfo_depth`
    (octaves, 0-6), `lfo_wave`, `lfo_beats` a cycle in time with the song (0: `lfo_hz`),
    `lfo_stereo` 0-1; the sound's own level on it: `env_depth` (octaves, -6 to 6: a wah),
    `env_attack_ms`, `env_release_ms`; `mix`, `output_db`.
  - `motion`: `style` (`chorus`, `ensemble`, `flanger`, `phaser`, `vibrato`, `tremolo`; a new
    style starts as it usually is), `rate_hz` or `beats` a cycle, `depth` 0-1, `feedback` -0.95
    to 0.95, `delay_ms`, `stages` (a phaser's, 2-12), `spread` 0-1 (the sides apart; a tremolo
    at 1 auto-pans), `mix`, `output_db`.
  - `multiband`: three bands split at `crossovers` (two Hz), each with its own `threshold_db`,
    `ratio`, `lift` (0-1: how far its quiet parts come up, the huge, close sound) and `gain_db`
    (each three values, low first); `attack_ms`, `release_ms`, `mix`, `output_db`.
  - `pitch`: `semitones` (-24 to 24), `cents`, `feedback` 0-0.9, `mix`, `output_db`. A chord stays a
    chord; it comes out about 85 ms late, which the song makes up for.
  - `tape_stop`: `stop` over 0.5 slows to nothing (automate it to throw), `stop_beats`,
    `start_beats`, `curve` 0-1, `mix`, `output_db`.
  - `tune`: pitch correction, the voice's character kept. `notes` twelve true/false from C (those
    it may land on: the song's key), `speed_ms` (0 snaps, the robot; 40 and up stays human),
    `amount` 0-1, `low_hz`, `high_hz`, `output_db`.
  - `shaper`: `attack` and `sustain` -1 to 1 (up to 18 dB either way, whatever the level),
    `speed_ms`, `clip`, `mix`, `output_db`.
  - `gate`: `threshold_db`, `range_db`, `ratio` (1.5 an expander, 20 a gate), `attack_ms`,
    `hold_ms`, `release_ms`, `low_cut_hz` and `high_cut_hz` (what it hears), `output_db`.
  - `de_esser`: `freq_hz`, `width` (octaves), `threshold_db`, `range_db`, `wide` (the whole sound
    down, not just the band), `listen` (hear only what it hears).
  - `stereo`: `width` `[low, middle, high]` (0 mono, 1 as it is, 2 wider), `crossovers` `[a, b]`,
    `mono_hz` (everything under it mono), `mid_db`, `side_db`, `balance`, `swap`,
    `invert_left`, `invert_right`, `output_db`.
- `edit_notes` `{"track": 1, "region": 0, "edit": {"kind": "quantize", "grid": 240, "strength": 1, "swing": 50}}`:
  changes a MIDI region's notes in one go (or one note, with `"note": 3`). `kind` is `quantize`
  (`strength` 0-1, `swing` 50 straight to 75), `humanize` (`timing` ticks, `velocity`, `seed`),
  `legato`, `chop` (`grid`), `arpeggiate` (`grid`), `strum` (`ticks` between strings), `reverse`,
  `flip` (pitches upside down), `glue` (touching notes of a pitch made one), `transpose`
  (`semitones`), `velocity` (`by`) or `dynamics` (`percent` of their spread, 100 as they are, and
  `by`).
- `add_marker` `{"tick": 15360, "name": "Chorus"}`: names a place in the song; answers with its
  index, markers being numbered in time order.
- `set_marker` `{"marker": 0, "name": "Verse 2", "tick": 7680}`, `delete_marker` `{"marker": 0}`
- `move_section` `{"section": 2, "before": 0}`: a section (a marker to the next, numbered from
  0; the last runs to the song's end, or its marker's `"length"`) moved before another (`before`
  one past the last: to the end), everything in it along: every track's regions (cut at its
  edges), their automation, the markers and chords in it and the tempo it's at; the rest of the
  song closes up behind it and makes room for it.
- `copy_section` `{"section": 1, "before": 3}`: a copy of a section before another, the song
  making room for it.
- `swap_sections` `{"a": 0, "b": 2}`: two sections change places, everything in them along.
- `delete_section` `{"section": 1}`: a section taken out, everything in it, the song closing the
  gap. `clear_section` `{"section": 1}` empties one of regions and automation, keeping its time.
- `add_section` `{"name": "Bridge", "bars": 8}`: a section after the last, `bars` long (8 if not
  given); answers `{"section": 4, "tick": 61440}`.
- `insert_time` `{"tick": 15360, "length": 3840}`: that many ticks of nothing at a tick,
  everything after it (regions cut there, automation, markers, chords, tempo changes) moving
  later. `remove_time` `{"from": 15360, "to": 19200}` takes a stretch out, everything after it
  moving up.
- `set_key` `{"key": "A minor", "move_notes": true}`: the song's key, which the piano roll shows
  as a scale and the score as its signature. With `"move_notes": true` the music moves into it:
  every note and pattern row on pitched instruments goes up or down to the new tonic (the nearer
  way), each step of the old scale landing on the same step of the new, so major to minor changes
  the mode; drums and audio stay. Without `key`, Pina Studio works it out from the notes.
- `set_cycle` `{"start": 0, "end": 15360, "on": true}`: playback loops over that range (ticks);
  `"on": false` keeps the range without looping.

## Writing music

Pina Studio writes no parts for you: there are no genre templates, grooves or styles to call.
Write every note yourself, as a producer and a musician would, for whatever the song is. Notes go
in with `add_midi_region` and `set_notes`, step patterns (your own rows, velocities, swing,
nudges, chance) with `add_pattern_region`; a drum or percussion kit is a `"sampler"` instrument
with a zone on each note, tuned to the song's key where it has a pitch. Write the whole song,
not one loop: plan the sections and where it builds and lets go, then write each one, changing
what plays, how dense and how loud, its fills and turnarounds, its answers to what came before.
Give it a feel: decide the swing, lean the offbeats, vary every velocity and nudge timings a
little, accent and ghost like a player. Make the bass sit with the kick and lead into the next
chord; voice chords so they move little from one to the next; build melodies from a motif you
repeat, vary and answer. `copy_bars` reuses bars you wrote; change them where they land. The
song's chords are its chord track (`set_chords`); the critic checks parts against it.

- `set_genre` `{"genre": "lofi"}`: what the song is, in any words. If it names one of the
  person's own playbooks (`pina playbook` lists theirs; there may be none), sounds and the
  critic's targets follow that playbook. Pina Studio has no playbooks of its own: use one only
  when the person asks for it.
- `set_chords` `{"chords": "i9 | VImaj9 | III | VII7", "start": 0, "bars": 8}`: the chord track.
  Chords are symbols (`Am9`, `Fmaj9`, `E7b9`, `C/E`, `F#m7b5`, `Bbsus4`, `G13`) or roman numerals
  in the song's key (`ii9 V13 Imaj9`, `bVII`, `iiø7`, `#iv°7`; in a minor key the degrees are the
  natural minor's). Bars are split by `|`; chords in one bar share it. It repeats to fill `bars`
  (one pass by default) from `start` (ticks), replacing what was there. Without `chords`, clears.
  Answers the chords bar by bar. It plays nothing: write the parts on it.
- `copy_bars` `{"from": 0, "bars": 8, "to": 30720, "tracks": [0, 1]}`: copies bars of tracks
  (all by default) to `to`, replacing what's there; the chords come too.
- `transition` `{"at": 30720, "kind": "build", "length": 2}`: into the downbeat at `at`:
  `riser` (noise and a tone sweeping up, `length` bars, 2 by default), `reverse` (a reversed
  cymbal swelling into it, 1 bar), `impact` (a low boom on it), `downlifter` (a sweep falling away
  from it), `gap` (everything but the FX stops for `length` beats, 1 by default) or `build` (a
  riser, a beat's gap and an impact). Sounds are made at the song's tempo and go on an `FX` audio
  track (made if there's none). Fills and rolls are notes: write them.
- `add_modulator` `{"track": 1, "modulator": {"type": "lfo", "shape": "sine", "rate": {"beats": 0.5}}}`:
  a modulator on a track, eight at most, that settings follow as routes say (`modulate`). Its
  `"type"`: `"lfo"` (`"shape"`: sine, triangle, saw_up, saw_down or square; `"rate"`
  `{"beats": n}` in time with the song, or `{"hz": n}`; `"phase"` 0 to 1; `"unipolar": true` for
  0 to 1 rather than -1 to 1), `"steps"` (`"values"` 0 to 1, a step each `"rate"`; `"glide"` 0 to
  1), `"envelope"` (`"attack_ms"`, `"decay_ms"`, `"sustain"`, `"release_ms"`: set off by the
  track's notes), `"follower"` (`"source"`: the track it follows, by its place; `"attack_ms"`,
  `"release_ms"`: its level, for sidechain pumping and ducking), `"random"` (`"rate"`,
  `"smooth"`) or `"macro"` (`"value"` 0 to 1).
- `set_modulator` `{"track": 1, "index": 0, "modulator": {"type": "lfo", "shape": "square", "rate": {"beats": 0.25}}}`:
  one replaced (its routes kept unless it brings its own); `remove_modulator` `{"track": 1, "index": 0}`.
- `share_modulator` `{"aux": 2, "modulator": {"type": "macro"}}`: a modulator a stack shares (an
  lfo, steps, a follower, random or a macro; eight at most), answering `{"index": 0}`: every track
  in the stack gets a `{"type": "shared", "modulator": 0}` modulator that plays it, which
  `modulate` routes to that track's own settings, so one macro or LFO moves the whole stack.
  `set_modulator` on a shared one changes the stack's for every track; `unshare_modulator`
  `{"aux": 2, "index": 0}` takes it off the stack and every track in it.
- `modulate` `{"track": 1, "modulator": 0, "target": "pan", "depth": 0.8}`: a modulator moving a
  setting by `depth` in its units (0 takes the route off): `"volume"` (dB), `"pan"` (-1 to 1),
  `{"send": 0}` (dB), `"low_cut"` (the EQ's first low cut; octaves), `{"eq_gain": 1}` (that EQ
  band's gain, by its place; dB), `{"eq_freq": 1}` (its frequency; octaves),
  `"comp_threshold"`, `"comp_makeup"`, `"sat_drive"` (dB), `"sat_tone"`, `"sat_mix"`,
  `"limit_gain"`, `"limit_ceiling"` (dB),
  `{"sampler": "cutoff"}` (octaves; also `resonance`, `filter_env`, `transpose` in semitones,
  `volume` in dB, `vibrato` in cents, `lfo_filter`, `tremolo`, `drive`, `position`, `speed`,
  `grain_size` in octaves, `spray`, `fm`, `sub`), or `{"plugin": ["instrument", 12]}`
  (a parameter by id, 0 to 1; `{"insert": 0}` for an effect's), or a step pattern's sound by its
  row: `{"sound": [2, "velocity"]}` (also `"pitch"` in semitones and `"chance"` in %), worked out
  for each step where it falls in the song (so an LFO, steps, random or a macro, not an envelope
  or a follower): an LFO on the hats' velocity, random on the snare's pitch. A follower ducking a bass under
  the kick: `{"target": "volume", "depth": -18}` on the bass, its follower's source the kick.
- `set_reference` `{"reference": {"file": "/path/song.wav", "lufs": -9, "crest_db": 10, "spectrum": [-3, -4, -8, -14, -18, -20], "width_db": -10}}`:
  a finished song the mix aims at instead of general targets (or the person's chosen playbook's); the `reference` tool measures
  one and sets it. Without `reference`, none.

## Listening

An agent can't hear, so Pina Studio measures:

    pina analyze <project>          the mix, then each track alone
    pina analyze <file.wav>

Each gives the length, peak (dBFS) and RMS per channel, integrated loudness (LUFS, as
ITU-R BS.1770) and the loudest 3 seconds' (`loudest_lufs`), crest (peak over RMS: small is
squashed), clipped samples, how the energy splits between lows (< 250 Hz), mids and highs
(> 4 kHz) and, finer, `spectrum_db` (sub < 60 Hz, bass, low mid, mid, presence, air > 8 kHz),
all in dB below the total; stereo `correlation` (1 mono, below 0 cancelling in mono) and
`width_db` (sides against middle); and the `notes` sounding, each pitch class's share of the
tonal energy, with the key they fit best and how well (`key_fit`, above 0.7 is clear). A
project's analysis also has each section, marker to marker, so a hook can be compared with a
verse. `pina render <project> <out.wav>` bounces it to listen to.

`pina listen <project> [--track N] [--bars 9-16]` (MCP: `listen`) hears each track alone, as
the sample library hears a sample: what it sounds like (the model's guess: `pad 70% · strings
20%`), its character (dark, punchy, wide, distorted, lo-fi…), attack and decay, brightness,
spectrum, width, loudness and key, against the part it plays; with what's wrong: a plugin left on
its default patch, a sound unlike its name or part (strings that sound like a synth, a bass for
low strings), a slow-attack patch on a part that wants its notes on time, a fader doing the
sound's job, rumble under a part, a wide low end, a loop in another key. `pina sounds <role>
[--genre g] [words]` finds presets by auditioning them, so check a sound with `listen` after
choosing it.

`pina params` and `pina presets` answer from the app when it has the project open, and
otherwise load the one plugin asked about.

If the app is running: `pina play`, `pina stop`, `pina locate <project> <tick>`,
`pina save`, `pina undo` and `pina redo` (each with the project) drive it.
