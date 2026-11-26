# Composer models

Compose writes bars of a song with a model trained on music itself: MIDI-GPT and its kind. Such
a model reads the song as a score (every part's bars, each note from its bar's start) and writes
the bars asked for, hearing the rest. Pina Studio ships with no model and downloads none until
you ask. The MIDI-GPT weights are licensed for non-commercial research only, so which model you
use, and on what terms, is yours to decide.

## Using one

1. Accept the model's terms where its makers host it. MIDI-GPT is on Hugging Face
   (huggingface.co/Metacreation/MIDI-GPT), which asks you to sign in.
2. In Settings › Composers, paste an access token (read access is enough), or run
   `hf auth login`. Both keep the token in `~/.cache/huggingface/token` (`HF_HOME` moves it).
3. Download a model there.
4. Pick tracks in the headers (⌘-click for several), loop the bars to write, and open Compose
   (its editor tab, or Compose… in a track's menu). Write.

A model carries on what each track already plays: a bass line stays a bass line, a melody a
melody. For chords, set the part's **Plays** to Chords (`--plays chords`): the track is then heard
as keys and doesn't hear its own line, and the model is asked for three or more notes starting
together. Held chords (`--plays held`) are a pad's: heard as a pad, each chord a half note or
longer. Each model is asked by its own names for these: Yellow has them for the whole part
(`min_polyphony`), Prism and Expressive per bar (`bar_min_polyphony`, set on every bar written). Single notes asks for one at a time; the model takes both as strong hints, not rules.

The first write installs the model family's Python, which takes a few minutes and about 690 MB.
After that, the model loads in seconds and stays up between takes, then stops itself after ten
idle minutes.

From the command line:

```sh
pina composers                          # the models, downloaded or not, and their licences
pina composers token hf_…               # sign in
pina composers get midigpt-yellow-small # download it, and install its Python
pina compose ~/Music/Song --track 1,4 --bars 9-16 --takes 4
```

## How it runs

Everything lives in the Composers folder:

- macOS: `~/Library/Application Support/Pina Studio/Composers`
- Windows: `%APPDATA%\Pina Studio\Composers`
- `PINA_COMPOSERS` moves it.

| Path | What it holds |
| --- | --- |
| `<model id>/model.safetensors` | the checkpoint, checked to the byte against the catalogue |
| `<model id>/info.json` | what the model said it can do, so its controls show before it runs again |
| `<family>/env/` | the family's Python, made by `uv venv --python 3.12` with its package installed by `uv pip install` |
| `<family>/server.json` | the server running: its address, model, process and device |
| `<family>/server.log` | the server's output |
| `python/` | the Python that uv downloaded, kept here and nowhere else on the computer |
| `uv/` | Pina Studio's own uv, used only when the computer has none |

A model is served on `127.0.0.1` at a free port. Pina Studio talks to it over its HTTP API using
the system's curl:

- `GET /health` and `GET /info` for its controls and their levels, and the bars it hears.
- `POST /generate` with `stream: true`, one request a take, so each take's notes arrive as
  they're written. Closing the connection stops a take.

## Adding a model of the same kind

A checkpoint for a family that's already there is one entry in `KNOWN`
(`crates/engine/src/composer/catalog.rs`). It needs:

- A download URL pinned to a commit, and its exact size in bytes.
- Its licence as the host states it, plus that page's link.
- Whether its makers ask people to sign in.
- How many bars it hears at once.

A new family (another model with the same idea: score in, bars out) needs a `Family` with:

- Its package at a pinned version.
- Its server's command.
- A score and request mapping in `score.rs`, if its API differs from MIDI-GPT's.

The sheet's controls come from the model's own `/info` (`attributes`, their levels, and the kind
of track each applies to), so a new model's controls show without code for them.
