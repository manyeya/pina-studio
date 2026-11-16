# Separation models

Pina Studio separates audio into stems (vocals, drums, bass and the rest) with a model you add.
It ships with no model and downloads none: the models that separate best have unclear or
non-commercial licences, so which one you use, and on what terms, is yours to decide.

A model runs in ONNX Runtime, built into Pina Studio on Macs with Apple silicon and on Windows
x64. Intel Macs can't run models.

## Getting a model

Settings › Stem models has a walkthrough, the models that have been run (each with what it
gives, how well, how long a song takes and its licence as its host states it) with a Download
button that fetches it from where its maker keeps it, and your own models, to show or remove.
Add a model you have… takes an ONNX file: one of the tested ones is known by its size and gets
its model.json; another gets a model.json guessed from its shapes, its stems left for you to
name. From the command line, `pina models get` lists them and `pina models get <name>`
downloads one.

## Adding a model by hand

Each model is a folder in the Models folder:

- macOS: `~/Library/Application Support/Pina Studio/Models`
- Windows: `%APPDATA%\Pina Studio\Models`

The folder holds the model's ONNX file (`model.onnx` unless `file` says otherwise) and a
`model.json` saying what it gives and how it reads sound:

```json
{
  "name": "BS-RoFormer SW",
  "stems": ["bass", "drums", "other", "vocals", "guitar", "piano"],
  "input": "spec_split",
  "n_fft": 2048,
  "hop": 512,
  "licence": "what its page says",
  "source": "where you got it"
}
```

`pina models` lists the models Pina Studio found, and why a folder isn't one. `pina models
inspect <file.onnx>` prints a model's inputs and outputs with their shapes, which is what you
need to write its `model.json`.

## model.json

| Field | | |
|---|---|---|
| `name` | required | Shown in the menu. |
| `stems` | required | What it gives, in the order of its outputs. |
| `input` | required | `wave`, `spec` or `spec_split` (see below). |
| `rest` | | Also make a stem of the sound minus all the others, with this name: `"instrumental"` for a vocals model. |
| `rate` | 44100 | The sample rate it was trained at. Audio is converted to it and back. |
| `segment` | from its input | Samples per run. Only needed when its input's length isn't fixed. |
| `overlap` | 2 | Runs over each segment's length. 2 overlaps each run by half; 4 is smoother and twice as slow. |
| `n_fft`, `hop` | | The spectrogram's window and hop. Required for `spec` and `spec_split`. |
| `bins` | all | How many of the spectrogram's bins it reads, from the bottom. MDX-Net reads 3072 of 3841. |
| `gpu` | false | Run on the GPU (Core ML, DirectML). Faster when it works, but some models never finish compiling on Core ML, so it's opt-in. |
| `file` | `model.onnx` | Its ONNX file's name. |
| `licence`, `source` | | Notes for you, shown in `pina models`. Never checked. |

## The three inputs

Pina Studio does the STFT and its inverse itself: a periodic Hann window, centred frames with the
sound reflected out at the ends, unnormalised, as `torch.stft` does by default.

- **`wave`**: samples in, `[1, 2, segment]`; each stem's samples out, `[1, stems, 2, segment]`.
  HTDemucs exports work this way.
- **`spec`**: a spectrogram, left's real and imaginary parts then right's, `[1, 4, bins,
  frames]`; each stem's the same way out. MDX-Net and Mel-Band RoFormer exports work this way.
- **`spec_split`**: the spectrogram's real and imaginary parts as two inputs, `[1, 2, bins,
  frames]` each; two outputs, `[1, stems, 2, bins, frames]`. BS-RoFormer exports work this way.

## Models that have been run

Measured on a 16-second mix of loops (drums, bass, piano and a dry vocal), against each part on
its own, on the CPU of an Apple silicon Mac:

| Model | `model.json` | Result |
|---|---|---|
| Mel-Band RoFormer vocals, Kimberley Jensen's (ONNX export, MIT) | `"input": "spec", "n_fft": 2048, "hop": 441`, stems `vocals`, `"rest": "instrumental"` | vocals 21.2 dB, instrumental 29.2; 24 s |
| BS-RoFormer SW, 6 stems (ONNX export) | `"input": "spec_split", "n_fft": 2048, "hop": 512`, stems `bass, drums, other, vocals, guitar, piano` | drums 20.9 dB, bass 20.6, vocals 20.4, piano 19.3; 44 s |
| HTDemucs FT drums (ONNX export) | `"input": "wave"`, stems `drums, bass, other, vocals` | drums 18.8 dB; 13 s |
| MDX-Net, Kim Vocal 2 | `"input": "spec", "n_fft": 7680, "hop": 1024, "bins": 3072`, stems `vocals`, `"rest": "instrumental"` | vocals 18.0 dB, instrumental 26.0; 8 s |

## Using it

Right-click an audio region and pick Separate Stems…, or press Stems… in the Audio editor. The
sheet lists your models (each with its stems, how well it did when tested, its licence, and how
long this file will take on this Mac, or that it's made already), more to download in place,
the stems to keep and whether to mute the region. Separate runs it in the background: the sheet
shows what it's doing (reading, separating with the time left, putting the stems in) with Stop;
Hide puts it away while it carries on, its progress in the status line, and Separate Stems…
brings it back. Each stem
goes on an audio track of its own, playing as the region did (its start, offset, loops, fades,
gain, timing and warps), all of them in a stack named after the region's track. The region is
muted, and one undo takes it all back. The stems are kept in the song's `Audio Files/Stems`, so
separating the same file with the same model again is instant.

From the command line, `pina separate <audio file> [--model <name>] [--out <folder>]` writes a
WAV per stem.

## Lyrics

Whisper hears the words sung in a recording: OpenAI's model, MIT, run by whisper.cpp on the
GPU (Metal) on Apple silicon and on the CPU on Windows. Settings › Stems & lyrics downloads one
from whisper.cpp's host, as does the Lyrics panel itself when there's none: large-v3 turbo
(574 MB, the best, sung words over music in any of the 99 languages Whisper knows) or small
(190 MB, lighter, misses more). Base isn't offered: on a whole song's vocals it heard nothing
where turbo heard 80 lines. The best one downloaded is used. They're kept in `Whisper`, beside `Models`.

Open an audio region in the Audio editor and press Lyrics. A vocal on its own (a vocals stem,
or what the sample library calls a vocal: an acapella, a vocal loop) is heard as it is. Anything
else, a whole song, has its vocals taken out first by the best separation model with vocals
you have (the vocal specialists first: Mel-Band RoFormer vocals, then Kim Vocal 2, then
BS-RoFormer SW), kept with the stems Separate Stems makes, and heard there; how far that's got
shows in the status line. Each line sung then shows beside the waveform: when it starts, how
many times it's sung (×2 and more: a hook), Hear, and Loop · N (Loop the hooks, at the top,
loops each line sung more than once, one after another), which puts the line on its
track's lyrics track ("Drums Lyrics") as a loop: from the bar line it starts after, its pickup
with it, for 1, 2, 4 or 8 bars, at the playhead's bar or after the loops already there. Drag a
line's words onto the tracks instead to put its loop there: on an audio track at the bar under
the pointer, anywhere else on the lyrics track at that bar. A loop of a line heard in a song's
vocals plays the vocals alone. Each word is drawn on the waveform where it's sung (a hook's in
amber; zoomed out over 40 s, each line as one). What's heard is kept in the song's `Audio
Files/Lyrics`, so a file is heard once (nothing heard isn't kept, so a better model can try).
The panel says what it's doing: waiting for Whisper to download, taking the vocals out (how
far), hearing the words, or that it heard nothing and what to try.

What Whisper makes up over music is left out: bracketed words ("(music)"), notes,
a word four or more times on end ("yo yo yo yo") and stutters ("ca-ca-ca-ca"). Lines are split
at pauses, at commas and full stops, and at capitals (Whisper starts a sung line with one), and
each line's start is moved back to where the voice starts.

From the command line, `pina lyrics <audio file> [--model base|small|turbo]` prints the lines
(of the file as it is).

## The Studio Agent

The Studio Agent has the same: `stem_models` (what's downloaded, what could be), `stems` (a
region separated, its stems on tracks in a stack), and `lyrics` (a region's lines, its vocals
taken out first if it's a song), then the `lyric_loop` edit to make a line a loop on the bars,
playing its vocals stem; and `add_stems` for stems it already has. Ask it "loop the hook of the
vocal on bar 33" and it hears the lines, finds the one sung most and puts it there.

Measured on vocal loops from an Afro house pack, with large-v3 turbo: on the same chorus mixed
over drums and bass, its vocals taken out by Mel-Band RoFormer first, the four lines word for
word; a chorus and a verse in
English heard word for word, line by line (8.6 s and 3.0 s for 15 s loops); a hook in Yoruba,
"Bami Dele", heard three times and marked a hook.
