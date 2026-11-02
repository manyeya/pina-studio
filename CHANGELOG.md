# Changelog

What changed in each release of Pina Studio, newest first. Every change that someone using Pina
Studio (or an agent driving it) would notice goes under **Unreleased** in the same branch that
makes it; a release turns Unreleased into its version and date (see `docs/branches.md`).

Versions follow semantic versioning: the third number for fixes, the second for features, the
first for changes that break songs or commands made before.

## [Unreleased]

### Changed

- Song Study knows what a file is before it says anything about it: a full song, a loop or a
  stem, and what's in it. A piano loop no longer gets drums, a drum loop no longer gets a key or
  chords, a bass line shows notes, and what isn't there is listed as not there. Loops with no
  drums told they had them went from 35 of 42 to 1 of 44. Tempo and key come from the file's name
  when it gives them, with the sound as the check: right tempos went from 26 of 47 to 50 of 50
  on files whose names give one, and right keys from 7 of 35 to 35 of 36. Each finding says where
  it came from and how sure it is. Songs studied before are studied again in the background.
- Song Study reads chords and notes far better. A loud note held over a chord no longer hides the
  chord, each pitch counts once, the bass is the lowest note really sounding however high that
  is, and chords lean to the song's key. Scored against 92 loops' own MIDI, bars with the right
  chord went from 37% to 53%, and bass loops' right notes from 58% to 87%. Keys of songs change
  where the old reading was off: That Night now reads Bb minor, I see through you D minor.
- The Song Study page is now one screen. Everything found sits on one timeline with one playhead:
  form, who plays where, chords, steps, a sharp spectrogram with its held lines, note starts and
  the sound. It plays the file itself, and a click on anything plays from there. Harmony, groove,
  mix, words, what's known, playbooks and compare are tabs under it. The list of studies has a
  search and filters.
- The Playbooks page has the same timeline. A click still changes who plays where, and the open
  song sits under the book. Moves says what the book does, lights the bars it's about, and asks
  the Studio Agent to do it in a click. Chords and groove can be heard, on real instruments. Feel,
  parts, mix target and notes are tabs. The list has a search and filters, so it holds any number
  of books, and New playbook starts one.
- A book made from a song now keeps that song's chords, drums and each section's loudness, and
  links back to its study. Saving a song's book again updates it instead of making a copy.

### Fixed

- The Studio Agent can fill a song with loops. Looping a region with nothing after it used to do
  nothing, and said nothing. Now it says how far the loop runs, and what to set if it doesn't
  repeat. `show` gives each looped region its `loops_to`, so looped bars no longer read as empty.
- Stretching a looped region with `trim_region` runs its loops further, as a producer means it. It
  used to stretch one pass, leaving silence after its notes: the empty bars agents kept leaving.
  `loop_length` given shorter than the region is put right, and a pass that ends in silence is
  called out in the answer.
- A region dropped into a loop no longer silences everything after it: the loop carries on after
  it, in time.
- The critic no longer calls a looped audio track silent in bars it plays.

- The Agent panel is now the Studio Agent, and it's been redesigned. Claude Code and Codex are
  the engines you pick under the box you type in. The header says which conversation you're in,
  and the song's genre and facts no longer run over each other.
- What the Studio Agent does reads calmly: a run of steps folds to one line saying what it's doing
  or what it did, and opens with a click. Answers are set properly, with headings, lists, tables,
  code, and bold.

- Play in Key works on MIDI. Switching it on moves the results at once: each row shows the move
  it will really make, what's already in key leads, and what's playing moves while you listen,
  sounds included. A song with no key says so rather than doing nothing. A file that starts with
  drums now moves its other parts, and asking for a key no longer loses drum files from the list.
- Fits the Song really filters MIDI now. It keeps what's in the song's notes and near its tempo.
  With Play in Key on, files that would be moved in fit too if they share its mood: a minor file
  for a minor song. The part chips count what's left.
- MIDI brought into an empty song no longer replaces a key you've chosen. A song with no key
  takes the key its notes end up in once moved, not the key the file was in.
- The top bar no longer has buttons for Live Loops and Settings. ⌥L and ⌘, still open them, and
  both are in the menus.
- Modulator cards keep everything inside them at any pane height and with any name. Long track
  names shorten, the picture takes the spare room, and in a short pane the card scrolls under its
  title. The Add buttons lead the bar, and a route's depth is a thin meter along its foot.
- Stacks are built like the tracks under them, with the same icon, strip, meter and buttons.
  Tall enough, they have a fader and pan knob too. A stack's level can be typed, the name being
  typed shows in its row, and a tall stack's lane shows its regions as small regions.
- The editor bar keeps what's used most in view, and the rest in a new More menu that lights up
  while something in it is on. The piano roll's More has chords, ghost notes, colour by velocity
  and fit; the Sampler's has Mono and Legato. Tabs scroll if there are too many for the bar, so
  they no longer hide the controls.

- Pina Studio runs on all your fast cores. Tracks that don't depend on each other now play at
  the same time, on helper threads as urgent as the audio device's own, instead of one after
  another on one core. A heavy synth patch that ran late at 8 tracks now plays 48 with no
  dropouts, and the mix comes out the same to the bit.
- The Synth costs far less, and an instance with nothing playing costs nothing. The worst patch
  takes about a third of what it did, and Init a quarter. A released build is also optimized
  across the whole program now.
- The CPU number in the status bar changes twice a second and shows the average, and turns red
  for a few seconds when a buffer really came too late. It now measures against what the device
  actually asked for. A low number doesn't show how much room is left, because macOS slows the
  chip down to just meet the deadline.

- The agent's `mix` uses everything the built-in effects have. Before, it put the same single
  Warm Tape saturator on every part, only its drive changed. Now it sets each saturator by
  listening to the part: where it splits into bands, and each band's style, drive, tone and
  dynamics for the part in the song's genre.
  - A bass keeps its fundamental clean and has its harmonics driven, harder when it's nearly a
    sine.
  - Drums get three bands: Crunch in trap, Transformer in house, Lo-Fi in lo-fi.
  - Leads in electronic genres ring a touch on the key's tonic.
  - Each saturator's output is set so the part comes out as loud as it went in.
  - Drums that swing far between soft and loud hits are compressed in parallel.
  - EQs get an analog colour (warm, or subtle) where the genre is warm, and a lead's or voice's
    EQ uses the natural phase mode; the master's tonal-balance EQ uses natural phase too.
  - `mix` says why it chose each saturator, track by track.
- `mix` puts Pina Studio's own reverb and delay on its Verb and Echo buses, so it works the
  same everywhere:
  - **Reverb kind by genre:** a plate for voices in pop, R&B and trap; a chamber for lo-fi and
    jazz; a room for dance music; a cathedral for ambient. Its pre-delay is a sixty-fourth note,
    and it ducks where a voice must stay clear.
  - **Delay kind by genre:** tape, ping-pong or dub; ducked under what it repeats.
  - Before, the reverb was Dragonfly if installed, else Apple's, and the delay was Apple's. A bus
    an earlier mix set up with those has them swapped for the built-ins.
- The agent is told to treat `mix` as a starting point and to shape each part's built-in effects
  with all they offer.

- Song Study's Start a song like this is now Create a song like this: the song keeps its reference
  and choices, the agent is asked to write it, and it's compared with the reference afterwards.
  It no longer saves a playbook of the study (Save as playbook still does).

- The Drum Kit, and Beat in the new-track palette, make a kit of pads from the library's drums,
  fitted to the song's key and tempo, in the Drum Machine; a pack in the palette makes a kit of
  that pack. With no drums in the library, the synthesized kit (as before). A sample dropped on a
  kit's track becomes a new pad in it.

- `master` (and `pina master`) uses the master's built-in limiter, set by listening, on any
  platform. It aims true peaks under −1 dBTP and leaves the master fader at 0 dB. A song
  mastered before keeps its saturator; macOS's peak limiter, which mastering used to add, is taken
  out.

- The compressor and saturator are Pina Studio's own Rust DSP (Faust is gone), and each opens a
  window of its own from its slot or mixer card, as the Channel EQ does.
- A compressor added to a channel sets itself by listening: its style, threshold, ratio, attack,
  a release in time with the song, how far it works spectrally, and the makeup that keeps it as
  loud as it went in. A bus or the master's starts as a console-style bus compressor. Learn in its
  window listens again; `pina learn <song> --track N | --master` prints what it would set.
- The saturator is multiband: up to six bands, each with its own
  style (warm and clean tape and tube, transformer, crunch and heavy amp, rectify, foldback,
  lo-fi), drive, tone, dynamics, feedback, mix and level, mute and solo; crossovers 12, 24 or 48 dB
  an octave; 2, 4 or 8 times oversampled. Songs saved with one drive and tone open as one warm
  tape band.
- Agents write songs themselves, every note, in any style: `write_drums`, `write_bass`,
  `write_melody`, `write_chords` and `arrange` are gone (genre grooves, bass, melody and chord
  styles, and a loop pasted through every section), and so is `transition`'s drum fill. They're
  told to plan the whole song, give each section its own parts, fills and transitions, and give
  it a feel, as a producer would. `set_chords`, `copy_bars` and the transition sounds stay. A
  playbook made from a song says what the song's drums do, not how a genre's would be written.
- Playbooks are yours alone, and optional. Pina Studio ships none (its 17 are gone), and agents
  follow one only when you ask for it or pick it as the song's genre: they're no longer told to
  start from one, the critic doesn't ask for a genre, `set_genre` takes any word, and `arrange`
  is given its sections. The Playbooks page has Delete in place of Reset.
- Flex is called Stretch: the Stretch tool, and the inspector's Stretch (Polyphonic, Slicing or
  Speed). Songs and commands say `"stretch_mode"` and `stretch_region`; `"flex"` and
  `flex_region` are still read. An audio region's Time reads As recorded rather than Off.
- Dragging a marker moves its section with everything in it; ⌘-drag moves the marker alone, as
  dragging did.
- Live Loops is on ⌥L; L loops the selected region.
- `set_tempo` scales the song's tempo changes along with it. Audio that follows the tempo keeps its
  own tempo rather than a stretch for the song's; songs saved before are read that way.
- The step sequencer, rebuilt. A menu bar (Edit, Functions, View; the pattern browser and
  inspector; MIDI In for step input, MIDI Out, Mono; the edit mode; live recording, Preview,
  zoom), a row header per row (its subrows, icon, assignment, mute and solo, its own rate and
  playback mode, rotate, down and up, randomize, clear), and every step with its own velocity
  (or value), gate, tie, note, octave, repeat (up to 16, with velocity ramp, curve and
  distribution), chance and condition (always, every so many passes, first or last pass),
  offset, rate and skip, each shown on the steps or on a subrow of its own. Rows have their own
  loops, rates and playback modes (forward, backward, ping-pong, pendulum, wander, shuffle,
  random); patterns up to 64 steps with a key, a scale, legato and swing on sixteenths or
  eighths. Automation rows step a track's parameters, MIDI controllers, pitch bend and
  pressure. A pattern browser of grooves, bass lines, melodies and templates, with your own
  saved in folders; Learn makes rows from what's played or moved; copy and paste steps, rows,
  pages and row settings; a default template per track; the shortcuts to go with it all. The
  pads, the Modulate strip and the Fill, Reverse and Euclid buttons are gone (a sound's
  velocity, pitch and chance can still be modulated from the Modulation pane).
- A new pattern's steps hold their whole length (patterns made before still hold 75%, as they
  did). Agents' `add_pattern_region` and `set_pattern` take the new settings: `steps`, `mode`,
  `legato`, `swing_eighths`, `key`, `scale`, and per row and step everything above.
- Region menus say Convert to Pattern Region, Convert to MIDI Region and Create Pattern Region.
  Converting MIDI to a pattern keeps each note's timing as its offset and its length as ties and
  a gate.

### Added

- Conversations stay. Each is kept in its song's folder, so a new one leaves the last to come
  back to. History lists them under when they were, finds them by anything said in them, and
  renames or deletes them; a new conversation offers your last three. Open one and carry on: the
  agent picks up from its own memory, and if it no longer has one (the song moved, or it's too old)
  it's told what was said, without you seeing a thing.
- Copy from the Studio Agent: drag across its words and ⌘C, across paragraphs too; a double-click
  takes a word. Hover for a copy button on what you asked, on code and on a whole answer, and
  the header's menu copies the whole conversation.
- The Studio Agent shows rather than tells. It can put components in the conversation with the
  new `present` tool: a plan that ticks along as it works, a choice you answer by clicking,
  measurements against their targets, chords you can hear, a song's shape to jump around, and
  what to do next.
- What Changed: at the end of a turn, a card lists what really happened to the song, worked out
  from the song itself. Click a line to go there, hear it from the first change, or undo the turn.
- A play button on every sound in the Browser, in the list beside the tracks and on its page: it
  plays and stops without picking the sound up. The MIDI list's Agent button moved after the key.
- The Map for MIDI: every file placed by how it plays, whatever key it's in, so basses gather
  with basses and busy lines with busy lines. Rest on a dot to hear it, click to pick it, draw a
  box to keep those inside it, and drag one to the tracks.
- The Kit for MIDI: a file for each part, chosen to go together, all in one key. Parts come from
  one pack where they can and follow the same roots bar by bar. Hear Together plays them looping
  as one, and stepping through a part or muting it changes what you hear at once. Lock the ones
  to keep and New Picks finds another kit around them. Add to Song puts them on new tracks that
  go round together, and a card dragged out takes just that part.
- Drag a stack's bottom edge to make it taller; double-click it for its usual height.
- Settings > Audio: choose the output, and the buffer size from Auto to 2048 samples. Both
  change while the song plays, and stay chosen. Bigger buffers give a heavy song more room,
  smaller ones answer your keyboard sooner.
- MIDI for the agent. Give it files or whole folders (drop them on the Agent panel, use its +, or
  the + on a Browser row) and it reads them. It can compare a group for what the files share and
  write a new one like them, checking how close it came. What it writes doesn't touch the song:
  each is a card in the conversation and on the panel's new MIDI page, to hear, stop, drag onto
  the tracks or give back to it. They're kept in the song's MIDI folder. Agents use `midi_group`
  and `midi_write`.
- Previews can be stopped: a Stop preview button in the Browser's lists while one plays, and a
  stop on the row playing.
- Six new built-in effects, in the Browser and stackable anywhere in a chain, each with a window
  you can drag on: Filter, Motion (chorus, ensemble, flanger, phaser, vibrato, tremolo), Transient
  Shaper, Gate, De-esser and Stereo. Agents set them with the `set_fx` edit, and `critique` says
  when a song of four parts or more uses none of them.
- Everything built in can now be automated. The Compressor and Saturator (every knob, each of a
  saturator's bands), a stacked Channel EQ, Compressor, Saturator or Limiter, every knob of the
  Reverb and Delay, and the Sampler's voice all appear in an automation lane's list, and all can
  be moved by modulators. Before, only the EQ's bands and the limiter could.
- Four more built-in effects, each with a window you can drag on: Multiband (three phase-coherent
  bands, each compressed and lifted alone), Pitch Shifter (a chord stays a chord), Tape Stop, and
  Pitch Correction (a voice brought to the song's notes, its character kept, with a live trace of
  what's sung and what comes out). Agents reach them with `set_fx`.
- Every knob of those six, and of a stacked Reverb or Delay, can be automated and modulated: in
  its lane, with a modulator, or with `set_automation` and `"param": "fx"` in the knob's own units
  (a cutoff in Hz). The window's picture moves with it, and lanes and routes follow an effect when
  it's moved, and go when it's taken out.
- Pina Studio's own Synth, in the Browser's instruments, with its own window.
  - **Oscillators:** three wavetable oscillators, each with 24 built-in tables to sweep through
    (shown in depth as they play), two warps from twenty (sync, bends, PWM, fold, phase
    distortion, FM, ring and AM from an earlier oscillator) and up to sixteen voices of unison, with
    a sub and noise. Nothing aliases.
  - **Filters:** two, in series, parallel or one a side, of twenty kinds. Ladder, cascade, diode
    and a snarling two-pole are circuits solved with zero-delay feedback, every stage saturating, so
    they stay in tune and sing and overdrive as hardware does.
  - **Movement:** four envelopes, eight LFOs (free or in time, drawn waves and chaos), eight macros
    and a 64-line matrix. Drag a source's ↗ onto any knob.
  - **Deep in the app:** every parameter can be automated, modulators reach every knob, its sources
    can ride the effects on its track, and its Effects page is the track's own chain.
  - **Voices:** mono, legato, glide, analog drift, an arpeggiator, and three quality settings.
  - Thirty patches. Agents use `set_synth`, and its patches come up in `sounds`. A song made with the
    earlier synth opens with this one, set as close as it can be.
- Pina Studio's own Reverb, in the Browser and every channel's effects. It's an
  eight-line feedback delay network:
  - **Styles:** Room, Chamber, Hall, Plate, Ambience, Cathedral and Shimmer.
  - **Settings:** pre-delay in ms or in time with the song, size, decay, early reflections,
    diffusion, damping, how long the lows ring, modulation, low and high cuts, width, ducking
    under what's playing, shimmer (the tail climbs an octave), freeze and mix.
  - **Window:** draws what it does to a click: the pre-delay, each early reflection, and the
    lows, middle and highs each falling as long as they ring. Drag the pre-delay and the decay
    on that picture. Beside it, how long it rings across the frequencies, over the live
    spectrum of what it plays, with points to drag for the lows, the damping and both cuts.
- Pina Studio's own Delay:
  - **Styles:** Digital, Tape, Analog, Ping-Pong, Dub and Slapback.
  - **Settings:** each side's time in note values (dotted and triplet too) or ms, feedback past
    100% held by a soft knee, crossing sides, low and high cuts on every repeat, drive, wow and
    flutter, diffusion, ducking, width, freeze and mix.
  - **Window:** draws every echo over the song's beats, left and right, as loud as it comes
    round. Drag each side's first echo for its time (note values when synced) and the next for
    the feedback. Beside it, what each repeat keeps, three times round, over the live spectrum,
    its cuts dragged by their points.
- Both stack like the other built-ins. Added to a bus they're all wet; on a track, part wet.
  Commands: `set_reverb` and `set_delay`.
- Built-in effects stack: a channel can have any number of EQs, compressors, saturators and
  limiters, each anywhere in its chain. For example, an EQ after the saturator to tame what it
  adds, or a fast compressor and then a slow one.
  - **Adding:** adding one the channel already has puts another after its plugins (before a
    limiter that's last), and opens it.
  - **In the app:** each copy shows in the strip and the mixer panel as "Compressor 2",
    "Channel EQ 2" and so on, with its own full window: knobs, styles, Learn, the EQ's analyzer
    and the compressor's and limiter's meters. Drag it anywhere in the chain or take it out from
    its menu.
  - **Commands:** `set_eq`, `set_eq_band`, `set_comp`, `set_saturator` and `set_limiter` take
    `"which"` (1 on: the stacked ones; one past the last adds another). Songs list them as
    `"more"`.
  - **Not yet:** stacked copies aren't automated or modulated; the channel's own are.
- A saturator added in Pina Studio sets itself by listening, as the compressor does, and its
  window has a Learn button. `pina learn <song> --track N --sat` (or `--master --sat`) prints
  what it would set, and why.
- The agent's `learn` tool sets one built-in effect (compressor, saturator or limiter) by
  listening, and answers the settings to shape from.
- `critique` says when a compressor or saturator is left as it came, when three or more tracks
  share one saturator setting, and when every saturator is Warm Tape.
- MIDI files. The sample library reads the MIDI files in its folders and studies each one: its key
  (from its notes, checked against its name), tempo, meter, chords and progression with
  numerals, how many bars it loops, each part's role (drums, bass, chords, pad, keys, pluck,
  lead, arp, fx) and how it plays (range, notes at once, notes a bar, where in the bar notes
  start, swing, syncopation, velocity, legato, a melody's shape, each drum's pattern). In the
  Browser's Sounds and on the sample page, MIDI shows them, best fit first, a chip for each
  part; each row has a little piano roll. A click plays the file at the song's tempo, moved into
  its key with Play in Key on, in time with the song while it plays. Picked, the page shows its
  whole study. Add to song, a double-click or a drag puts it in the song. Dragged onto an
  instrument track, a one-part file becomes a region there. Otherwise each part gets a new track
  with an instrument that plays it (General MIDI sounds on macOS, drums on drums). MIDI files
  dragged in from the Finder land the same way. Drum files on an ordinary channel, as packs
  often write them, come in as drums and are never transposed.
- `pina midi <file> [--notes]` prints a MIDI file's study (with `--notes`, every note as text,
  bar and step by bar and step); `pina midi find "dark bass" --role bass --key "A minor" --for
  <song>` searches the library's MIDI files, each hit with the semitones that move it into the
  key. The `import_midi` command brings a file into a song (`fit` moves it into the song's key).
  Agents get `midi_find` and `midi_study` to read the person's MIDI for ideas, and write their
  own notes from them.
- Songs like a reference. Song Study draws a blueprint of any song it studies (its spectrum over
  time with its held lines, its sections and energy, where things start low, mid and high, every
  finding marked measured, estimated or unknown) and studies a picture of a spectrogram when
  there's no audio: its plot and colour scale found, its axes given from its labels (how long,
  its frequencies and their scale), and nothing claimed that a picture can't show. Create a song
  like this keeps the reference and the person's choices (a style, and how closely to follow its
  rhythm, form and sound) with a new song and asks the agent to write it, every note its own.
  The song is then compared with its reference section by section (groove, notes, balance,
  energy), with its biggest differences and revisions asked for in words, which the agent makes
  and Pina judges, stopping once reached or no longer helping. On the command line as `pina like`,
  `pina like create` and `pina like compare`; for agents as `reference_study`, `reference_create`
  and `reference_compare`. From a recording, the bass line and the melody are read note by note
  (estimates: their rhythm and the way they move) for the agent to write its own on, and compared
  with the song's; a song made like a reference is compared with it on every full critique.
- The critic hears a chord track that leaves the song's key (and says which key the chords fit),
  a fader pushed up to make up for a quiet sound, and a whole song nothing has been mixed in.
- The Drum Machine, a tab under the tracks for a kit (or any of its pads). Pads four by four
  from C1, a map of all 128 notes beside them; click to hear a pad (higher, harder), drag one onto
  another to trade places (the kit's patterns follow), drop a sample on one from the browser or
  the Finder (an empty pad becomes a new one). Pads show their sound's outline and light as they
  play. The pad picked: its sound stepped through (its folder's others of its kind, then the
  library's most like it) or picked from a menu, its waveform and start and end, its choke group,
  mute and solo, and its shaping: tune, fine, level, pan, attack, length, velocity, a pitch drop,
  the filter, punch, sub, noise and colour. The whole kit's sounds can be stepped at once. Pads
  off the sixteen shown are counted above the grid, the nearest a click away. A kit made from
  the Samples page leaves the page be, the Drum Machine there when the tracks are next shown.
- Layers on a pad: up to eight sounds, played stacked, in turn (a round robin) or split by
  velocity, switched at a hardness or blended. + Layer offers what goes with the pad's sound (a
  clap on a snare, a sub under a kick); a sample dropped on the pad's sound is layered on. Each
  layer is a tab by name, with its own level, pan, tuning, start and end. A sampler zone's `layer` keeps zones that share notes and velocities
  playing together rather than taking turns.
- Kits from a library folder: in the Drum Machine, and the sample browser's Kit view (From a
  folder), a pad a hit (or, for a big folder, a pad a kind of sound, sixteen in all), each with the
  folder's others of its kind to step through. The Kit view's Load into puts its picks into the
  kit picked in the song, keeping its pads' shaping, channels and patterns.
- A navigation bar under the tracks. Under the headers are Follow (the view follows the
  playhead; scrolling sideways while playing lets go of it until play starts again or the
  playhead's moved), zoom out, zoom in and fit the song. Under the lanes, the whole song in
  miniature (every region in its track's colour, the cycle, the playhead) with the part on
  screen framed: drag it to scroll, click elsewhere to jump there, double-click to fit the
  song. The tracks have a scrollbar at the right when they don't all fit, ⇧ with a mouse's
  wheel scrolls sideways, and scrolling stops eight bars past the song's end.
- A built-in Limiter, last in a channel's chain: a lookahead brickwall that nothing passes (true
  peaks too, by default), in ten styles (Transparent, Punchy, Dynamic, Allround, Aggressive,
  Modern, Bus, Safe, Classic, Multiband). Gain, ceiling, lookahead, attack, release (set or
  auto), link, a soft clipper oversampled up to 8 times, and a DC filter. Unity and Delta are
  for listening. Its window shows in, out, gain reduction and short-term loudness as they play,
  with a loudness meter (integrated, short-term, momentary, true peak, PLR) beside it. Learn sets
  its style, release and gain for the loudness picked (Streaming −14, Quieter −16, Broadcast
  −23, Club −9, Loud −7), on a track, an aux (hearing everything sent or routed to it) or the
  master. Its gain, ceiling, release and clip can be automated on a track (Touch, Latch and
  Write record its knobs), and modulators move its gain and ceiling. Oversampled, the whole
  limiter runs at the higher rate through linear-phase filters, so nothing it does folds back
  into the band. Delay compensation waits for its lookahead. Agents set it with `set_limiter`
  and automate it with `limit_gain`, `limit_ceiling`, `limit_release` and `limit_clip`; `pina
  learn <song> --master --limit -14` (or `--track N`, `--aux N`) prints what Learn would set.

- Effects in any order: drag the EQ, compressor, saturator or a plugin up and down a channel
  strip (or Move up and down from its menu, in the strip or the mixer's cards) and the chain runs
  in that order. A plugin moved keeps its settings, automation and open window: it isn't loaded
  again, nor when the move is undone. `move_effect` does it for agents.
- The Sampler, rebuilt. Three ways to read a sample: Repitch (Hermite-interpolated, from copies
  halved in rate so it doesn't alias played high), Stretch (grains keep each note's pitch at any
  speed, or none) and Grains (a cloud with Freeze). Three characters: Clean, Tape, Vintage (the
  old samplers' rate and bits). Filters: notch, a 24 dB ladder, comb and formant join low, high
  and band pass, with drive. Unison (up to 8 voices, detuned and spread), two LFOs (seven waves,
  synced or free, retriggered), a modulation envelope, an 8-route matrix, and layers from the
  drum samplers: a pitch drop, sub, noise, punch, FM and ring. Zones gain loop crossfades,
  velocity fades (layers blend), release samples, and their root heard from the sample.
- The Sampler's editor, redesigned: the sample big (zoom, every slice in its colour, the loop's
  crossfade drawn, live playheads and grain sparks), the filter's response and the envelopes drawn
  and dragged, the matrix, and a card for each part. Track modulators reach the new settings too.
- Compressor styles: Clean, Smooth, Punchy and Bus (a console bus compressor's stepped controls and Auto release), with
  mix for parallel compression, an automatic release, a sidechain high pass and spectral
  compression, which gives the turning down to the part of the spectrum that caused it (a kick
  turns down the lows, not the hats over it).
- The compressor's window shows what goes in, what comes out and what it takes off as it plays,
  its curve with the level now on it, and a reduction meter. The saturator's shows its bands over
  the channel's spectrum: click one to work on it, double-click to split one, drag a split to
  move it.
- Holding ⌥ (Alt on Windows) puts a stretch grip on every region's end, and all of the end
  time-stretches then (its top half no longer loops); the status bar says so.
- A region says "Stretching…" while its audio is being stretched, and "Finding tempo…" while
  it's listened to for its tempo.
- ⇧T time-stretches the audio picked to the song's tempo (its tempo found first if it isn't
  known); again, back as it played before.
- Several regions picked at once: ⇧-click, or a box dragged from an empty lane. They move
  together, and delete, mute, copy, cut, paste, nudge, transpose, loop and quantize act on them
  all.
- Drag modes in the snap menu: Overlap, No Overlap (what a region lands on is cut back) and
  Shuffle (the track's regions close up end to end; a deleted one leaves no gap).
- A line down the lanes while a region is dragged, where its start or end meets another region's.
- Audio quantize: Q on an audio region (or Quantize Hits in its menu) moves its hits onto the
  grid with warp markers.
- Hover help: over a control in the top bar or over the tracks, the status bar says what it does
  and its key.
- The arrangement's keys: the previous and next region and track, nudge, transpose, loop,
  quantize, join, to the playhead, trim to the playhead, bounce in place, stretch to the cycle or
  bar, snap on and off, cycle what's picked, markers (add, previous, next), the playhead by bars
  and to the end, play from what's picked, zoom to fit, lane heights, and the track's mute, solo,
  arm, rename and duplicate.

- A kit from the sample library goes into the song as a stack: a kit track whose notes play its
  pads, and a track a pad with its own sampler, channel and effects. Drums on their General MIDI
  notes, the hats choking each other; a starter pattern on the kit track, a row a pad; live notes
  on the kit play its pads; a sound dropped on a pad swaps it; Split Notes onto Pads moves the kit
  track's notes onto the pads' tracks. Agents: `add_kit` and `split_kit`.

- Sections: each marker starts a section that runs to the next. Dragged, a section moves with
  everything in it (regions cut at its edges, their automation, the markers and chords in it, its
  tempo), the song closing up and making room; dropped on another's middle, the two swap; ⌥-drag
  copies it; ⌘-drag moves the marker alone. Its menu cycles it, copies it to the end, puts a bar
  before it, clears it or deletes it (closing the gap); + adds an 8-bar section after the last.
  Agents: `move_section`, `copy_section`, `swap_sections`, `delete_section`, `clear_section`,
  `add_section`, `insert_time` and `remove_time`.

- The tempo can change as the song goes. G shows the tempo lane under the ruler: the tempo as
  steps, a change added with a double-click, its point dragged in time and tempo, a step dragged
  up or down, a point double-clicked out. The display shows the tempo where the playhead is;
  clicked, its menu.
- The song's tempo mode, Keep, Adapt or Auto, says what audio recorded or brought in does to
  the tempo: kept (a loop follows it), adapted to the audio, or adapted only while nothing else
  keeps time and the metronome is off. Audio dropped in or recorded is listened to for its tempo,
  its beats and its downbeats.
- Audio keeps time with the song as its Time says: played as recorded, following the song's tempo
  (its beats on the song's through every tempo change, a badge on it), or locked to its bars or
  beats, so a take that drifts keeps to the grid. Its own tempo shows in the inspector (halved,
  doubled, typed, or found by listening). Keep time makes the song take its tempo, bar by bar or
  steady; Fit cycle sets the tempo so it fills the cycle. Its menu has the same.
- Agents: `set_tempo_at`, `delete_tempo`, `set_tempo_mode`, `analyze_tempo`, `adapt_tempo` and
  `fit_tempo`; `set_region` takes `"follow"` as off, on, bars or beats, and `"tempo"`.

- The Plugins page: the Browser's instruments and effects as a page of their own, as the sample
  library has one. Expand in the Browser (or ⌃⌥D) opens it on the Browser's tab with its search,
  Compact goes back. On the left the search; all, instruments or effects; the starred ones;
  where from, the formats and the order; chips for the categories (and the kinds in them: a
  compressor's opto or FET, a reverb's plate or hall), the makers, your collections and what
  Health found (what won't load, needs Rosetta, is installed in two formats, is in no song); and
  the library, its size, what could go to the Trash and the pictures. In the middle what's found
  as pictures of each plugin's own window, as a list (its kind, formats, CPU, size and songs), or
  on the keyboard. On the right the one picked: its picture, what it is, Add to a track (or a new
  one), the tracks it's on, its files, key and collections. A double-click or ↵ puts it on the
  selected track; dragged out of the page the tracks come back to drop it on.
- Take pictures opens each plugin once in a process of its own to photograph its window and time
  how hard it works; one that crashes or hangs is marked as not loading, and Pina Studio carries
  on. A plugin opened in a song has its picture taken then, if it has none.
- Keys: put a plugin on a key of its own (pick it and press the key, click a key and then a
  plugin, or drag one onto a key), and that key puts it on the selected track.
- A mixer slot, Add effect and an instrument's Change open a picker: a search, the categories,
  each plugin with its picture; ↑ ↓ pick, ↵ puts it in.
- The Browser's rows show each plugin's picture, and its kind (Graphic, Shelf, Opto…). A new
  category, Multi FX, for plugins that do several things.
- Pina Studio is open source, under the MIT license, and has a website:
  https://manyeya.github.io/pina-studio/ (its downloads, Midnight Drive and Corner Store Blues to
  play track by track, and an agent building Corner Store Blues one command at a time).

### Fixed

- A stack's channel shows in the Inspector when the stack is picked, and beside a stacked
  track's own channel, so its volume and effects can be set there. Before, the Inspector showed
  only the track's channel and the master.
- A dynamic EQ band's dot stays where the band is set while the song plays, so it can be grabbed;
  where it's heard is a ring riding its range line, and its curve still moves. The dot used to
  follow the sound, slipping out from under the pointer.
- Playing (or bouncing) from the start of a song no longer skips its first few milliseconds
  when a channel has latency (a linear-phase EQ, a limiter, a plugin): a note on the very first
  beat was lost. Play now reads the delay-compensation lookahead before the playhead moves, and
  a limiter added while playing doesn't skip what's between.

- Adding, taking out or replacing a plugin no longer reloads the channel's other plugins from
  the last save: they run on as they are, and only the new one loads. A channel that does have to
  be made again (another instrument, or new samples on a sampler) gives its plugins the settings
  they have now, not the saved ones. Two of the same plugin on a channel are told apart as they're
  moved and taken out.
- A plugin that won't load no longer silences its whole channel: the rest play, it passes the
  sound through, and its saved settings are kept when the song is saved.
- A mixer strip full of effects still shows the slot to add another, with how many more there are
  than it shows.
- Mixer strips are wider (effect names fit), show five effects, and fit the window whole: their
  name plates no longer fall off the bottom, and the labels down the left line up with every row
  (Mode included).
- Dragging an effect up or down a strip (or by the grip on its card in the mixer's panel) lifts it
  under the pointer and marks where it'll land, which is the row nearest the pointer as drawn.
- The mixer scrolls sideways when there are more strips than fit, by trackpad or by a mouse
  wheel; before, the strips past the window's edge couldn't be reached.
- An agent's answer that goes wrong says what happened and what to do ("Codex couldn't reach
  OpenAI…" for "workspace routing discovery failed"; signed out; a usage limit), what the agent
  said under it, and offers Try again and Ask the other agent. `pina ask` says the same.
- Time-stretching is quick: only the part of a file a region plays is stretched (a second or two
  either side), not the whole file, so an 8-bar region of a song-long take is ready in about a
  second rather than playing a sped-up stand-in for 15.
- An audio region's Time switched back to As recorded (from Follow, Lock bars or Lock beats)
  plays as it did before it followed, rather than at the stretch following left it; As recorded
  again plays it as it was recorded, not stretched.
- An audio region's length follows when it starts or stops following the tempo, or its own tempo
  is halved, doubled or typed, so it holds the same audio and a loop keeps whole beats.
- Audio stretched by its end (⌥-drag, the Stretch tool, ⌥⌘L, ⌥⌘B) fills the length asked when
  the tempo changes under it.
- Clicking a device in the Browser puts it on the selected track again (letting go of the click
  ended it as a drag first).

## [0.3.0] - 2026-10-22

### Added

- Pina Studio for Windows: an installer (per user, no administrator) with the app, the `pina`
  command and a Start menu entry. VST3, CLAP and VST2 plugins, with their editors; settings in
  AppData; deleted songs to the Recycle Bin. Audio Units stay macOS's; bounces are WAV and MP3
  there; mastering needs macOS's limiter for now.

## [0.2.0] - 2026-10-22

### Added

- Modulators reach every plugin parameter. **Learn** on a modulator's card maps it to the next
  control you turn in a plugin's own window, and its target menu lists every parameter of the
  instrument and of each insert, searchable.
- A modulator's settings can be automated: right-click a knob on its card (a macro's value, an
  LFO's phase, an envelope's times…) and pick **Automate**. Agents do the same with
  `set_automation` and `"param": "modulator"`.
- An automation lane takes modulators: right-click its header to map one of the track's
  modulators to what it shows (or make a new one that does), or drop a modulator's ↗ on it.
  Coloured dots show which modulators move it.
- Pick several tracks: ⌘-click a header (or a mixer strip) to add or remove it, ⇧-click for every
  track in between. ⇧⌘D, or **Create Track Stack** in a picked track's menu, stacks them all.
- Stacks have a row of their own above their tracks: fold them away, the stack's name and track
  count, mute, solo and a level to drag. Its lane shows its tracks' regions, and its tracks are
  set in under it. **Flatten Stack** (in its menu, or a track's) undoes one. Agents:
  `flatten_stack`.
- Stacks share modulation: **+ Shared** in the Modulation pane adds an LFO, steps, follower,
  random or macro to the stack, and every track in it routes it to its own settings. Agents:
  `share_modulator`, `unshare_modulator`.
- Mixing by measurement: each part's channel set from its own measured sound, room made between
  parts, real reverb and delay, and a tonal master.
- Agents can hear timbre (`listen`) and audition presets for a part (`sounds`), and samples are
  chosen by what they're for: drums judged as drums, pitched drums tuned to the key.

### Changed

- Making a stack moves its tracks together under the first of them. The `add_stack` command
  answers with where every track went.
- A follower's source, in commands, is the followed track's place, as every other track is named.
- The code is organised by feature: the app in `ui`, `workspace`, `arrange`, `editors`, `mix`,
  `sounds`, `pages`, `agent` and `cli`, the engine in `song`, `audio`, `dsp`, `music` and `plugin`;
  large files are folders of a file per job, and tests sit in files of their own.
- Comments, docs and on-screen text say what Pina Studio does rather than which other product
  something resembles.

### Fixed

- The Modulation pane no longer slows the whole app with a large plugin loaded: it asked the
  plugin for every parameter's name on every frame.
- A follower follows the same track after the song is saved and opened again.

## [0.1.0] - 2026-10-20

The first release, as Pina Studio (it was called tempo while it was being built). macOS, on Apple
silicon and Intel, from an installer disk image.

### Arranging and editing

- The main window: tracks with console-style headers, regions that loop, trim, fade and
  time-stretch, markers, the cycle, aliases, take folders with comping, Live Loops, and tools
  (Text, Glue, Solo, Mute, Zoom, Fade, Flex).
- Track stacks and output routing, sends and aux buses, plugin delay compensation.
- The piano roll (many notes at once, chords, lanes), the score editor, and a step sequencer
  with pads, per-step settings and a built-in drum kit.
- A sampler: zones, envelopes, filter, LFO, slices, loops, layers and round robins, with its own
  editor.
- Audio stretched to the song's tempo with its pitch kept, warp markers and formants.
- Recording audio and MIDI, musical typing, and unsaved edits kept and offered back.

### Mixing and sound

- Automation on every control: Read, Touch, Latch, Trim and Write, lanes per parameter with
  curves, shapes, selection, copy and paste, and smoothing.
- Modulation: LFOs, steps, envelopes, followers, random and macros on any setting.
- The Channel EQ: up to 24 bands, dynamic bands, natural and linear phase, EQ Match, Spectrum
  Grab, collisions with another track, and every EQ in the song in one list.
- The compressor and saturator, and a binaural panner placing sound all round the head.
- Bouncing the mix, stems or sections in every format, to a loudness, with tails.
- VST3, CLAP, VST2 and Audio Unit plugins: scanned, played, automated, saved and shown.

### Sounds

- The sample library: every sample analysed for what it is, searched by words, key and tempo,
  made into kits, and laid out on a sound map.
- One Browser for everything that goes on a track, and a New Track palette searching every
  instrument, preset, kit and sound.

### Agents

- Agents built in: an Agent panel showing an agent at work, `pina ask` in the terminal, and an
  MCP server (`pina mcp`) so any agent that speaks it can make music.
- Producer's tools for agents: music theory, a critic, genre playbooks (on a Playbooks page you
  can edit), arranging a loop into a song, mixing and mastering.
- Song Study: a finished song taken apart from its file, its form, harmony, groove and words.

### Songs

- The Songs page, where Pina Studio starts: make, open, close and trash songs.
- An installer disk image built by `scripts/release-mac.sh`.
