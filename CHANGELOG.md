# Changelog

What changed in each release of Pina Studio, newest first. Every change that someone using Pina
Studio (or an agent driving it) would notice goes under **Unreleased** in the same branch that
makes it; a release turns Unreleased into its version and date (see `docs/branches.md`).

Versions follow semantic versioning: the third number for fixes, the second for features, the
first for changes that break songs or commands made before.

## [Unreleased]

## [0.4.1] - 2026-11-20

### Added

- About Pina Studio, in the app menu and at the end of Settings: who made Pina Studio, Manyeya
  Khutso Siema, with links to manyeya.dev, GitHub, the website and the source; which licence
  covers what; the open-source projects it's built on most; and every open-source component in
  it, with its version, authors and licence (Show all). Settings' search finds it by credits,
  licence or author.

### Changed

- The licences come with Pina Studio. The app, the Windows installer and the zip carry:
  - `LICENSE`: the MIT License, for the source;
  - `COPYING`: the GNU GPL, version 3, for the builds, which include the Rubber Band Library;
  - `NOTICE`: which covers what;
  - `THIRD-PARTY-NOTICES.md`: every open-source component's licence, in full.
- The Windows installer shows `NOTICE` before it installs, and names its publisher and links;
  the Mac app's copyright line names Pina Studio's maker.

## [0.4.0] - 2026-11-16

### Changed

- Slicing a sample (Slice to Sampler, To Sampler, or Slice in the Sounds browser) before its
  audio has been read waits for it and slices it then, the status bar saying it's reading it
  and for how long, rather than asking to try again or leaving it unsliced.
- What takes a while shows that it's at work: progress that can't say how far moves; waveforms
  still being read breathe where they'll be.
- The Lyrics panel's hooks are rows like its lines (how often each is sung where a line has its
  time), each line's waveform in one quiet tone (the accent while it's heard), and Hear is a play
  button.
- The Drum Machine is now a drum rack and drum sampler. The rack sits on the left and its panel
  on the right, laid out to fit the pane with nothing cut off; the pane opens at the height they
  need:
  - **Drum Rack**: sixteen pads at a time, sized to the pane's height within limits, with an
    overview of all 128 notes beside them (drag it; ↑ ↓ a bank, ⌥ or ⌘ a row). Empty pads show
    their note and the General MIDI drum there. Each pad has mute, preview and solo, and the
    previous and next similar sound and a lock on hover (Swap all leaves locked pads). Under the
    pads, the picked pad's strip: its name, note and choke group, mute, solo, lock and swaps.
    Its title has Swap all, the kit's hot-swap, and its options (auto select, the swap buttons
    on the pads). Right-click a pad or chain to rename, duplicate, lock, show similar files,
    extract the chain to a track of its own, or delete it. Drag a pad onto another to trade,
    with ⌘ or ⌥ to layer it; several samples dropped on a pad map up a note each (with ⌘ or ⌥,
    all layer on it).
  - **Its panel**, by tabs: Sampler, Effects (the picked pad's own), Chains, Macros and
    Hot-swap.
  - **Chain list**: a row a pad, with on, solo and hot-swap; Receive, Play (moves the sound by
    the difference) and Choke (16 groups) with a preview; volume and pan; a send to each of the
    kit's return chains (up to six).
  - **Macros**: up to 16, shared with every pad; Map, then turn a pad's knob to set how far a
    macro moves it; Rand; variations kept (New), launched, overwritten and deleted.
  - **Drum Sampler** on the picked pad, a waveform over two rows of cards: the waveform (drag
    start and end; the similar sounds and hot-swap along its foot; right-click for Show Similar Files, Return to Reference, Save as
    Similarity Reference, Reverse); Start, Length and Gain; Attack, Hold and Decay with Trigger
    or Gate; Transpose and Detune. A playback effect (Stretch, Loop, Pitch Env, Punch, 8-Bit,
    FM, Ring Mod, Sub Osc or Noise) on and off, its two settings on knobs and an X/Y pad. The
    filter on and off as 12 dB or 24 dB low pass, 24 dB high pass, or a peak with its gain.
    Volume, pan, velocity to volume, and velocity or slide moving the filter, attack, hold,
    decay or either effect setting. Its layers as tabs, with how they play.
  - **Effects**: the picked pad's own, each opening its window, + to add one.
  - **Hot-swap**: the picked pad's sounds (like it, its pack's, its kind),
    or with D the kit's (drums picked to fit the song, or a folder's, loaded into it).
  - A new kit starts empty, its pads waiting for sounds (with no drums in the library at all,
    the synthesized kit as before). The Drum Machine's bar no longer has a Kit button.
  - Too narrow for three across, the Drum Sampler's cards wrap under each other and its panel
    scrolls, rather than squeezing on top of each other.
- The sampler has a hold stage, a 24 dB high pass and a peak filter, the sub at its own
  frequency, punch's release, the vintage sampler's grit fading out, a loop of its own, its own
  pan, and these all automate.
- A track has up to 24 modulators (it was 8), so a kit's macros leave its pads room.
- The Sampler's editor is laid out by what each setting belongs to. Along the top, captioned,
  the instrument's mode and character; under the sample, the picked zone's bar in its colour:
  Trim (start and length as sliders, and reverse), One-shot or Gate, its loop and the loop's
  crossfade, Trigger (as its note starts or as it's let go) and its choke group from a
  dropdown. The sample's start and end marks are bigger, with a grip to take hold of and their
  names. A zone's note (a range's low, high and root) is a dropdown of every note, an octave a
  row, and its other settings sit three and three. Too short for all of it, the editor scrolls.
- The Drum Sampler's Stretch Factor is how many times longer the sound lasts, 1× to 16×.
- An audio region opens in the new Audio editor (double-click it, or Open in Audio Editor from its
  menu): its file big, the part the region plays lit in its track's colour, its hits marked
  along the top and where it's playing. Drag its START and END handles to trim it (what it plays
  stays where it is on the timeline) and its fades' dots to fade it; under it, Trim (start and
  end as sliders, and Reverse, which plays it backwards from a reversed copy of its file and
  back), Fades, Level (an audio region has its own gain now, ±30 dB, its waveform drawn as loud
  as it plays here and on the tracks, white where it clips), Pitch (transpose and formants),
  Stretch and Time. The wheel zooms where it points.
- An audio region made a Sampler track, from the Audio editor's bar or the region's menu:
  Convert to Sampler Track puts what it plays across the keys (C3 as recorded, its transpose,
  fades and timing kept); Slice to Sampler Track cuts it into a sound a note from C1, at its hits
  or on the song's grid (every 1/16, 1/8 or 1/4 note, or bar).
  Either way a MIDI region plays it back where it was (at the speed and in the key it played,
  its loops going on as far as they did), the audio region is muted, and the Sampler opens on it.
- Any sample in the Sounds browser can be sliced: Slice, beside Add to song (Slice to Sampler on
  the Sounds page), cuts it at its hits or on the song's grid onto a new Sampler track, a sound a
  note from C1, with a MIDI region at the playhead playing them back in time (a loop fitted to
  the song's tempo and key, looping on). No audio track is left behind; one undo takes it back.


- Opening a song shows a loading screen while it takes more than a moment: Pina's mark with its
  steps playing, the song's name, a bar filling as each track's instrument and plugins load (and
  what's loading now), and a cell for every track, in its colour, lighting up as it's ready. Its
  audio files are read on a thread of their own while the plugins load, and the window keeps
  drawing between tracks rather than freezing till the whole song is in. The screen fades out
  over the song once it's open; a song that opens quickly never shows it.
- An audio region's end drags no further than its audio goes, as in Logic: dragging its bottom
  right corner on stops where the file ends instead of leaving an empty region after it (loop it
  from the top of its end for more).

- Plugins as MIDI FX: Scaler, MIDI-processing Audio Units, CLAP note effects and VST3 event
  effects go in a track's MIDI FX chain, before, after or between the built-in ones; what each
  hears is what's above it, and the notes it plays go on down the chain to the instrument (live
  and in a bounce). The MIDI FX window lists the MIDI effects found, and Any Plugin… searches
  every instrument too, for those (as Scaler is) that play notes out. A plugin slot opens its
  own window from the chain, keeps its settings when the chain changes, and saves them with the
  song. MIDI effect Audio Units no longer show among audio effects. The window's kind tiles grow to
  fit what they say.
- Tracks move by dragging their header up or down: a line shows where it goes, a stack's tracks
  stay in their stack and others stay out of it; one undo puts it back.
- The snap has every grid Logic has: Bar, Beat, 1/2 to 1/64 notes, 1/4 to 1/32 triplets, dotted
  1/8 and 1/16, and Off (ticks); Smart reaches 1/8 and 1/32 as you zoom in. Snap Relative moves
  regions by the grid while keeping how far off it they were. The lanes draw the snap's division
  (finer than the beats) where there's room.
- Automation: a click on the line puts a point on it there (it used to grab the line, so points
  needed the pencil); a drag still moves the line. ⌃⇧-drag bends a line, as Logic's does, and each
  line's bend handle shows faintly without hovering. Every lane has a Curves button: S-curves to
  smooth every line, slow or fast starts, straight lines, steps, smoothing out bumps and thinning,
  on the points picked or, with none, the whole lane (its right-click menu has S-curves and
  straightening too).
- Convert to Pattern Region keeps the region as it plays: the step its notes sit on (1/16, 1/16
  triplets, 1/32 or 1/32 triplets), all of its length (up to sixteen bars), or the fewest bars
  that loop through it unchanged; before, it took four bars at most and looped them, losing the
  rest. Pattern regions draw as steps in the arrangement: a row a sound, a cell a step, lit by
  velocity, held steps joined.
- MIDI FX open in a window of their own, built like the compressor's and the delay's: the chain
  down the left (each with its switch, what it's set to, moved up or down, taken out), the one
  picked on the right, drawn as it plays (the arpeggio over two bars, the chord on a keyboard, the
  scale's notes on an octave whose keys pick its root, the chord before and after transposing, the
  velocity curve, the repeats fading, sixteen randomized notes, the controller sent on, the MPE
  shapes), its numbers as dials (drag, ⌘-click to reset) and its choices as button grids. Adding
  one picks from tiles saying what each does. Hear It plays the song key's chord through them for
  two bars. The window moves by its top and stays open while you work. The strip's MIDI FX "+"
  sits in the middle, as the effects' does.
- Pina takes Logic Pro's default keys, wherever it does the same thing, so Logic's hands work
  here. Keys go to the area clicked last, the tracks or the editor (whose frame lights while it
  has them), and Tab passes them across, as Logic's key focus does; before, the piano roll took
  every arrow while it was open. In the piano roll: ← → pick the note before or after (⇧ adds
  it), ⌥← ⌥→ move notes by the nudge value (⌃⌥← ⌃⌥→ a quarter step, as ⌥ did), ⌥⇧← ⌥⇧→ change
  their length, ⌥↑ ⌥↓ and ⌥⇧↑ ⌥⇧↓ transpose, = and - change velocity by 1 (⇧ by 10), ⌘T splits
  at the playhead, ⌘J joins, ⌘R repeats, ⌘[ ⌘] trim to the playhead, ; and ⇧; move to it, ⌥D
  takes out doubled notes, ⌃M mutes, ⇧Home ⇧End pick the first and last. In the tracks: ⇧← ⇧→
  add the region before or after, ⇧Home ⇧End the first and last, ⌥⇧← ⌥⇧→ resize, ⇧; to the
  playhead then the next, ⌥⌘T split at the cycle, \ remove overlaps, ⇧\ trim to the next, ⌘\
  crop to the cycle, ⌃\ fit MIDI to its notes in bars, ⌥⌘Q dequantize, ⇧N rename, ⌥⇧N name
  after the track, ⌃A alias to copy, ⌥⇧A pick its aliases, ⌃⇧T move to the selected track, J
  and ⌃L for join and loop, ⌥⌘A ⌥⌘S new audio and instrument tracks, ⌃⌘→ ⌃⌘← open and close a
  stack, ⌥⌘↓ ⌥⌘↑ ⌥⌘Z ⌥⌘⌫ the track's own height, ⌃⇧M unmute all, ⌥S unsolo all, and ⌃⌘X ⌃⌘I
  ⌃⌘R cut, insert silence or repeat between the locators. Both: Logic's Select keys (⇧F, ⌃⇧F,
  ⇧L, ⇧O, ⇧E, ⇧S, ⇧P, ⇧M, ⇧I, ⇧D). Everywhere: ⌘U, ⌃⇧, ⌃⇧. ⌃Home ⌃End, ⇧⌘, ⇧⌘. ⌥⇧⌘, ⌥⇧⌘.
  for the cycle and selection, ⌥⇧' ⇧' ⌥⌫ for markers, ⇧K count-in, ⌘. throws a recording away,
  ` follows the playhead, ⇧` scrolls to the selection, ⌃⇧Z zooms to the cycle, ⌥⇟ ⌥⇞ the next
  tool, ⌃⌥D ⌃⌥B ⌃⌥M ⌃⌥T the nudge value, B the Browser, ⌥K the shortcuts, ⌘2 ⌘4 ⌘5, ⌃⌘F full
  screen, ⌘M, ⌃⌘M, ⌘H, ⌥⌘H, ⇧⌘N a new empty song and ⇧⌘E stems. ⌘D now adds a track with the
  selected one's settings and no regions (as Logic's); ⌥⇧⌘D duplicates it with them. The
  Plugins page moved to ⌃⌘P (Logic's plug-in search) and the sample library's page to ⇧B, their
  old keys now Logic's. With the mouse, as Logic: ⌃-drag notes or regions finely (⌃⇧ or ⌃⌥ by
  the tick), ⇧ lines up the dragged notes' ends and ⌥⇧ makes them as long, ⌃⌘-drag a note for
  velocity, ⌃⇧-click writes off the grid, ⇧-click the roll's keys adds their notes, ⌥-click Mute
  or Solo for that track alone and ⌘-click for every track, ⇧-click a lane for the playhead,
  ⇧-double-click a region for every region on its track, ⇧-click the cycle to stretch it. The
  shortcut sheet lists them all.
- The Songs page is redesigned. One slim row along the top: Songs, how many and where they're
  kept, Open folder, Study a song, Back to the open song and New song (⌘N). New song opens a
  panel over the page with the new song's name, tempo, key and time and the templates as tiles
  that show their tracks, each with a Create button; Escape, its ✕ or a click off it puts it away.
  With no songs yet the panel is on the page itself. Under the row, every song as a
  list (cover, name, tempo, key, time, tracks, length, when edited, grouped Today, This week,
  This month, Earlier) or as a grid of covers; Pina remembers which. Songs can be searched,
  filtered by tempo, key and time, and sorted by when they were edited, name, tempo or length
  (or by clicking a column's name). Each song's ⋯ and Trash are always shown, and its box picks
  it. Covers draw the arrangement across their whole height, painted at once, so a long library
  stays quick.
- A long message in the status bar stays on one line and ends in "…" instead of running across
  the bar.
- The Synth is now called Synth-0. Songs that use it open as they did, and the `set_synth` command
  and its settings are unchanged; it takes macro names now too.
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
- The picker a mixer slot opens narrows down like the Plugins page: All, Built in or Plugins;
  Starred and Recent; the categories, the kinds in the one picked and the makers, each showing how
  many it would find; a count of what's found and Clear. Rows have a star, the list shows every
  match instead of the first 60, and the arrow keys scroll to the one picked.

### Fixed

- A kit's pads stayed its own only until the song was closed: reopened, they'd lost their kit.
  Songs saved that way find their kits again when they're opened.
- A one-shot sound that never reached its end (a pad's Loop effect with the envelope holding,
  or a stretched or frozen sound standing still) kept sounding for ever; it now plays out once
  it's had its length (stretched, sixteen times its length).
- Audio regions drew their waveforms squeezed and cut short (to 92% of where they play, at 44.1
  kHz against a 48 kHz song) whenever the sound output ran at another rate than the song, as
  Bluetooth headphones can: a loop looked to end early, an empty gap before its next repeat. It
  played right all along; it now draws as it plays.
- Time-stretching an audio loop that follows the song's tempo (⌥-dragging its end) stretched it
  the wrong way: dragged to twice as long, the region doubled but its audio played twice as fast,
  filling a quarter of it. It now slows to fill the region (and speeds up to fit a shorter one).
- A loop that follows the song's tempo no longer says "Stretched ×1.01" (a stretch it kept from
  before, which it doesn't use).
- A menu opened from a window (the MIDI FX window's Any Plugin…, a plugin window's, the MPE
  window's) showed behind it; menus now show over every window.

- The sound can be moved off the speakers to Bluetooth headphones: an output that can't play the
  engine's rate gets the engine started again at one it can, the song and playhead as they were.
  With the system output picked, Pina follows macOS when it moves to another output, and goes back
  to an output that was picked when it's plugged in again.
- Recording from an input that can't run at the song's rate (a headset's microphone) records at the
  input's own rate instead of failing, and the take is placed at the right length.

- A click on one of several notes picked picks it alone (⇧ still adds, a drag still moves them
  all), so a tool or shape goes on that note; before, it stayed one of the group, and after
  Generate or Chords had written (which picks what it wrote) a shape went on everything.
- The editor's tabs fit beside its controls: Steps, Drums and Loops on their tabs (their full
  names in Settings).
- The mixer's channel panel names a plugin instrument's real format (Audio Unit, VST2, CLAP); it
  called every one VST3.
- Every effect stacked on a channel shows on its mixer strip. The strips' effect rows grow to fit
  the busiest channel (up to twelve), and past that a strip scrolls them; before, anything past
  six hid behind a small "more" label, so a second or third reverb looked as if it hadn't gone on.
- The built-in effects' windows (Compressor, Saturator, Limiter, Reverb, Delay and the rest), the
  Channel EQ and Synth-0's window grow and shrink as a whole by their corner, which shows a grip:
  everything in them (type, knobs, pictures, gaps) scales together and nothing moves about inside.
  Before, a bigger window spread its controls apart or cut them off.
- The playhead is on the right bar when Pina runs at another rate than the song (Bluetooth
  headphones at 44.1 kHz playing a 48 kHz song, or 96 kHz picked in Settings). It counted the
  engine's samples at the song's rate, so it ran 8% behind at 44.1 kHz, falling further back every
  bar and badly out of place after each loop, and twice as fast at 96 kHz. Notes recorded and
  automation written while playing land where they were played for the same reason, and the EQ's
  analyzer and Synth-0's spectrum show each frequency where it is (they were 8% off).
- The playhead shows what's being heard. It used to run ahead of the sound by the output's delay
  and move in buffer-sized jumps; it now follows when each block reaches the speakers, so it sits
  on the beat at any buffer size, counting the stream's own latency too (about 12 ms on a
  MacBook's speakers, left out before). Pressing Play or clicking somewhere while playing no longer
  flashes the playhead back to where the last play was: it waits where play starts until that's
  heard. Looping the cycle no longer runs it past the cycle's end. Stop leaves it where the sound
  stopped.
- Bluetooth headphones say they're less late than they are, so Settings › Audio has Playhead in
  time: how much later than it says the output's really heard, picked, typed, or measured by
  tapping along to the click (Space or the pad, ten taps; taps that don't agree are asked for
  again). It's kept for each output, a headset on a call apart from it off one.
- Settings › Audio scrolls: anything below the window was out of reach. Device rows sit inside
  their cards' rounded corners.
- Settings › Audio no longer opens a Bluetooth headset's microphone for its level meter. Opening it
  turns the headphones down to call quality (16 kHz, one channel, no highs or lows) for everything,
  which made notes sound thin and high ones vanish, and made Pina refuse the headphones ("can't run
  at 16000 Hz"). A headset on a call can now be played through (an output of one channel gets both
  sides), the page says it's on a call and why, and Pina goes back to the headphones' full rate by
  itself when the microphone's let go.
- Typing a name (a sound saved in Synth-0, a pack, any field) types it. Before, the letters played
  notes or toggled views while the field had the keyboard.
- ⌘W closes the window in front (an effect's, the EQ, Synth-0's, the shortcuts) instead of
  quitting Pina; with none open it quits as before.
- Synth-0's close button and the arrows either side of its patch's name show again.
- A slot's picker offers a second Channel EQ, Compressor, Saturator or Limiter on a channel that
  has one, and the other built-ins on buses and the master too; it used to hide them.

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

- Flip, in the Sampler: a chopped sample (Slice to Sampler, or Slice in the Sounds browser)
  rearranged as on an MPC. Its chops along the top, each its waveform and number, to hear with a
  click or drag onto the sequence; the sequence under them, each chop its waveform, in time, a
  playhead going through it as it plays. Drag a chop onto another to swap them, or one from the
  top onto a place to play it there. Pick one to stutter it (×2, ×3, ×4), play it backwards (a
  reversed copy of its sound, made once) or silence it. Flip makes a new arrangement every press
  (the first chop on the downbeats, chops said again and answered); Turn round, Half time,
  Double time, and As it was; Play it loops the region with its track soloed (the solo let go
  when it stops), so only the flip is heard. Where the chops are cut is yours: the whole sample
  above them, each chop in its colour, a cut a white handle. Click the sample to cut there (the
  sequence plays as it did, the chop it split now two), drag a handle to move a cut, double-click
  one to take it out. A sliced sampler's chops choke each other, so a chop stops the one before
  it rather than playing over it. Every change is an undo step.
- Smart Slice, in the Audio editor (in place of Lyrics): any recording sliced as a producer
  would, in one click, nothing asked. It listens to what's in it by its sound, never its name:
  how much of its sound is a voice, as the quickest separation model with vocals hears three
  short stretches (about ten to fifteen seconds; a vocals stem is nine-tenths voice, a song a
  fifth, a beat none). Then it slices it that way, the panel saying what it's doing and how far
  (listening, taking the voice out, cutting) with Stop. A beat is cut at its hits, each slice
  named for what it sounds like by the sample analysis (Kick, Snare, Closed Hat, Chord Am, Bass
  F#; a hit or phrase in its key when a mix's slice is neither). A voice alone is cut into vocal
  chops, never named for an instrument: by their notes, then, its words heard at once by
  Whisper, by the words sung in them. A voice in a beat is cut at its hits, voice and all (as a
  soul sample is chopped); its words are a tab away, and the voice can be taken out first. What
  it heard is in the panel's header, to put right if it's wrong (kept, and sliced again that
  way); without a model to listen with, it guesses, and says so. Only the stretch the region
  plays is sliced, each slice to where it dies away. The slices: how many of each kind; a
  dropdown to narrow them (drums; chords, notes and phrases; with a voice; or one kind); a row
  each with where it is, its kind's icon, its name and waveform, to hear (click the row); To
  Sampler puts those shown on a Sampler track, a pad each, named, played where they play now
  (one undo takes it back). Their names are drawn on the waveform (faint ticks when there are
  too many to read). Flips flips them as on an MPC (a new groove from the hits, chords
  re-sequenced, half-time, rolls, a bounce, reversed into the beat), a voice's words as hooks.
  Kept with the song, so slicing it again is instant. The Studio Agent has it too (`slices`, then
  `flip_words`), and `pina slices <file>` from the command line.
- Flips, beside a region's lyrics (the Lyrics panel's Flips tab, in its header beside Lines):
  the Studio Agent's engine reads the words sung and how long each lasts, and writes the hooks a
  producer would make of them: words from anywhere in the vocal put in a new order on the grid so
  they say something new, pitched up and sped up (chipmunk soul) or down and slowed, stuttered,
  answered a bar later, reversed into the beat. Say what you want in your own words ("darker",
  "make it say never let go") and Find (or Enter); under it, the engine (Claude Code or Codex,
  with its logo), its model and effort for flips (kept for each engine apart from the Studio
  Agent's conversation, so flips can run light and fast), and a style. While it works, Find is
  Stop in its place, the list's header says what it's doing and for how long ("Writing 3 of 6 ·
  23s") over a line filling as flips are written, the next one waits as an outline, and each
  flip shows the moment it's written. A flip is a row, as a line is: what it says, its bars and
  pitch, hear and Use it; its chops on the bars, each numbered, with its waveform, in the colour
  the Sampler gives it (reversed and pitched ones marked); its name and style, how many chops and
  hits; while it's heard (click the row), a playhead going through and why it works. Use it puts
  it on a Sampler track of its own (a zone a chop, gated, choking each other; a MIDI region
  looping for 8 bars from the playhead's bar), opened in the Sampler to flip further; one undo
  takes it back. 6 more asks for others; Start over asks afresh. When the engine can't answer,
  it says why in plain words (not signed in, out of usage, no connection), with Try again and
  the other engine. The Studio Agent can flip words itself (`flip_words`, with the `lyrics`
  tool's words and their times); `pina flips <song> <track> <region>` prints them.
- Lyrics: Smart Slice's Slice the words hears the lines sung in an audio region's file with
  Whisper (a model you download in Settings › Stems & lyrics: OpenAI's, MIT; on the GPU on Apple
  silicon). A whole song has its vocals taken out first, by the best separation model with
  vocals you have, and is heard there; a vocal alone is heard as it is. Beside the waveform: the
  hooks first (each line sung more than once, shown once, as a card with how often it's sung and
  its waveform), then every line, each with its own stretch of the waveform, a tick where each
  word starts, and when it starts. Each has ▶ (lit while it plays), Loop · N bars, which puts it
  on the track's lyrics track as a loop on the bars: from the bar line it starts after, its
  pickup with it, for 1, 2, 4 or 8 bars, playing the vocal alone when it came from a song; and
  Chop, which cuts it at its words onto a Sampler track of its own (a chop a word, a MIDI region
  playing them as sung) and opens it in Flip. Drag a line or hook onto the tracks to put its loop
  there, on the bar under the pointer.
  Each word is drawn on the waveform where it's sung. Loop the hooks loops every line sung more
  than once at a stroke. The panel says what it's doing (downloading Whisper, which it offers
  when there's none; taking the vocals out, how far; hearing) and, when it hears nothing, what to
  try. Lines are heard once per file and kept with the song (nothing heard isn't); what Whisper
  makes up over music is left out. `pina lyrics <file>` prints them.
- The Studio Agent separates and hears too: `stem_models`, `stems` (a region's stems on tracks
  in a stack) and `lyrics` (a region's lines, its vocals taken out first if it's a song), with
  the `lyric_loop` edit to make a line a loop on the bars and `add_stems` to put stems in.
- Stems: an audio region separated into vocals, drums, bass and the rest by a separation model
  you add. Pina Studio ships with no model and downloads none. A model is a folder in Models
  (in Pina Studio's Application Support, or %APPDATA% on Windows) with its ONNX file and a
  model.json saying what it gives and how it reads sound: as samples (HTDemucs), as a
  spectrogram (MDX-Net, Mel-Band RoFormer) or as a spectrogram's real and imaginary parts
  (BS-RoFormer), so the separators people use all work. Separate Stems… in an audio region's
  menu (or Stems… in the Audio editor) opens a sheet: the model picked from a dropdown of those
  you have, each with how long this file will take (or that it's made already), and under it the
  picked one's stems, how well it did and its licence;
  more to download there; the stems to keep; whether to mute the region. It runs in the
  background, the sheet showing what it's doing and the time left, with Stop and Hide. Each stem
  goes on a track of its own, playing as the region did, all of them in a stack; one undo takes
  it back. Stems are kept per file and model,
  so asking again is instant. Settings › Stems & lyrics walks through it, lists the models that have
  been run (what each gives, how well, how long a song takes, its licence as its host states it)
  with a Download button that fetches one from where its maker keeps it, and your own, to show
  or remove; Add a model you have… takes an ONNX file, knowing the tested ones by their size.
  `pina models` lists the models, `pina models get [name]` downloads one, `pina models inspect
  <file.onnx>` shows a model's inputs and outputs to write its model.json from, and `pina
  separate <file>` writes a WAV per stem. Models run in ONNX Runtime on the CPU, or the GPU when the model.json
  asks; on Apple silicon (so that half of the app needs macOS 13.4) and Windows x64. Intel Macs
  can't run models. See `docs/separation-models.md`.
- Bloom, a new built-in instrument: sounds grown from a seed. A seed is 48 genes, each made to
  sound good however it's changed (envelopes, a contour, an LFO, two wavetable oscillators with
  FM, feedback, ring, fold, sub and noise, a filter swept from low- to high-pass, a resonator tuned
  to the note, crush, tilt, and an echo and a space the whole plant shares). Twelve branches grow
  out of it, one for each note of the octave: drag a flower out and its sound strays from the
  seed, sideways and it strays another way at the same distance, and the branch's note plays
  while you drag. The Growth knob, the mod wheel and each MPE note's own slide grow every branch
  further, live under a held note. Keys can play a branch per note, per zone, per velocity, a
  layer of branches at once, the next branch each note, or one branch everywhere.
  Make any branch the seed, clone it to all, cross two branches into a child, nudge the seed, lock
  genes so they never stray, or steer what one branch grows a gene to. Every seed it grew from is
  kept, to go back to. The Strand page shows every gene down a turning double helix, with knobs
  for the seed's genes and for a branch's own steering. Grow makes a seed sound like a recording:
  pick up to two seconds, and four strands, each breeding seeds of its own kind on a core of its
  own, find closer and closer ones (about 80% alike, in a few seconds, on sounds it can make); click
  one to hear it, plant the one you like. Everything is on this machine. Forty factory seeds and
  every plant you save are on its Sounds page, searched and filtered by kind. The garden is drawn
  from the plant's own genes: each flower's petals from its brightness and attack, its breathing
  from its LFO, sap running up the branches that play; it rests when nothing's happening. Every
  control, branch and gene of the seed is automatable and modulatable. Agents plant and shape it
  with `set_bloom`.
  Seeds come in 29 kinds in seven families (low end, keys and bells, plucked and bowed, leads and
  voices, pads and spaces, drums, effects), picked from one panel over the garden. Its Sounds
  page has 232 factory seeds, eight of each kind, each card a flower drawn from its genes and a
  line saying what it's like (worked out from its genes, so it's true), sorted newest or by
  name, narrowed by where they're from, a family or a kind, or one picked at random. Grow takes
  up to four recordings at once (dropped from the Finder or the Browser, or chosen): different
  notes, or the same played softer and harder, and the seed grown to match them all. Bloom's
  window draws its own frames: while the garden moves, nothing else in Pina Studio is drawn
  again.
- Synth-0 has effects of its own, up to eight, part of its sound and kept with it. The Effects page
  shows them as a chain across the top (its voices, each effect, the track): each a tile with its
  switch, a little picture of what it does, what it's set to, what rides it and its mix, dragged
  to change the order, and a switch for the whole rack. Under it the effect picked is shown as the
  mixer's own editor shows it, its picture to drag, every control and its meters (the EQ is the
  whole EQ, a click on it makes a band), with the sources that can ride its knobs and those that
  do. Racks can be saved and loaded by name, a factory sound's too.
- Add an effect opens a browser of all 37: searched by name or what they sound like (warm, wide,
  shimmer), narrowed to drive, motion, space, shape or those added lately, each drawn and said in
  a line, and heard in the sound as it plays when pointed at.
- An LFO's rate can be any length from a sixty-fourth to 32 bars, straight, triplet or dotted, or
  free from 0.01 to 200 Hz, or any length typed (3/16, 1/8 t, 2 bars, 4 hz). Switching between in
  time and free keeps its speed.
- Notes are named with middle C (MIDI 60) as C4, as most people learn it: the C3 below it now
  sounds as low as a C3 should, and the top note is G9. Settings › Editing names it C3 or C5
  instead, as some apps do.
- Picked automation points edit as a group. Two or more get a box round them: drag its left or
  right edge to stretch or squeeze them in time (on the grid unless ⇧, none passing a point not
  picked), its top or bottom to spread or flatten their values, with ⌥ about the middle. ← →
  move them a grid step (⌥ a quarter of one), ↑ ↓ raise or lower them (⇧ ten times as far), ⌘D
  puts them again straight after themselves, ⌘A picks the whole lane, Escape lets go. Paste and
  duplicate pick only what they put, not the points that keep the line either side.
- A sheet of every keyboard shortcut (⌘/ or ?), grouped by what they work on and searched by
  what they do or by their keys ("⌘D" finds every duplicate). It reads the keys from the app
  itself, so it's never out of date. New keys: Home and End to the start and end, ⌘= and ⌘- to
  zoom, ⌘A for every region (or a lane's points), Escape back to the pointer and then nothing
  picked, and T then a letter for a tool (T T pointer, T P pencil, T I scissors, T E eraser, T X
  text, T G glue, T S solo, T M mute, T Z zoom, T F fade, T R stretch). A menu item showing a
  letter is picked by that letter.
- The Studio Agent works Synth-0's sounds with `synth_sounds`: lists and searches the library (by
  words, part or pack), saves a track's sound into it with a name, part, words, a line and a pack,
  loads one onto a track, makes packs and moves sounds between them. With the song open in Pina
  the window and its library follow at once.
- Synth-0's sounds are a library, a page of the window: the factory's, those saved and packs of
  them, searched and narrowed by part and by what they sound like, liked and played lately, the one
  picked shown whole. Save keeps a sound with its name, part, words, a line about it and its pack
  (the factory's stay as they are); the title says when it's changed, compares it with how it was
  saved and takes it back. Packs are made, named, given a cover, shared as one `.pinapack` file and
  brought in (or dropped anywhere in Pina) as packs of their own. A sound taken out can be brought
  back.
- Settings is grouped (studio, working, library) and searched. Audio shows what's playing (through
  what, the rate, the buffer in ms, the engine's load, buffers late in the last ten minutes), a
  test tone each side, every output and input with the channels and rates it has and the inputs'
  levels live, the rate to run at, the buffer with its latency, how far takes move back for the
  input's lateness, which output pair the song plays out of, and the cores making sound.
- Recording has a page: 16, 24 or 32-bit float takes, a count in of 1, 2 or 4 bars, and the click's
  sound (click, wood, beep, tick), level, beats, eighths or sixteenths, the bar louder, and heard
  only while recording. Plugins has a page of its folders, what's found and any folder to add;
  Sounds & packs the name on what's shared; Saving & files how often a recovery copy's kept.

- The piano roll is rebuilt around a panel of five tabs. Notes shows the picked notes' pitch,
  start, length, velocity, velocity range, chance and release (drag a value, or step it), with mute (0)
  and picking the top or bottom note of each chord, or the rest. Transform has its tools and
  more: staccato, flam, grace note, connect (passing notes between notes), twice and half speed,
  crescendo and fade, recombine (pitches, velocities or lengths rotated in time order, or
  shuffled) and into the key. Generate shows one writer at a time: a rhythm spread evenly (3
  hits in 8, and so on, its steps drawn, named when it's a known one: tresillo, cinquillo, bossa
  nova, samba, bell, four on the floor), a melody grown from a seed in the key, or one following
  a shape (rise, fall, arch, valley, wave, each drawn). Chords shows the key's chords, coloured
  by what they do (at rest, leaning away, pulling home), and those it borrows (a click plays one
  and adds it); or progressions by mood, each to hear before taking. The chords picked stay at
  the tab's foot with how they're voiced (voice-led, close, open or spread) and played (held,
  strummed, arpeggiated, pulsed, on the off-beats, in a rhythm or broken), a bass under them if
  asked, and the button that writes them; each writing tab says what it writes over, and keeps
  replace or add and its button in sight. The panel's tabs never spill past it: shorter names
  when it's narrow.
- Each note has a chance of playing (rolled every time it comes round, so a loop varies), a
  velocity range it strays within, and can be muted (drawn hollow, kept, not played). The lane
  under the notes has Chance as well as Velocity, and shows each note's range. The agent writes
  them too (`chance`, `spread`, `muted`).
- The ruler names the chords the notes play, beat by beat.
- MPE: each note's own pitch, slide and pressure, kept on the note as lines of points, so they go
  wherever it goes: moved, copied, pasted, transposed, split, stretched. The Pitch, Slide and
  Press lanes edit them: a click adds a point, a drag draws (thinned to the points that shape it
  when let go, over every note it crosses, or the picked ones), a point drags, a double-click
  takes it out, ⌥ on a line bows it into a curve, ⌥-drag rubs out, ⇧-drag scales the notes'
  lines; the lane shows a scale and every note's line, the picked ones bright. With Pitch
  showing, a note bends right where it is in the grid, dragged up or down from its middle, its
  points there to take hold of; bends land on whole semitones (⌘ for between) and a readout
  says how far. The roll's new MPE tab writes shapes on the picked notes (or all) with a click:
  scoop, fall, bend up or down, vibrato, glide into the next note, a slide opening, closing or
  wobbling, a pressure swell, fade, accent or tremolo, as far and as fast as asked; copies one
  note's expression to others (stretched to each one's length), clears it, and says whether the
  track's instrument hears it, with a button to make it if not. The agent writes it on notes
  (`expression`) and shapes it (`edit_notes` `express`, `unexpress`).
- An MPE controller plays and records each note's own pitch, slide and pressure, and Settings
  has a MIDI page: each controller's MPE on (found from how it plays, or its own MPE message,
  lower or upper zone) or off, and how far its notes bend until it says. Synth-0 follows them
  voice by voice (its matrix has Slide as a source); VST3 and CLAP instruments hear them as note
  expression; an Audio Unit or VST2 with MPE on (its slot's menu, the roll's More, or the MPE
  tab) hears each note on a channel of its own, its MPE card setting the zone, how many note
  channels and the bend range to match the instrument's.
- Release velocity: how fast each note is let go, recorded from a keyboard that sends it, drawn
  in its own lane (Rel), set in the Notes tab, and heard by plugins.
- MIDI FX, on every instrument track before its instrument, for the song and live playing alike:
  Arpeggiator (rate, order, octaves, gate, latch), Chord Trigger (eleven shapes, in key or not),
  Scale Quantizer (the song's key or its own), Transposer, Velocity Processor, Note Repeater,
  Randomizer, Modifier (one controller sent as another) and MPE Shaper (each note's bend scaled
  and drawn toward the nearest semitone, its slide and pressure curved into a range, or swapped).
  A note's own expression follows it through what transposes, quantizes or chords it. The mixer
  has a MIDI FX row; its slot
  opens a card for each, to set, bypass, reorder and take out. The agent sets them with
  `set_track` `midi_fx`.
- Synth-0 has effects of its own: a rack of up to eight, after its voices and before the
  track's own effects, kept with its sound, so every patch comes with its own (pads their
  ensemble and hall, plucks their ping-pong delay and plate, acid its tube drive and tape echo).
  The window's Effects page is the rack: the effects in the order they run, each with its
  switch, moved earlier or later or taken out; the one picked open with its knobs large. Add
  them by name: drives (tape, tube, transformer, crunch, heavy, rectify, foldback, lo-fi),
  motion (chorus, ensemble, flanger, phaser, vibrato, tremolo), space (room to cathedral and
  shimmer, digital, tape, analog, ping-pong, dub and slapback delays) and shaping (filter, EQ,
  compressor, multiband, width, transient shaper, gate, limiter, pitch, tape stop). Its LFOs,
  envelopes and macros ride any of their knobs (drop one on a knob, or pick it in the matrix),
  so do the track's modulators, and every knob automates; moved or taken out, an effect takes
  what rides it along. The agent sets them with `set_synth` `fx`.
- Synth-0 has a new window, laid out the way sound flows: its sources down the left (each with
  its level and which filter it goes to), the one picked open in the middle with its wavetable in
  depth and its warps drawn into it, both filters on one curve over a live spectrum of what's
  playing, and what comes out on a scope. Under them, a chip for every envelope, LFO and playing
  source: picked to shape it, dragged onto any knob with a cord to see where it lands, and pointed
  at to light up everything it moves. Colour means one thing: what's moving a knob, its ring and
  a dot showing where it is right now. Every setting today's window had is still there, with the
  oscillators' start phase, noise pan, envelope delay, voice count, bend range, glide held, and the
  arpeggiator's octaves, gate and swing in reach for the first time. Knobs reset with a
  double-click and offer "Back to", "Automate it on the track" and taking a source off with a
  right-click.
- Synth-0 plays from its window: four octaves of keys (lower on a key is louder), bend and mod
  wheels, and typing A to K while it's open. Its notes go to its own track, and record when that
  track is armed. Play a demo runs a phrase for the patch's part.
- Its sounds have a drawer: search by name or sound, narrow by part, like the ones you keep coming
  back to (they stay liked), and hear one before choosing it. The wavetables open as pictures to
  choose from, and search too.
- Every patch has eight named macros that do something: Bright, Bite, Motion, Width, Weight, Air,
  Attack, Tail. At 0 they leave the patch as it was made; turned up, each changes it, on all 30.
  Double-click a name to rename it, and the automation list calls them by name. Macros cost
  nothing while they're down. A synth in a song made before gets them with one click, its sound
  unchanged until they're turned.
- The matrix narrows to envelopes, LFOs, macros or playing sources, scrolls when a patch has many
  routes, and dropping a source on a knob it already moves no longer adds it twice.

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
