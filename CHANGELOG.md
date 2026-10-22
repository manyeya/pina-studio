# Changelog

What changed in each release of Pina Studio, newest first. Every change that someone using Pina
Studio (or an agent driving it) would notice goes under **Unreleased** in the same branch that
makes it; a release turns Unreleased into its version and date (see `docs/branches.md`).

Versions follow semantic versioning: the third number for fixes, the second for features, the
first for changes that break songs or commands made before.

## [Unreleased]

### Added

- Pina Studio is open source, under the MIT license, and has a website:
  https://manyeya.github.io/pina-studio/ (its downloads, Midnight Drive and Corner Store Blues to
  play track by track, and an agent building Corner Store Blues one command at a time).

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
