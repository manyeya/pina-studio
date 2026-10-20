# tempo

A DAW with Logic Pro's structure and FL Studio's speed, written in Rust: gpui for the UI, VST3,
CLAP, VST2 and Audio Unit plugins, and agents as first-class collaborators. macOS first.

## Layout

- `crates/engine`: no UI. The project model (`project.rs`), the engine that plays and bounces it
  (`engine.rs`), the built-in synth, and plugin hosting (`plugin/`: one interface, with
  `vst3/`, `clap.rs`, `vst2.rs` and `au.rs` behind it).
- `crates/engine` also has the live `player.rs` (keeps the engine in step with edits) and
  `session.rs` (an open project and its undo history).
- `crates/engine` also has the built-in drum kit (`drums.rs`), the channel EQ (`eq.rs`), the
  compressor and saturator (written in Faust, in `faust/*.dsp`, compiled to Rust in `src/faust/`;
  `dsp.rs` runs the three), keys, scales and note edits like quantize and legato (`music.rs`), MIDI input
  (`midi.rs`), audio recording (`record.rs`), the plugin scan (`scan.rs`), edits as data
  (`commands.rs`, described in `commands.md`) and measuring audio (`analysis.rs`).
- `crates/engine` also writes music for agents: `theory.rs` (chords from symbols and numerals,
  progressions by genre and mood, voice-leading, basslines, melodies, genre grooves),
  `compose.rs` (the song's chord track, and the parts written on it), `arrange.rs` (a loop made
  into a song, copied bars, transitions with their sounds synthesized), `playbook.rs` (genres:
  tempo, swing, parts, sounds, sections, mix targets measured from released songs) and
  `critique.rs` (a producer's notes, from the notes and from a render).
- `crates/engine` also takes finished songs apart (`study/`): frames of onsets, pitch classes
  and timbre (`features.rs`), tempo and beats (`beats.rs`), key and chords (`harmony.rs`),
  sections and their names (`form.rs`), the drums' groove (`groove.rs`) and lyrics' rhymes,
  syllables and hooks (`words.rs`); `examples/study_eval.rs` measures its tempo and key against
  labelled loops.
- `crates/app`: the gpui UI and the `tempo` binary: `workspace.rs` (the window, shortcuts, drags),
  `topbar.rs` (transport and display), `inspector.rs` (the Inspector, left of the tracks),
  `chat.rs` (the Agent panel) with `agent.rs` (the agents, run headless for it and `tempo ask`),
  `palette.rs` (sounds, instruments and effects picked for the agent)
  and `producer.rs` (its song strip, producer's path, critic, undo for a turn and selection),
  `sampler.rs` (the Sampler editor), `genres.rs` (the Playbooks page, where genres are read and changed), `study.rs` (the Song
  Study page) with `study_cli.rs` (studies kept, and lyrics, year and genre looked up),
  `looks.rs` (track colours and icons, and their picker), `catalog.rs` (the Browser's devices; `browser.rs` the panel itself and its sounds),
  `projects.rs` (songs: the Songs page, the new-song sheet, opening another in the window),
  `tracks.rs` (tools, markers, ruler, headers, regions), `automation.rs` (automation: its lanes,
  and what moving an automated control does), `editor.rs` (the
  editor under the tracks) with `pianoroll.rs`, `steps.rs` (the step sequencer), `score.rs`,
  `takes.rs` (take folders), `liveloops.rs` and `browser.rs` (the sample browser, a page of
  its own (⇧F) and a compact list beside the tracks to drag from (F): search, filters, the sound
  map, kits, adding samples to the song); `mixer.rs`, `strip.rs` (channel strips),
  `record.rs` (recording takes), `menu.rs`, `text.rs` (the text field names are typed in),
  `theme.rs` (colors, Geist, icons, controls),
  `server.rs` (the project's socket), `cli.rs` (the commands for agents), `produce.rs` (the
  critic, playbooks, chords, references, sounds, mixing and mastering for agents), `mcp.rs`, and
  `tests.rs`, which drives the window headless with simulated mouse and keys.

## Run

    cargo run -p tempo -- examples/midnight-drive           # open a project in the app
    cargo run -p tempo -- examples/demo                     # a smaller one, with audio
    cargo run -p tempo -- render examples/demo demo.wav     # bounce a project
    cargo run -p tempo -- play examples/demo                # play it through the default output
    cargo run -p tempo-engine --example play_vst -- "/path/to/Some Synth.vst3"

`play` sets the output device to the project's sample rate, as Logic does. `play_vst` loads a
plugin, opens its editor and plays an arpeggio through it.

`scripts/bundle.sh` builds `tempo.app` (the release binary, its icon, an Info.plist that asks for
the microphone to record, signed ad hoc) and installs it in `/Applications`, or the folder given.
The icon, a metronome on a dark squircle on Apple's grid, is drawn by `scripts/icon.py`
(`crates/app/icon/tempo.svg`) and packed into `tempo.icns` by `cargo run -p tempo --example
icon`; macOS 26 draws it in its own glass, not in a gray box.

`examples/midnight-drive` was made entirely with `tempo do` commands, as an agent would make it.
So is Corner Store Blues, a 95 BPM boom-bap beat on real samples (MusicRadar's free 90s Hip Hop
pack, downloaded on first run: they can't be passed on, so they aren't here) with Surge XT and
Dragonfly Room Reverb installed:

    cargo build -p tempo && TEMPO=target/debug/tempo python3 examples/corner-store-blues/build.py

It goes in `~/Music/tempo/Corner Store Blues`: sampler kits on swung step patterns with aliases,
a Moog multisample bass and an 808 on the keys loop's roots, the loop flexed to half time in the
break, a Surge XT pad whose filter opens through each hook and a gliding sine whistle, drums
glued in a stack, two reverb sends, Live Loops scenes, and the master clipped by the saturator
to about -13.5 LUFS.

In the app, mostly as in Logic:

- Space plays and stops, return goes to the start, R records onto the armed track: the selected
  one, until a track's R button arms another (click it again to disarm). An instrument track
  records MIDI from any MIDI keyboard or musical typing; an audio track records the input its strip
  picks (any channel or pair of the default input device). C turns the cycle on and off (with a
  region selected, it cycles that region), K the metronome; the cycle can also be swept out (or
  clicked on and off) in the ruler's top half. With the cycle on, the playhead stays in it: play
  from before it runs into it, from after it (or a cycle drawn behind the playhead) jumps to its
  start.
- Song plays the arrangement; Pattern (as in FL Studio) loops the selected region alone. Sweeping
  a cycle goes back to Song.
- To write notes: double-click an empty spot on an instrument track (a four-bar region there, or
  the cycle's, opens in the piano roll), or just draw in the piano roll: with no region there it
  makes one round the note, and a note past a region's end grows it. A new instrument track says
  so in its lane. An instrument or effect added from the Browser opens its window, a new track
  named for it.
- The display shows the position, the time, the tempo (drag it), the time signature (pick one:
  4/4, 3/4, 6/8, 7/8…) and the key: worked out from the notes, or chosen from its menu. Choosing
  one moves the music into it (each step of the scale onto the new scale's, so major to minor
  changes the mode; drums and audio stay), unless the menu's last row is turned off. The piano
  roll shades the rows out of the key and marks it down its keyboard, notes are named as the key
  spells them, and the score shows its signature. The window narrows to 1120 points with all of
  it showing.
- In Pattern mode, selecting another region plays it at once, from the same place in its loop.
- Markers sit over the ruler: double-click to add one (and type its name), drag to move it, click
  to go there, double-click to rename it, right-click for a quick name or to delete it.
- Double-click a track's name, or a strip's name plate in the mixer, to rename it; a region's name
  is in the inspector. Enter keeps it, escape doesn't, clicking elsewhere keeps it too.
- Over the track headers: the tools (pointer; pencil, which draws a region in an empty lane and a
  note in the piano roll; scissors, which cut a region or note where clicked; eraser), and what
  edits snap to. The fifth button (or T) is Logic's Tool menu, with the rest: Text names a region
  where it is; Glue joins a region to the next on its track (MIDI always, audio cut from one
  take); Solo plays a region alone while held, from where it's held; Mute mutes or unmutes one;
  Zoom zooms in where clicked, out with ⌥, or fills the window with the bars dragged across;
  Fade drags an audio region's fade in from its start or fade out from its end (equal-power
  curves, drawn on it); Flex makes dragging a region's end stretch it, as ⌥ does. The status bar
  says what the tool in hand does.
- A track's header is a console strip: its colour down its edge, its icon (click for its colour
  and icon), number, name and what it plays; then a raised strip with lit M S R buttons, an LED
  meter (left over right, its peak held), a fader with a scale and a cap like the mixer's, an LCD readout
  and a pan knob with an LED ring. The strip keeps its size however wide the column is. Drag the
  fader or knob (the knob up or right to turn it right; ⇧ for fine control, even mid-drag), swipe
  sideways over either, double-click either (or the readout) to type a value (-6, L30, C),
  ⌥-click for 0 dB or the middle; the readout shows the pan while the knob turns. A short track
  keeps the buttons and LEDs. Drag a header's bottom edge for that track's height, and the
  column's right edge (or the side panel's) for its width; double-click an edge to put it back.
- Pans as Logic's: right-click a pan knob for Balance, Stereo Pan (each side placed, ⌘-drag the
  knob to spread them, or swap them) or Binaural (round your head, for headphones), and the
  song's pan law (0, −3, −3 compensated or −6 dB). Binaural opens the Binaural Panner (again from
  the 3D button beside the knob, or double-clicking it): the sound round the listener's head in
  3D, all the way round, above or below, near or far. Drag the sound round the head, ⌘-drag it up
  or down, drag the space to look from another side, or use Front, Left, Right, Behind, Above,
  Below and the sliders. It pulses with the track's level. The pan knob turns it round the head
  (so pan automation circles it), and the far ear hears it later, quieter and darker; behind and
  below sound duller, above brighter, further off quieter.
- Right-click for more: a track's header (new tracks, one with the same channel strip,
  duplicating, renaming, colour and icon, mute, solo, record, height, pan mode, moving it,
  stacking, Bounce in Place, Export as Stem, deleting), a region (cut, copy, paste, repeat, split,
  mute, loop, rename, alias, open it, quantize, into a step pattern or MIDI, bounce, delete) or an
  empty stretch of a lane (paste, a new MIDI region or step pattern, a marker, an audio file).
- ⌘T splits the selected region at the playhead, delete deletes it, ⌘C ⌘X ⌘V copy, cut and paste
  regions (at the playhead, on the selected track), ⌘R repeats one after itself and ⌃M mutes it,
  ⌘Z and ⇧⌘Z undo and redo, ⌥⌘N adds a track and ⌘⌫ deletes the selected one, ⌘S saves.
- X shows the mixer, P the piano roll (or double-click a MIDI region), A automation lanes, I the
  Inspector, F the Browser beside the tracks (on Sounds: click a sound to hear it, ↑ ↓ for the
  next, drag it onto a track or double-click it to add it, More Like It for what sounds like it)
  and ⇧F the sample library's page (filters, the sound map, kits, the details; drag a sound out of it and the tracks come
  back to drop it on); ⌘K turns on musical typing (A to K play a C major scale from C3, W E T Y U the
  sharps, Z and X change octave). ⌘B exports the song to `Bounces/` in the project.
- The Inspector, left of the tracks, shows the selected region with its playback settings, as Logic's:
  Loop (it plays over and over until the next region on its track, its repeats drawn after it),
  and for MIDI and patterns Quantize, Transpose and Velocity, which change what plays but not
  the notes ("Apply to notes" writes them in). Then Smart Controls (a plugin instrument's filter
  and envelope, else the channel's EQ and compressor; turning them is an undoable edit) and the
  channel strips.
- The Browser, right of the tracks (F): one panel and one search for everything that goes in a
  song, its tabs All, Instruments, Effects, Sounds (the sample library: one-shots and loops, what
  fits the song, its categories); what's typed follows from tab to tab. tempo's own
  instruments and effects and every plugin installed, each once however many formats it comes in
  (CLAP, VST3, AU, VST2 shown as badges, the best used), in the categories the plugin says it
  belongs to (VST3 subcategories, CLAP features, an Audio Unit's type, VST2's category; its name
  where those are silent): Synths, Samplers, Drums, Keys, Orchestral, EQ, Dynamics, Reverb,
  Delay, Distortion, Modulation, Filter, Pitch, Stereo & space, Utility. Words that mean a thing
  find it ("echo" a delay, "space" a reverb, "glue" a compressor); sounds from the sample library
  and songs come up too. Favourites (★), what's in this song and what's used lately come first. A
  plugin's arrow shows its preset files. Click puts it on the selected track (an instrument in
  place of its own, an effect after its others), double-click or ⇧↵ on a new track, or drag it
  onto any track; ↑ ↓ and ↵ work from the search.
- Songs (⌘O, or the song's name in the top bar): every song in `~/Music/tempo`, each card's cover
  its arrangement in miniature, coloured by what each track plays, with its tempo, key, tracks
  and length. Open one in this window (asked first whether to save the one open), or from its
  card's menu rename, duplicate (a clone, taking no space until it changes), show in the Finder,
  or move to the Trash. New Song (⌘N): a name, tempo, key and time, and a start: empty, or a
  beat, band, lo-fi or electronic set of tracks. The top bar's menu has the songs opened lately.
- Every track is coloured and drawn by what it plays, worked out from its sound (its samples' and
  audio's analysis), the plugin it plays, its name and its notes: drums warm, bass violet, keys
  teal, vocals pink, effects grey, each track a shade of its family's colour so they read as one
  and each as itself. Click a track's icon for the picker: its colour from a palette (hover to
  try it on the track; ⌥-click, or "Shade all", colours its whole family in shades of it), and
  its icon from 54 instruments and marks drawn for tempo, searchable, those its sound suggests
  first. Auto goes back to what it plays saying. Regions and channel strips follow.
- ⌘L, or the agent's mark in the top bar, opens the Agent panel beside the tracks: ask for
  anything and watch it work. Its thinking, each step (reading the song, finding samples, every
  edit, listening) and what it says show as they happen, and each edit lands in the tracks as
  it's made, outlined, undoable on its own. The agent's name at the top picks another (Claude
  Code or Codex, with their logos, each saying whether it's signed in and on what), and beside it
  its model and effort: Claude Code's Fable, Opus, Sonnet or Haiku and effort from Low to Max;
  Codex's models as Codex lists them, each with the efforts it takes; or its own settings. Picked
  ones are kept for every song and `tempo ask` uses them too; changed mid-conversation, the
  conversation carries on with them from the next question. Edits by any other agent show there
  too.
- Sounds, instruments and effects for the agent: "Pick sounds · instruments · effects" atop the
  Agent panel's message box (the + by "Only these" once there are some) opens the Browser there
  to pick from (＋ on each sound, plugin or tempo device, or "Agent" under the sound picked; on
  the sample page too), and audio files dropped on the Agent panel from the Finder are picked as
  well. They show in the message box: click a sound to hear it, a tag to give it a part (kick,
  snare, hats, 808, bass, chords, pad, lead… or any; an effect's is what it goes on), × to take
  it out; "Only these" asks the agent to use these and no others of each kind picked. They're
  kept with the song, and the agent is told them with the next message after they change, each
  instrument and effect with how to put it in (and its `palette` tool lists them with what each
  is and whether it's in the song yet).
- Drag audio files (WAV, AIFF, FLAC, MP3) in from the Finder, or sounds from the browser, and
  they land on the timeline where they're let go: on the audio track there, or on a new audio
  track (under an instrument track, or under the tracks); several files, a track each. A sound
  dropped on a sampler track becomes a pad. A file the library knows is fitted to the song's tempo
  and key, as from the browser.
- Drag regions to move them (across tracks too) and their edges to trim; option-drag one to copy
  it, option-shift-drag for an alias: a linked copy, its name in italics with a link, that plays
  the same notes, so editing either edits both (its place, length and playback settings are its
  own; "Make unique" in the inspector, splitting it or trimming its start unlinks it).
- A region's end works as Logic's: its bottom half trims; its top half (a loop sign shows on
  hover) loops it out to wherever it's dragged, and the last repeat's drags the loops back; and
  option-dragging it time-stretches the region: audio flexes (slower or faster, same pitch, by
  Signalsmith Stretch), a MIDI region's notes and controllers spread out or close up, and a step
  pattern's steps get longer or shorter.
- Bounce (⌘B, or Export in the control bar) opens the Bounce sheet, as Logic's Bounce and FL
  Studio's Export: the mix, each track as a stem (through the master or not), or each marked
  section as its own file; over the song, the cycle or any bars; as WAV, AIFF, FLAC, MP3 (LAME,
  built in), AAC or ALAC (macOS's encoder), and the notes as a MIDI file, several formats at once;
  16-, 24- or 32-bit float at 44.1 to 96 kHz (rendered at that rate, not converted); as it plays,
  to a peak, or to a loudness (Club −9, Streaming −14, Apple −16, Broadcast −23 LUFS) under a
  −1 dB ceiling; with the tail its reverbs and delays leave (until it's silent, ten seconds at
  most); TPDF dither. Presets: Master, Streaming (WAV and MP3 at −14), CD (16-bit 44.1), Stems,
  Demo MP3, MIDI. It shows how many files and how big before it starts, each file's progress as
  it bounces, then each one's loudness and peak (and why, if its peaks kept it from the target),
  to play or show in the Finder. Right-click a track's header for Bounce in Place (the track
  rendered to audio on a track under it, through its own plugins and sends, the original muted;
  one undo puts it back) or Export as Stem.
- The piano roll works as Logic's and FL Studio's do. Pick notes by clicking (shift adds), by
  dragging a band over the grid, ⌘A for all, or ⌘-click a key for every note of its pitch; drag
  them together, by either edge to resize, ⌥-drag to copy. ↑↓ move them a semitone (⇧ an octave),
  ←→ a grid step (⌥ finer); ⌘C ⌘X ⌘V copy, cut and paste at the playhead, ⌘D duplicates them
  after themselves, ⌫ deletes, Q quantizes; right-click for the rest. Double-click (or click with
  the pencil) writes a note at the length last picked or stretched, or a chord with a stamp (triad,
  7th, 9th, sus, power, octave: stacked on the song's scale, so they stay in key); drag the pencil
  on to paint more. "In key" keeps drawn and moved notes on the scale; Velocity colours notes
  cool to hot. Each note shows its velocity along its foot. Under the notes a lane for velocity
  (drag a dot, the picked notes move with it, or draw across) or a controller: the mod wheel,
  pitch bend, expression or the sustain pedal, drawn freehand, ⌥ to erase. Its tools work on the
  picked notes (or all): quantize (grid, strength, swing), humanize, legato, chop, arpeggiate,
  strum, glue, reverse, flip, softer and harder. Ghost shows another track's notes behind; Fit
  fits the notes' range; ⌥-scroll makes rows taller or shorter.
- Recording over a region of the same kind makes a take folder, as in Logic. Double-click it to
  see its takes; swipe across a take to comp that stretch from it, click one to use all of it,
  and Flatten to turn the comp back into plain regions.
- The Sampler tab (in the editor under the tracks, on a sampler track) is tempo's sampler, as
  Logic's Sampler, Ableton's Simpler and FL Studio's Slicex: its zones down the left (drop samples
  on it, or click; right-click removes one), the picked zone's waveform with its start, end and
  loop points dragged in place and its sample's other slices marked, the keys with each zone over
  its notes (click to hear), and knobs for the zone (notes, root, fine tune, level, pan,
  velocities) and the instrument: amp envelope, filter (low-, high- or band-pass, with its own
  envelope and key tracking), transpose, glide, LFO (vibrato, wah, tremolo) and volume. A zone
  plays one-shot or gated, loops (forward or ping-pong) and reverses; zones split by velocity are
  layers, and zones on the same notes take turns (round robins). Mono and Legato play one note at
  a time, gliding. Slice… cuts a loop across the keys from C1 at its hits, or into equal parts.
  Turning a knob while it plays changes what's playing, without a reload.
- Automation (A shows it): each track has lanes under it, a parameter each: volume, pan, mute,
  its sends, or any parameter of its plugins (type to search; those with automation are marked).
  + adds a lane, × takes one away; each shows its value at the playhead and a scale. Click for a
  point (on the line if near it), drag points (selected ones together, on the grid unless ⌘),
  drag the line between two points to move both, drag the ◇ halfway along a line to bend it
  (double-click it straight), drag across a lane to select (⇧ adds), ⌥-click or double-click a
  point to delete it, Delete takes out the selection, ⌘C ⌘V copy and paste it. The pencil draws
  a lane freehand; the eraser takes out what it's dragged across. Right-click a point for its
  line's shape (straight, an S that eases out and in, slow or fast start, a step), or a lane for
  shapes drawn over the selection, the cycle or that bar, a cycle every grid step (sine,
  triangle, saws, square, random, ramps), and to thin or clear it. Smooth (on either menu, Smooth
  More for more) rounds off the selection's corners and wobbles, or the whole lane's, its ends
  staying where they were; it stays selected to smooth again. Mute is all steps.
- Every control shows its automation: the header's fader, pan and readout, the strips' faders,
  knobs, sends and mute buttons, Smart Controls and plugin editors; an automated readout lights
  in its mode's colour. The mode is on the lane and on each track's strip, as in Logic: Read plays
  it, and a control held while it plays takes over until let go; Touch records a control while
  held, then ramps back to the line; Latch records it until stop; Trim moves the line up or down
  by as much as the control moves while held, keeping its shape; Write records every automated
  control from play, over what was there, then turns to Latch; Off ignores it. Moved with the song
  stopped, an automated control moves its whole line, sets a point at the playhead, or changes it
  until you play (Settings → Automation, with the lanes' height, snapping, thinning what's
  recorded, and automation going with regions when they move).
- Drag faders, pan knobs and the tempo; option-click a fader or knob to reset it. Click or drag
  in the ruler to move the playhead; ⌘-scroll zooms.
- The "+" over the track headers (or ⌥⌘N) opens the New Track palette: type to search every
  instrument installed (and tempo's), their presets (their own programs and preset files), kits
  made from your sample library's packs, and single sounds (an 808, a bass note) played across the
  keys on a sampler; ↑↓ pick, Enter adds, ▶ plays a sound, ☆ keeps an instrument among the
  favourites. With nothing typed it shows the kinds of track as tiles (Instrument and Drums narrow
  the list to theirs; Sampler, Audio and Beat, a kit with a pattern open in the step sequencer, add
  one at once), then what's been used lately and the favourites. ⌥⌘P makes a step pattern region on the
  selected instrument track (at the first free bar from the playhead); double-click one for the
  step sequencer, as Maschine's and Push's: the sounds as pads (tap one to hear and pick it,
  right-click for its menu, an empty pad adds a sound) beside every sound's steps in its own
  colour. Click a step on or off and drag across to paint more (or rub them out), option-click for
  an accent (a white dot; again to take it off); a step is as bright as it hits hard, and the
  picked sound's velocities open under its steps, drawn with the mouse. M mutes a sound. Under the
  grid, the picked sound's actions in sight: fill every 1, 2, 4 or 8, shift ← →, reverse,
  randomize, Euclid (so many hits spread evenly), a length of its own (three steps against
  sixteen: a polymeter, its repeats drawn faint), clear, remove. Each step has its own settings
  too, as Logic's edit modes: in the lane under the picked sound, Velocity, Chance (it plays that
  often, rolled afresh each time round), Pitch, Repeat (a ratchet of up to four hits), Gate and
  Nudge, drawn a bar a step; the grid marks them (a ratchet split, a chance faded, a pitch moved).
  MODULATE above the grid: the track's modulators, to drag onto a sound's lane (an LFO swaying the
  hats' velocity, random on the snare's pitch, its dots showing where each step lands) or onto
  any knob of the track; + LFO, + Random; and + Parameter row: a row under the sounds that steps
  any setting in time with the pattern (a filter, the volume, a plugin's knob), as Logic's
  automation rows, picked from a menu and painted. The editor's bar sets the
  pattern's length (8 to 64 steps, repeating the groove as it grows), its rate (1/8, 1/16, 1/32)
  and swing; To MIDI turns it
  into notes for the piano roll, and on a track with MIDI (the drums an agent wrote) the step
  sequencer makes a pattern of it in a click. Patterns loop over their region.
- Modulation (its tab in the editor, or a track's right-click menu): a track's modulators as
  cards, each moving as it plays in its own colour: an LFO (sine, triangle, saw up or down,
  square; in time with the song, 4 bars to 1/32 with dotted and triplet notes, or free in Hz; its
  phase; ± or 0 → 1), Steps (values you paint, 4 to 32 of them, a glide, dice for random ones),
  an Envelope its notes set off (attack, decay, sustain, release), a Follower of another track's
  level (sidechain pumping and ducking: the bass under the kick), Random (stepped or smooth) and
  a Macro to turn by hand. Drag a card's ↗ Map onto any knob or fader of its track to have it
  move it: the header's volume and pan, the strip's pan and sends, Smart Controls, the EQ's low
  cut and each band's gain and frequency (from the card's menu), the compressor's threshold and makeup, the saturator's drive, tone and mix, the
  Sampler's cutoff, resonance, filter envelope, transpose, level, vibrato, LFO filter and
  tremolo, and a plugin's parameters. A modulated knob wears a ring in the modulator's colour,
  its range, and a dot where it has it now. Each card lists where it goes: drag a route sideways
  for how far (⇧ finer), ✕ to take it off; click a card to pick it, and dragging any knob it moves
  sets its depth instead. Modulation plays in bounces too, and agents use it as well
  (`add_modulator`, `modulate`).
- Settings (⌘,, the tempo menu, or the top bar's sliders), as Logic's, a section at a time: Track
  headers (numbers, icons, what each plays, level meters, fader and pan, record buttons, as
  Logic's Track Header Components; and how tall tracks are, Small to Huge), Regions & grid (names,
  contents, bright colours, bar shading, beat lines), Editing (the loop zone at a region's end;
  writing a region with a double-click), Transport (following the playhead; going back where play
  started on stop, as Logic's Return to Last Play Position) and Editor tabs (which show; a hidden
  one's shortcut still opens it). Kept for every song in
  `~/Library/Application Support/tempo/settings.json`.
- N shows the score: the selected MIDI region as notation on a grand staff, in time with the
  tracks; click a note to select and hear it, delete to delete it.
- L shows Live Loops: a grid of loop cells, a row per track and a column per scene. Click an
  empty cell to copy the track's selected region into it; click a cell to launch it (on the next
  bar, or at once, starting playback) and again to stop it; right-click to empty it; the buttons
  under the grid launch a scene. A launched cell plays instead of the track's regions.
- ⇧⌘D makes a summing stack of the selected track: a stack channel (in the mixer) it plays
  into, its name shown on the track. A strip's "→" row routes a track into any stack or aux, or
  back to Stereo Out.
- On a channel strip, click a plugin to open its editor; click an empty slot, or right-click any,
  to choose a plugin or the built-in Channel EQ, Compressor and Saturator (drive, tone, mix,
  output). The master takes plugins too, after its built-ins. In the mixer the selected channel's
  chain shows on the right: the EQ's curve, the compressor's knobs and gain reduction, the
  saturator's drive and curve.
- The Channel EQ is a Pro-Q style EQ in a window of its own (click a strip's EQ curve, or the
  mixer's; the channel name at its top switches to another's): up to 24 bands, each a bell, low or
  high shelf, low or high cut (6 to 96 dB an octave, or a brickwall), notch, band pass, tilt
  shelf, flat tilt or all pass; on the whole sound or its left, right, mid or side (the display
  draws those curves white, red, green and blue, the whole's yellow); gain-Q interaction; and
  dynamic: past its threshold (its own, following what it usually hears, unless set) it turns by
  up to its range, listening to what it covers, between two frequencies, or another track (the
  EQ's sidechain), its attack and release its own unless turned. A live analyzer shows what goes
  in and comes out, an output meter beside it. Hover the display for the band a click would make
  there (a cut at either end, a notch low down, a bell between), click to make it (double-click
  or ⌘-click once one's picked), or pull the curve to make one and drag it (a shelf from either
  end; ⌥ for a dynamic one). Drag a box to pick bands, ⌘-click to add one, ⇧-click a run; drag a
  dot to move all those picked (⇧ fine, ⌥ one way only, ⌘ their width); the wheel on one changes
  its width (a cut's slope), with ⌥ its dynamic range, with ⌘ its gain; ⌥-click switches it off,
  ⌘⌥-click changes its shape, ⌥⇧-click its slope; double-click it to type its frequency ("2k",
  "A4", "C#2+13"), gain ("2x") or width; right-click it, or the background, for the rest (split
  into left and right or mid and side, copy and paste, select all). The bands picked show their
  panel under them: shape, slope, frequency, gain with its dynamic range a ring round it, width,
  part of the sound, solo (hold it to hear what the band works on; drag it to move the band, up
  and down for the level), dynamic and its own settings, on/off and delete. The display's range
  grows as a band's dragged past it; the frequency scale zooms (drag it up and down), pans and
  shows notes (the piano, which snaps bands to them). Along the top: presets (tempo's, and yours
  to save), undo, A/B and full screen; along the bottom: the analyzer's settings (pre and post,
  range, resolution, speed, tilt, freeze), the character (clean, subtle, warm), bypass, and the
  output (level, pan left/right or mid/side, phase, gain scale, auto gain). Every band's
  frequency, gain and width can be automated, in the track's lanes or by moving it in Touch, Latch
  or Write. Its filters are matched to their analog shapes, so a bell or shelf near the top keeps
  its shape rather than cramping; bands glide and fade in and out without clicks, and cost
  nothing while flat. Songs made before open with their EQ as it was (a low cut, two bells and a
  high shelf, as bands). The Inspector's Low Cut, Presence and Air knobs turn the
first low cut, a bell round 3 kHz and the high shelf, adding them when first turned. "Send" sends the track to a
  bus, making an aux for a new one; drag a send for its level, right-click it to switch it before
  the fader (amber) or after, or to remove it. Meters are stereo, with peak holds.

Plugins are VST3, CLAP, VST2 and Audio Units. They're found in `VST3`, `CLAP` and `VST` under
`/Library/Audio/Plug-Ins` and `~/Library/Audio/Plug-Ins`, the folders in `TEMPO_PLUGIN_PATH`
(colon-separated), and, for Audio Units, wherever macOS has them registered. Each bundle is
first opened in a child process (`tempo __scan`), so a broken one can't crash the app, and the
results are cached in `~/Library/Caches/tempo/plugins.json`. A plugin slot's menu lists them all
(with their format, since one plugin is often installed in several) and the presets of the one
in it. Saving writes each plugin's settings to `Plugin States/` in the project. Plugin latency is
compensated, as Logic does. If the output device can't run at the project's sample rate (a
Bluetooth speaker at 44.1 kHz), playback runs at the device's.

Audio is stretched (Flex, for a tempo or a region's length) three ways, per region.
Polyphonic, the default, is Rubber Band's finer engine (R3, from Rubber Band 4, built in), run
offline over the whole file with its hits pinned to their times, and the recording itself kept
around every hit (the 25 ms before, where a phase vocoder smears a hit ahead of itself, and its
first 20 ms), so drums stay as sharp as recorded with no pre-echo, and chords and voices are
R3's; transposed, the recording around each hit moves in pitch as a sampler would, unless a
region keeps its formants (for a voice). Slicing cuts at the hits and plays each as recorded at
its new time; Speed is a turntable. Each can be warped: a selected audio region shows its hits
as handles along its top, and dragging one makes a warp marker there (on the sixteenth), the
hits either side pinned, so that beat moves and the audio around it stretches to fit, as Logic's
flex markers do; double-click a marker to take it out.
Stretches are made off the main thread, something standing in until they're done; loops from
the sample browser follow the tempo, and their previews play at it, in time with the song while
it plays. `cargo run --release -p tempo-engine --example stretch_ab -- <out dir> <loop.wav>
<bpm>` renders a loop each way (and with Signalsmith Stretch) to compare, and measures how sharp
and how on time its hits come out.

Builds need only the Command Line Tools: gpui compiles its Metal shaders at launch.

The compressor and saturator are Faust (`crates/engine/faust/*.dsp`), using its libraries: its
soft-knee compressor gain computer, and antiderivative antialiasing for the saturator, which also runs at twice the sample rate (a
half-band filter designed by `scripts/halfband.py`). The Rust Faust makes of them is checked in
under `crates/engine/src/faust/`, so building tempo doesn't need Faust; after changing a .dsp, run
`scripts/faust.sh` (with `faust` on the path, or in `~/.local/faust/bin`).

## Agents

tempo is made to be driven by agents as well as people. The quickest are built in, as modisa
knows them (ids, logos from modisa's logo font, brand colours): Claude Code and Codex, in the
Agent panel (⌘L) in the app, or from a terminal

    tempo ask [--agent codex] <project> "make the hook hit harder"

which prints the agent's words and steps as it goes (the agent last picked in the app by
default). Each runs headless, installed and signed in on its own account (Claude Code on your
Claude plan), with tempo's MCP server as its only tools: Claude Code with no built-in tools, other
MCP servers, settings or hooks; Codex with tempo's server in place of yours and its sandbox
read-only. One that isn't signed in says how to sign in rather than starting. When the app has
the project open, `tempo ask`'s edits land there live and its Agent panel opens to show the
conversation. (opencode isn't one yet: version 2 hands MCP tools to its model inside a single
code-running tool, so its steps can't show as tempo's.)

Any other agent drives tempo the same way Claude does. `tempo describe` prints every command an
agent can send, with examples. An agent can:

- Read a project with `tempo show <project>`.
- Change it one command at a time with `tempo do <project> '<json>'`.
- Measure it with `tempo analyze`, since it can't listen: loudness (integrated and loudest),
  peaks, clipping, crest, six-band balance, stereo correlation and width, and the notes sounding
  with the key they fit, for the mix, each section (marker to marker) and each track.
- Find samples that sound good together: `tempo samples find "dusty keys loop in C minor 95 bpm"`
  (or a JSON query), `tempo samples similar <file>`, `tempo samples kit "punchy" --for <project>`
  (a kit fitting the song's key and tempo, each pick going with the rest, with alternatives) and
  `tempo samples info <file>`. Every hit says why it fits ("in C minor", "stretches from 92
  BPM", "same pack as Kick_01"), how far to transpose it if a small move fits better, and, for a
  one-shot, the sampler zone that plays it in tune. `tempo samples put <file> <project>` puts one
  in the song made to fit: a loop stretched to its tempo and moved into its key, a one-shot as a
  sampler zone in tune.
- Play any plugin as a person would: find it with `tempo plugins`, set its parameters by name and
  as it shows them (`"param": "Filter Cutoff", "text": "850 Hz"`), list and load its presets and
  save new ones (`tempo presets`, `set_preset`), automate any parameter, send it controllers,
  pitch bend, pressure and per-note expression with the notes (`set_controls`), and feed a track
  into its sidechain (`set_sidechain`).

- Make songs that sound finished, as a producer would. An agent can't hear, and isn't a
  musician, so tempo does the parts it would get wrong:
  - `tempo playbook <genre>`: what a genre is (17 of them, lofi to amapiano): its tempo, swing and
    keys, its parts with the sounds they take and how to write them, its sections, progressions,
    and where its mixes sit (loudness and six-band balance, measured from released songs).
    The Playbooks page (File ▸ Genre Playbooks…, or a genre's Edit playbooks… in the Agent
    panel) shows each genre as its song's shape, and changes it in place: tempo range, swing,
    key, groove (its kick, snare and hats shown), harmony words (and the progression they find),
    the song map (sections as blocks as long as they are and as tall as they're loud, and a grid
    of which part plays in which, clicked on and off), the parts (sounds to look for, how to
    write them), the mix target (six bands dragged, and loudness) and notes for the agent.
    Duplicate makes a genre of your own; Reset puts tempo's back. Each change is saved as it's
    made, to JSON files, one a genre, in `~/Library/Application Support/tempo/playbooks/`
    (`tempo playbook --edit [genre]` opens them too); tempo's own are in
    `crates/engine/playbooks/`.
  - `tempo theory "A minor" dark trap`: the key's chords and progressions that suit the words.
    `set_chords` takes them as symbols (`Am9 | Fmaj9`) or numerals (`i9 | VImaj9`) and makes them
    the song's chord track; `write_chords`, `write_bass`, `write_melody` and `write_drums` write
    parts on it that are in key and on the chords, voice-led, with a player's rhythm and a
    genre's groove (swing, ghosts, rolls, fills); an 808 can follow the kick.
  - `tempo sounds <role> --genre trap`: real sounds for a part instead of the built-in sine:
    every installed instrument's presets that suit it (each instrument asked for its own, a few
    from each, with the commands that load them), and samples.
  - `tempo critique <project> [--quick]`: what's wrong, worst first, each with where and the
    command that fixes it: wrong notes and semitone clashes, a bass off its chords, kick and 808
    flamming, robotic velocities, missing swing, placeholder sounds, a song that never changes or
    a hook weaker than its verse; and from a render, clipping, loudness, tonal balance against the
    genre (or a reference), low end in the wrong parts, parts too loud for their role, stereo.
  - `arrange` lays a loop out as a song (the genre's sections, or given ones: parts by role or
    track, marked, quieter sections lower), `copy_bars` copies, and `transition` puts a riser,
    reversed cymbal, impact, downlifter, drum fill, gap or a whole build into a section.
  - `tempo mix <project>` sets each part's channel EQ, compression, saturation, pan, reverb send
    (a shared reverb aux, made once) and level, measured alone; `tempo master <project>` soft-clips
    and limits the master to the genre's loudness (or a reference's: `tempo reference <file>
    <project>`), measuring until it's there.
  - `tempo analyze` and `tempo render` take `--bars 9-16` to hear part of a song.
  - `tempo bounce <project>` (and the agents' `bounce` tool) bounces as the Bounce sheet does:
    `--stems` (`--tracks 0,2`, `--master` through the master) or `--sections`, `--formats
    wav,mp3,flac,aiff,aac,alac,midi`, `--bits`, `--rate`, `--lufs -14` (`--ceiling -1`) or
    `--peak -1`, `--kbps`, `--no-tail`, `--bars`, `--out`, `--name`; it answers with each file
    and its loudness and peak.
  - `tempo study <file> [--summary] [--playbook] [--offline]`: a finished song taken apart from
    its audio. Its identity (tags, else its file name; online, its year and genre from
    MusicBrainz), tempo, meter, key (and tuning, and sections in another key), chords beat by
    beat with numerals, its form (intro, verses, pre-chorus, choruses, bridge, outro, found from
    what repeats and named by it, with bars, energy, loudness and the chord loop each goes round;
    a song that never repeats is named by its energy: build, drop, breakdown), its groove (kick,
    snare and hats on a sixteen-step bar, swing, syncopation, the nearest of tempo's grooves),
    what plays in it, its mix and dynamics overall and section by section, the playbooks it's
    nearest, and its lyrics from LRCLIB (timed into their sections where they can be), with each
    line's rhyme and syllables, the rhyme schemes, the hooks, who sings to whom and the words it
    keeps coming back to. Only its title, artist and length are sent anywhere; never its audio.
    Studies are kept in `~/Library/Caches/tempo/studies/`. `--playbook` saves a playbook made from
    it, to write songs like it; agents have it as the `study` tool, and read its theme, story and
    imagery from the lyrics themselves.

The Song Study page (File ▸ Study a Song…, or the song menu) shows a study: the song's name and
vitals over its shape (a stroke a bar, as loud as it is, coloured by section), its form (a block a
section, the same colour for the same music, each opened up to its chords, numerals, parts and
lines), its harmony on the circle of fifths, its groove as steps, its parts and where they play,
its loudness, spectrum (against its nearest genre's target) and dynamics, its lyrics with their
rhymes and hooks, and its nearest playbooks. Drop a song on the page, or click, to study one. From
a study: Start a song like this (a new song at its tempo, key and meter, its playbook as the genre
and its mix as the reference), Save as playbook, Mix reference (this song's), and Ask the agent
what it means (its theme and message, metaphors and imagery, and why it works).

The Agent panel is a producer's desk as well as a conversation: under its header the song at a
glance (its genre, picked from the playbooks, its key, tempo and length); a new conversation
offers the stages the song hasn't had yet (genre, chords, a loop, arrangement, mix, master), a
click each, or for a finished song what it wants next; in the composer the critic, tempo's notes on the song kept
up to date as it changes, each with a Fix to ask for (or Fix all), and Listen to the mix for the
notes a render gives; after an answer, Undo this turn puts the song back as it was when it was
asked (one step, so ⌘Z puts it back again); and what's selected (a region and its bars, or a
track) goes along with what's asked, unless its chip is turned off.

When the project is open in the app, commands go to the app over a socket. Each lands on screen as
one undoable edit, and `tempo save` saves it. The agent shows up like a collaborator in a shared
file: its avatar in the top bar, an outline and cursor on what it just changed, a line in the
status bar, and each edit in the Agent panel. Otherwise the folder is edited directly.

`tempo params <project> <track>` (or `--aux N`, `--master`, with `--insert N`) lists a plugin's
parameters (value and display, default, unit, steps, and a choice's options), for
`set_plugin_param` and plugin automation; `tempo presets` lists its presets, and `--save <name>`
saves its settings as one. Both answer from the app if it's open, else by loading that plugin.

The sample library is the folders in `~/Library/Application Support/tempo/library.json`
(`~/Music/tempo/Samples` to start; `tempo samples add <folder>`), each sample analysed once (what
it is, its note or key, tempo, envelope, spectrum, stereo width, traits like punchy or lo-fi, and
a timbre fingerprint) and indexed in `~/Library/Caches/tempo/samples.json`; `tempo samples scan`
analyses what's new or changed, on every core. The analysis (`crates/engine/src/samples/`) reads
WAV, AIFF, FLAC and MP3, and names a sample from its audio with small models fitted on free
MusicRadar packs (`model.rs`), its file name a strong hint on top;
`cargo run --release -p tempo-engine --example sample_eval` measures it against the 90s Hip Hop
pack's names (held out from the fit) and, with `--fit`, fits the models again.

`tempo mcp` is the same as an MCP server on stdio, for agents without a shell (Claude Desktop,
claude.ai and the like): its tools list themselves with typed inputs, and a user can allow them
without allowing a shell. Its tools are `describe`, `show`, `edit`, `analyze`, `render`,
`critique`, `playbook`, `theory`, `reference`, `sounds`, `mix`, `master`, `transport`,
`plugins`, `params`, `presets`, `samples_find`, `samples_similar`, `samples_kit`, `samples_put`,
`palette` and `samples_library`. For Claude Code:

    claude mcp add tempo -- /path/to/tempo mcp

## Projects

`tempo projects` lists every song as JSON; `tempo new <name> [--tempo N] [--key "A minor"]
[--time 6/8] [--template beat|band|lofi|electronic]` starts one in `~/Music/tempo` (or
`TEMPO_SONGS`). A project is a folder holding `project.json` and, for audio regions, `Audio Files/`. Positions
are in ticks, 960 to a quarter note; audio offsets are in samples. Unsaved edits are kept every
30 seconds in `.recovery/`, a project of its own; if tempo stops before they're saved, opening the
project offers them back.

```json
{
  "tempo": 120,
  "sample_rate": 48000,
  "tracks": [
    {
      "name": "Keys",
      "instrument": "synth",
      "volume_db": -6, "pan": -0.2, "mute": false, "solo": false,
      "inserts": [{ "path": "/Library/Audio/Plug-Ins/VST3/Some Reverb.vst3", "class": "Some Reverb" }],
      "regions": [{ "start": 0, "length": 3840,
                    "midi": { "notes": [{ "start": 0, "length": 960, "pitch": 60, "velocity": 100 }] } }]
    },
    {
      "name": "Drums",
      "regions": [{ "start": 0, "length": 7680, "audio": { "file": "Audio Files/drums.wav", "offset": 0 } }]
    }
  ]
}
```

An instrument is `"synth"` (built in) or `{ "plugin": { "format": …, "path": …, "class": …,
"uid": … } }` (as `tempo plugins` lists it; `"vst3"` is still read); a plugin can carry the
`"preset"` tempo loaded, `"params"` tempo sets on it (by id, in the order set) and, as an insert,
the track it hears as a `"sidechain"`. A track without one is an audio track, with an `"input"`.
Audio files are WAV, AIFF, FLAC or MP3, at any sample rate. An audio region can have a
`"stretch"` (2 is half speed), a `"flex"` (`"polyphonic"`, `"slicing"` or `"speed"`) and
`"follow": true` to follow the song's tempo. A MIDI region can have `"controls"` beside its notes. Tracks can have
`"sends": [{ "bus": 1, "level_db": -6, "pre": false }]`, played by an aux in `"auxes": [{ "bus":
1, "name": "Reverb", "inserts": […] }]`. Tracks, auxes and the master (`master_eq`,
`master_comp`, `master_sat`, `master_inserts`) can have an `"eq"`, a `"comp"` and a `"sat"`. A
track's `"automation"` has `volume`, `pan`, `mute`, `sends` and `plugins` lanes (points can have a
`"curve"`, `"ease"` or be a `"step"`) and a `mode`. Regions can have a
`"name"`, a `"link"` (aliases share one), `"loop"`, `"transpose"`, `"velocity"` and `"quantize"`.
The project can have a `"time_signature"`, `"markers"`, a `"key"`, a `"cycle"` and a `"palette"` of
sounds, instruments and effects picked for an agent (see `tempo describe`).

The engine's tests that need a real plugin are opt-in, each format by its own variable:

    TEMPO_TEST_VST3="/path/Some Synth.vst3" TEMPO_TEST_CLAP="/path/Some Synth.clap" \
    TEMPO_TEST_CLAP_FX="/path/Some Reverb.clap" TEMPO_TEST_VST2="/path/Some Plugin.vst" \
    TEMPO_TEST_AU="aumu SgXT VmbA" cargo test -p tempo-engine -- --ignored

Apple's own Audio Units (AUSampler, AUDelay, AULowpass…) are tested on every run.
