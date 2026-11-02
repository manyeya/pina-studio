# Task: Create a similar song from a spectral reference

## Outcome

A person can give Pina Studio a song's audio or a spectrogram image and ask for a **new, editable song with a similar musical shape and sound**. Pina studies the reference, explains what it could and could not infer, writes parts on ordinary tracks, renders the result, and offers targeted revisions. The song remains editable with Pina's existing instruments, samples, plugins, and mixer.

Audio is the preferred input. A spectrogram image is useful when audio is unavailable, but its results must be labelled as estimates. This task does not promise recovery of the original recording.

## Current starting point

- Song Study already reads WAV, AIFF, FLAC, and MP3 and reports tempo, key, chords, form, groove, likely parts, and mix. See `crates/engine/src/study/` and `crates/app/src/cli/study.rs`.
- Its frame analysis already computes onsets, chroma, timbre, and level from short-time spectra. See `crates/engine/src/study/features.rs`.
- **Start a song like this** currently creates an *empty* project with the study's tempo, key, meter, playbook, and mix reference. See `crates/app/src/pages/study/shelf.rs`.
- The agent can write and edit project parts through Pina's commands, and Pina can render, analyze, critique, mix, and master them. There is no image study or reference-to-render comparison loop yet.

## User flow

1. In Song Study, drop an audio file or a PNG/JPEG spectrogram. The CLI and MCP tool also accept a local reference path.
2. For an image, Pina identifies the actual plot and its time and frequency axes. The person can correct the crop, duration, frequency range, and linear/log/mel scale. If these cannot be established, Pina asks for the missing values before making precise claims.
3. Pina shows a **reference blueprint**: sections and energy, rhythmic events, tonal contours where visible, and relative frequency balance. Each finding has a confidence level and says whether it came from audio or an image. Image-only study does not show invented exact chords, instruments, LUFS, stereo width, or lyrics.
4. **Create a song like this** opens a new project and asks the agent to compose a full arrangement from the blueprint. The user can specify style and how closely to follow rhythm, form, and sound. The agent writes new parts and selects available sounds; the result is a Pina project, not a single opaque audio clip.
5. Pina renders the draft and shows where it differs from the reference by section: groove, tonal contour, frequency balance, and energy. The person can request a revision such as “closer drums” or “brighter chorus.” Each revision is undoable.

## Technical approach

### 1. Shared reference blueprint

Define a compact, versioned representation independent of input type. It should contain source kind and provenance; duration and axis calibration; time-aligned frequency energy; onset strength by broad frequency range; tonal/harmonic contours; section boundaries and energy; and confidence or `unknown` for every inferred feature. Store detailed arrays in the study cache rather than inflating `project.json`. Projects should retain a link to the reference and the selected creative controls.

Audio input should reuse the existing Song Study analysis and add only the spectral detail needed for comparison. Keep the existing study and playbook fields compatible, with an explicit cache-version migration or regeneration path.

Image input should first support clearly labelled spectrogram exports. Parse plot pixels into *relative* energy after crop, axis, and colour-scale calibration. Support screenshots with manual calibration. Do not convert pixel colour directly into absolute loudness unless a known scale is supplied. Preserve uncertainty from low resolution, lossy compression, missing labels, or unknown analysis settings.

### 2. Composition

Give the agent a concise blueprint rather than a full image matrix. It should plan sections, tracks, sound choices, and motifs before editing. Reuse Song Study, playbooks, sample search, instrument presets, theory, and Pina's existing edit commands. Generate a complete first draft with variation and transitions, then render it. The reference guides form, groove, and texture; the agent writes new notes and lyrics if requested.

If the input is an image, derive only what its calibration supports. For example, a visible horizontal harmonic line may guide a sustained pitched part, but it is not proof of a specific instrument or chord.

### 3. Render comparison and revision

Analyze the rendered draft into the same blueprint. Align by bars and sections, then compare multiple features: onset placement, tonal contours, broad frequency balance, and energy through time. A single pixel-level spectrogram score would reward superficial matches, so the comparison must report musical differences and identify which tracks or sections the agent should revise. For image references, use relative measures only where absolute calibration is unavailable.

Show a small set of actionable differences in Song Study and the Agent panel. Revision should be bounded, preserve unrelated user edits, and stop when the requested change is achieved or further passes cease improving the relevant measures. Keep each agent turn undoable.

### 4. Spectrogram-to-audio experiment (separate, optional)

Evaluate phase estimation from a clean numeric magnitude spectrogram, then from an image with known transform settings. This may produce a rough audio preview of what the image encodes. It is **not** the song-creation path: a screenshot discards phase and often resolution, stereo, absolute level, and transform metadata. Do not make this experiment a dependency of the editable-song feature or present its output as the original recording.

## Delivery order

1. **Reference and comparison foundation:** versioned blueprint; audio extraction; render extraction; section-aligned comparison; tests using known tones, sweeps, beats, and real project renders.
2. **Image study:** import, calibration UI, relative feature extraction, uncertainty display, CLI/MCP access, and image test fixtures with different axes and colour scales.
3. **Song creation:** agent planning and project creation from either source, sound selection, full arrangement, render, and an initial comparison report.
4. **Guided revision:** per-section feedback, user controls for similarity, bounded agent revisions, undo, and evaluation on several genres and reference qualities.
5. **Optional inversion research:** a prototype and listening evaluation, kept outside the main workflow unless it proves useful.

## Acceptance criteria

- A labelled spectrogram image can be imported, calibrated, and studied without audio. Missing calibration produces an explicit request or `unknown` fields, not false precision.
- An audio reference follows the same creation flow and retains the richer information available from Song Study.
- **Create a song like this** produces a playable project with editable tracks and a complete arrangement, not only an empty project or bounced file.
- The comparison identifies section-specific differences. On representative references, a targeted revision improves the requested measure without systematically degrading the other measures or deleting user edits.
- The UI clearly separates measured facts, estimates, and creative choices. It does not claim exact reconstruction from an image.
- CLI and MCP access expose the same study, creation, and comparison capabilities as the app, with local reference files kept local.
- Existing songs and cached studies still open correctly, and the normal formatting, lint, and test gates in `docs/branches.md` pass before merge.

## Product boundary

This feature improves **reference-guided composition in an editable studio**. Spectrogram matching alone does not provide the learned vocals, performances, or finished audio generation of a dedicated music model. Adding such a model would be a separate task with its own quality, compute, licensing, and integration decisions.

## Status (2026-10-29)

Built, behind Song Study, `pina like` and the `reference_study`, `reference_create` and
`reference_compare` agent tools:

- **Blueprint** (`crates/engine/src/study/reference/`): versioned; 24 log bands at 20 frames a
  second, starts low/mid/high (and, from audio, a bar's sixteen steps), pitch classes and held
  lines, sections with energy, tempo, meter, bars and key where known; every finding measured,
  estimated or unknown, and from what. Kept beside the studies (`blueprints/`); projects keep a
  `like` link (file, reading, style, rhythm/form/sound closeness).
- **Image study**: plot found inside margins and frame, colour scale matched (gray either way,
  viridis, magma, inferno, plasma, jet); axes given by the person (Song Study's reading, drag to
  crop) or by an agent reading the labels. Missing axes produce `needs` and `unknown`, not
  numbers. Pictures and audio read alike (a song against a drawn spectrogram of itself agrees).
- **Song creation**: sets up tempo, key, meter and the link only; the agent writes every note.
  Pina writes no draft of its own.
- **Comparison and revision**: render vs reference by section (one to one, else by time or
  place): groove, notes and held lines, balance, energy, all relative. Differences name the parts
  and tracks; a request in words is aimed, the agent revises, Pina compares again and stops when
  reached, stalled, or after three passes. Each pass is an undoable agent turn.

## Evaluation (2026-10-29)

Five real references, read-only from the person's disk: an amapiano/house track (127 BPM, D minor,
MP3), two pop tracks from one album (120 and 122 BPM, F# major and Bb minor, WAV), a drums-only
clip and a piano-only clip (74 BPM). Re-run with
`PINA_EVAL_SONGS="a.wav|b.mp3" cargo test -p pina-engine --lib evaluate_on_real_songs -- --ignored --nocapture`.

**Reference quality.** Each song's spectrogram drawn as a picture (magma, log 40 Hz–16 kHz) and
read back, then compared with the same song's audio, so alike is right. Likeness, % (groove,
notes, balance, energy), and the differences it would ask the agent to fix:

| reading | full songs (3) | drums, piano clips |
|---|---|---|
| PNG 1200×400, axes given | 89–94, 59–69, 95–97, 93–95; 0–1 differences | 97–100, 58–82, 88–96, 100; 0 |
| PNG, axes and colour span given | 89–95, 59–69, 82–90, 96–97; 1–2 | 97–100, 58–82, 37–88, 100; 0–1 |
| JPEG quality 40, axes | 91–95, 60–69, 89–94, 93–95; 0–2 | 97–100, 38–91, 83–95, 100; 0 |
| PNG 300×100, axes | 88–90, 66–76, 88–94, 70–94; 0–6 | 97–100, 75–76, 97–99, 100; 0 |
| PNG, no axes | 89–90, 50–69, –, 93–96; 0 | 89–91, 58–100, –, 94–99; 0 |

**Telling songs apart.** Each song's audio against the others' (different songs, so less alike
is right): balance 1–61% (same-song pictures 82–99%), energy 57–77% between full songs, groove
72–95% (the two tracks from one album 95%, close as they sound). Every full-song pair gives 8
differences; the clips 2–6.

**Fixed because of it.** The audio side was decimated to 22 kHz, deaf to everything over 11 kHz a
picture shows (now read at its own rate); a picture's starts were counted per second although
1200 px over four minutes is 5 px a second (now each against its own song's whole); a picture
with no colour span compresses levels by an unknown factor (balance now fits that one factor
before comparing shapes). Same-song false differences fell from 8 to 0–2.

**What a picture can't back up.** Notes from a picture (its held lines) read 38–100% alike
against the same song, mostly 50–70%, and a picture's tonal centre was wrong on 2 of 5; groove per
section needs about 8 px a second. So a picture reference reports differences in energy and
balance (and groove when it resolves time) only; its notes likeness shows, marked an estimate.
Tempo from a picture is only tried at 16 px a second or more: right on the drum clip (74), wrong
on the piano clip (188 for 74: no beat to find).

**Open.**
- Not run: create → agent composition → compare → guided revision with a live agent. It needs
  Claude Code headless on the person's account, minutes and usage per genre; the tests stand in
  for the agent (the revision loop is judged against hand-made edits). Waiting on the person's
  go-ahead.
- With the colour span given, the piano clip's balance reads 37% against its audio (82–90% for
  full songs): a quiet top range sits near the floors, where a picture's span and audio's floor
  differ. Unexplained beyond that.
- A 300×100 picture of a 16-section song gives energy differences in short sections (about 20 px
  a section).
- Not done: the optional spectrogram-to-audio experiment; reading axis labels from pixels (the
  person or an agent gives them); genres beyond these five references.
