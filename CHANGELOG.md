# Changelog

What changed in each release of Pina Studio, newest first. Every change that someone using Pina
Studio (or an agent driving it) would notice goes under **Unreleased** in the same branch that
makes it; a release turns Unreleased into its version and date (see `docs/branches.md`).

Versions follow semantic versioning: the third number for fixes, the second for features, the
first for changes that break songs or commands made before.

## [Unreleased]

### Changed

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

- The Plugins page (⌃⌥D, or the plug in the top bar): every instrument and effect as a card
  with a picture of its own window, in a grid. Effects or Instruments; all, plugins, Pina
  Studio's own or saved presets; formats turned off and on; categories with what's in them
  (a compressor's kind, a reverb's, a synth's), makers, your own coloured collections; sorted by
  name, use or how hard each works the computer. Click puts it on the selected track (⌥: a new
  track), drag onto a collection; right-click for its collections, key, files and picture.
  **Take Pictures** opens each plugin once in a process of its own to photograph its window and
  time it; one that crashes or hangs is marked as not loading, and Pina Studio carries on. A
  plugin opened in a song has its picture taken then, if it has none.
- The Map: a keyboard, the keys Pina Studio uses marked; put a plugin on any other key (press a
  key, click one, or drag a card onto it), and that key puts it on the selected track.
- Health: how much disk the plugins take and how much could come back, which plugins the songs
  use and which they miss, those that need Rosetta, that no song uses, that are installed in more
  than one format and that won't load (try again), and spare files moved to the Trash at once.
- A mixer slot, Add effect and an instrument's Change open a picker: a search, the categories,
  each plugin with its picture; ↑ ↓ pick, ↵ puts it in.
- The Browser's rows show each plugin's picture, and its kind (Graphic, Shelf, Opto…). A new
  category, Multi FX, for plugins that do several things.
- Pina Studio is open source, under the MIT license, and has a website:
  https://manyeya.github.io/pina-studio/ (its downloads, Midnight Drive and Corner Store Blues to
  play track by track, and an agent building Corner Store Blues one command at a time).

### Fixed

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
