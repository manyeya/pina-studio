#!/usr/bin/env python3
"""Builds "Corner Store Blues", a 95 BPM boom-bap beat, with tempo's agent commands alone.

    python3 examples/corner-store-blues/build.py [project folder]

The project goes in ~/Music/tempo/Corner Store Blues by default. It plays real samples from
MusicRadar's free 90s Hip Hop pack (royalty-free to use in music, not to pass on, so they aren't
in this repo: the pack is downloaded on first run) and two free plugins, Surge XT and Dragonfly
Room Reverb, which have to be installed (`tempo plugins` lists them).

The keys loop the beat is built on plays a ninth chord on every beat, C9 Eb9 F9 G9, so
everything else follows that: the bass and 808 take each beat's root, the pad holds the notes
two chords share, and the hook's whistle lead picks notes from each chord. (Note names here are
scientific, middle C being C4; tempo's piano roll calls it C3, as Logic does.)
"""

import json
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

TEMPO = os.environ.get("TEMPO", "tempo")
PACK = "https://cdn.mos.musicradar.com/musicradar-90s-hip-hop-samples.zip"
SAMPLES = Path.home() / "Music/tempo/Samples/MusicRadar 90s Hip Hop"
VST3 = Path.home() / "Library/Audio/Plug-Ins/VST3"

BEAT, BAR, S16 = 960, 3840, 240
TICKS_PER_SECOND = BEAT * 95 / 60


def bar(n, beat=1):
    """Where bar `n`, beat `beat` starts (both from 1)."""
    return (n - 1) * BAR + (beat - 1) * BEAT


def fetch_samples():
    """The pack's '90s Hip Hop' folder, downloaded and unzipped the first time."""
    root = SAMPLES / "90s Hip Hop"
    if root.is_dir():
        return root
    SAMPLES.mkdir(parents=True, exist_ok=True)
    zip_path = SAMPLES / "pack.zip"
    print(f"downloading {PACK}")
    with urllib.request.urlopen(PACK) as r, open(zip_path, "wb") as f:
        shutil.copyfileobj(r, f)
    with zipfile.ZipFile(zip_path) as z:
        for name in z.namelist():
            # Only the pack's own folder, and nothing that climbs out of it.
            if name.startswith("90s Hip Hop/") and ".." not in Path(name).parts:
                z.extract(name, SAMPLES)
    zip_path.unlink()
    return root


def plugin(file, name):
    path = VST3 / file
    if not path.exists():
        sys.exit(f"{name} isn't installed: put {file} in {VST3}")
    return {"path": str(path), "class": name}


class Song:
    def __init__(self, dir):
        self.dir = str(dir)

    def do(self, op, **args):
        args["op"] = op
        out = subprocess.run(
            [TEMPO, "do", self.dir, json.dumps(args)], capture_output=True, text=True
        )
        if out.returncode != 0:
            sys.exit(f"{op} failed: {out.stderr.strip() or out.stdout.strip()}")
        return json.loads(out.stdout)


def notes(*spec):
    """(start, length, pitch, velocity) tuples as tempo notes."""
    return [{"start": s, "length": l, "pitch": p, "velocity": v} for s, l, p, v in spec]


def steps(bars, hits):
    """A pattern row's cells: `hits` maps a step (0-15 in bar 1, 16-31 in bar 2...) to velocity."""
    cells = [0] * (16 * bars)
    for step, v in hits.items():
        cells[step] = v
    return cells


def main():
    project = Path(sys.argv[1] if len(sys.argv) > 1 else Path.home() / "Music/tempo/Corner Store Blues")
    if project.exists():
        sys.exit(f"{project} already exists: move it, or give another folder")
    pack = fetch_samples()
    hits, beat6 = pack / "Drum_Hits", pack / "Beats/Beat_06((95BPM)/Drum_Hits"
    sauce = pack / "Special_Sauce"
    surge = plugin("Surge XT.vst3", "Surge XT")
    room = plugin("DragonflyRoomReverb.vst3", "Dragonfly Room Reverb")

    song = Song(project)
    do = song.do
    do("set_tempo", bpm=95)
    do("set_key", key="C minor")

    # The tracks, in the order they're numbered.
    def sampler(name, zones):
        return do("add_track", name=name, instrument={"sampler": {"zones": zones}})["track"]

    def zone(file, name, note, **more):
        return {"file": str(file), "name": name, "note": note, **more}

    kick = sampler("Kick", [zone(beat6 / "Beat_06_Kick02.wav", "Kick", 36)])
    snare = sampler("Snare", [
        zone(beat6 / "Beat_06_Snare.wav", "Snare", 38),
        zone(hits / "Clap_04.wav", "Clap", 39, gain_db=-3),
        zone(hits / "Rim_02.wav", "Rim", 37, gain_db=-8),
    ])
    hats = sampler("Hats", [
        zone(beat6 / "Beat_06_Hat.wav", "Closed Hat", 42, choke=1),
        zone(hits / "Hat_22.wav", "Open Hat", 46, choke=1, gain_db=-6),
        zone(beat6 / "Beat_06_Shaker.wav", "Shaker", 70, gain_db=-6, pan=0.3),
    ])
    fx = sampler("FX", [
        zone(hits / "Cymbal_03.wav", "Crash", 49, gain_db=-4),
        zone(sauce / "Rev.wav", "Reverse Swell", 50),
        zone(hits / "Rev_01.wav", "Reverse Hit", 51, gain_db=-4),
        zone(sauce / "Rev_bass.wav", "Reverse Bass", 52, gain_db=-6),
    ])
    # Sub_01 sounds at 25.07 Hz: G0 (MIDI 19) and 40 cents, played across the low notes.
    sub = sampler("808", [zone(hits / "Sub_01.wav", "Sub", 12, high=48, root=19,
                               tune_cents=-40, gate=True, release_ms=90)])
    names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
    bass = sampler("Bass", [
        zone(pack / f"Multis/Moog_Bass/Moog_Bass_{n}{octave}.wav", f"{n}{octave}", pitch,
             high=pitch, gate=True, release_ms=70)
        for octave in (1, 2)
        for i, n in enumerate(names)
        for pitch in [12 * (octave + 1) + i]
    ])
    keys = do("add_track", name="Keys")["track"]
    stabs = sampler("Stabs", [
        zone(pack / "Hits/Keys_10.wav", "C Chord", 60),
        zone(pack / "Hits/Stab_07.wav", "Eb Stab", 63, gain_db=-4),
        zone(pack / "Hits/Guitar_01.wav", "Blues Lick", 72, pan=-0.35),
    ])
    pad = do("add_track", name="Pad", instrument={"plugin": surge})["track"]
    lead = do("add_track", name="Whistle", instrument={"plugin": surge})["track"]
    vinyl = do("add_track", name="Vinyl")["track"]

    # Surge XT's init patch, made into a warm pad and a gliding sine whistle, its settings named
    # as Surge shows them (tempo finds the parameter and the value). Its global volume is set
    # too: its default depends on whether its factory patches are installed.
    def surge_set(track, values):
        for param, text in values.items():
            do("set_plugin_param", channel={"track": track}, plugin="instrument",
               param=param, text=text)

    surge_set(pad, {"Global Volume": "0 dB", "A Osc 1 Unison Voices": "3 voices",
                    "A Osc 1 Unison Detune": "16 cents", "A Filter 1 Type": "LP 24 dB",
                    "A Filter 1 Cutoff": "920 Hz", "A Filter 1 Resonance": "12 %",
                    "A Amp EG Attack": "170 ms", "A Amp EG Release": "600 ms",
                    "A Osc Drift": "35 %", "A Volume": "-3 dB"})
    surge_set(lead, {"Global Volume": "0 dB", "A Osc 1 Type": "Sine",
                     "A Play Mode": "Mono (Single Trigger & Fingered Portamento)",
                     "A Portamento": "38 ms", "A Osc Drift": "15 %", "A Amp EG Attack": "28 ms",
                     "A Amp EG Release": "170 ms", "A Volume": "-5.8 dB"})

    # The arrangement: 56 bars.
    INTRO, VERSE1, HOOK1, VERSE2, BREAK, HOOK2, OUTRO, END = 1, 5, 21, 29, 43, 45, 53, 57
    for at, name in [(INTRO, "Intro"), (VERSE1, "Verse 1"), (HOOK1, "Hook"), (VERSE2, "Verse 2"),
                     (BREAK, "Break"), (HOOK2, "Hook"), (OUTRO, "Outro")]:
        do("add_marker", tick=bar(at), name=name)

    # Keys: the two-bar loop, round and round; in the break its first bar stretched to two, half
    # time at the same pitch.
    keys_loop = pack / "Loops/Keys_01(95BPM).wav"
    do("add_audio_region", track=keys, file=str(keys_loop), start=0, length=2 * BAR)
    do("set_region", track=keys, region=0, name="Keys Loop", loop=True)
    do("add_audio_region", track=keys, file=str(keys_loop), start=bar(BREAK), length=BAR)
    do("flex_region", track=keys, region=1, length=2 * BAR)
    do("set_region", track=keys, region=1, name="Half Time")
    do("add_audio_region", track=keys, file=str(keys_loop), start=bar(HOOK2), length=2 * BAR)
    do("set_region", track=keys, region=2, name="Keys Loop", loop=True)

    # Vinyl crackle under everything.
    crackle = sauce / "Crackle_09.wav"
    do("add_audio_region", track=vinyl, file=str(crackle), start=0)
    do("set_region", track=vinyl, region=0, name="Crackle", loop=True)

    # Drums: two-bar step patterns, swung like an MPC, with the second verse and hook aliases of
    # the first (edit one and both change).
    SWING = 58
    def pattern(track, start, bars_long, rows, name, length=None, swing=SWING):
        r = do("add_pattern_region", track=track, start=bar(start),
               length=(length or bars_long) * BAR, step=S16, swing=swing,
               rows=[{"pitch": p, "cells": steps(bars_long, h)} for p, h in rows])["region"]
        do("set_region", track=track, region=r[1], name=name)
        return r[1]

    kick_verse = {0: 115, 7: 92, 10: 108, 16: 115, 18: 80, 23: 92, 26: 104, 31: 64}
    kick_hook = {0: 120, 7: 96, 10: 110, 16: 120, 18: 88, 23: 96, 26: 108, 29: 80, 31: 70}
    snare_verse = {4: 112, 12: 115, 20: 112, 25: 34, 28: 116, 30: 44}
    hats_verse = {0: 100, 2: 64, 4: 92, 6: 60, 8: 98, 10: 66, 12: 90, 14: 62,
                  16: 100, 18: 64, 20: 92, 22: 58, 23: 40, 24: 98, 26: 66, 28: 90, 31: 48}
    shaker = {i: 44 + (i % 4 == 1) * 10 for i in range(1, 32, 2)}

    k1 = pattern(kick, VERSE1, 2, [(36, kick_verse)], "Verse", length=14)
    # The turnaround: the kick drops out under a snare roll into the hook.
    pattern(kick, 19, 2, [(36, {0: 115, 7: 92, 10: 108, 16: 115, 18: 80})], "Turnaround")
    kh = pattern(kick, HOOK1, 2, [(36, kick_hook)], "Hook", length=8)

    s1 = pattern(snare, VERSE1, 2, [(38, snare_verse)], "Verse", length=14)
    pattern(snare, 19, 2, [(38, {4: 112, 12: 115, 20: 112, 25: 34, 26: 60, 27: 70, 28: 84,
                                 29: 96, 30: 108, 31: 118})], "Roll")
    sh = pattern(snare, HOOK1, 2, [(38, snare_verse), (39, {4: 100, 12: 104, 20: 100, 28: 106})],
                 "Hook", length=8)

    hats_intro = {k: v for k, v in hats_verse.items() if k % 2 == 0}
    pattern(hats, 3, 2, [(42, hats_intro)], "Intro")
    h1 = pattern(hats, VERSE1, 2, [(42, {**hats_verse, 30: 0}), (46, {30: 84})], "Verse", length=16)
    hh = pattern(hats, HOOK1, 2, [(42, {**hats_verse, 14: 0, 30: 0}), (46, {14: 80, 30: 90}),
                                  (70, shaker)], "Hook", length=8)

    # Verse 2 and the second hook are aliases; verse 2's hats a touch harder.
    for track, region, start, length in [(kick, k1, VERSE2, 14), (snare, s1, VERSE2, 14),
                                         (hats, h1, VERSE2, 14), (kick, kh, HOOK2, 8),
                                         (snare, sh, HOOK2, 8), (hats, hh, HOOK2, 8)]:
        a = do("add_alias", track=track, region=region, start=bar(start))["region"]
        do("trim_region", track=track, region=a[1], end=bar(start) + length * BAR)
    v2_hats = next(i for i, r in enumerate(show(song)["tracks"][hats]["regions"])
                   if r["start"] == bar(VERSE2))
    do("set_region", track=hats, region=v2_hats, velocity=8)
    # The break keeps time with a shaker alone.
    pattern(hats, BREAK, 2, [(70, shaker)], "Break")

    # FX: crashes into each section, reverse swells landing on the downbeats.
    swell = round(2.95 * TICKS_PER_SECOND)   # Rev.wav peaks 2.95 s in
    rev_hit = round(0.70 * TICKS_PER_SECOND)
    rev_bass = round(1.90 * TICKS_PER_SECOND)
    fx_notes = [(bar(b), 1920, 49, v) for b, v in
                [(VERSE1, 92), (HOOK1, 110), (VERSE2, 100), (HOOK2, 112), (OUTRO, 100)]]
    fx_notes += [(bar(HOOK1) - swell, 1920, 50, 100), (bar(HOOK2) - swell, 1920, 50, 110),
                 (bar(VERSE1) - rev_hit, 960, 51, 90), (bar(OUTRO) - rev_hit, 960, 51, 90),
                 (bar(VERSE2) - rev_bass, 1920, 52, 100)]
    do("add_midi_region", track=fx, start=0, length=bar(END), notes=notes(*sorted(fx_notes)))
    do("set_region", track=fx, region=0, name="Transitions")

    # Bass: each beat's root, C Eb F G, with ghost notes and a walk up into the next bar.
    C2, Eb2, F2, G2, D2, G1, Bb1, B1, F1 = 36, 39, 41, 43, 38, 31, 34, 35, 29
    def bass_bar(at, walk):
        out = [(at, 360, C2, 112), (at + 720, 120, C2, 76), (at + 960, 420, Eb2, 106),
               (at + 1680, 120, Eb2, 70), (at + 1920, 600, F2, 108)]
        if walk:
            out += [(at + 2880, 220, G1, 104), (at + 3360, 200, Bb1, 92), (at + 3600, 200, B1, 100)]
        else:
            out += [(at + 2880, 220, G2, 104), (at + 3360, 400, G1, 98)]
        return out

    def bass_hook_bar(at, walk):
        out = [(at, 240, C2, 116), (at + 480, 120, C2, 80), (at + 720, 200, G1, 92),
               (at + 960, 420, Eb2, 110), (at + 1440, 120, Eb2, 72), (at + 1920, 360, F2, 110),
               (at + 2400, 120, F2, 76), (at + 2640, 200, F1, 92)]
        if walk:
            out += [(at + 2880, 220, G1, 106), (at + 3360, 200, Bb1, 94), (at + 3600, 200, B1, 102)]
        else:
            out += [(at + 2880, 420, G2, 110), (at + 3360, 200, G1, 96), (at + 3600, 200, D2, 90)]
        return out

    v = do("add_midi_region", track=bass, start=bar(VERSE1), length=2 * BAR,
           notes=notes(*bass_bar(0, False), *bass_bar(BAR, True)))["region"][1]
    do("set_region", track=bass, region=v, name="Verse", loop=True)
    # Played straight, then swung to sit with the drums.
    do("edit_notes", track=bass, region=v, edit={"kind": "quantize", "grid": S16, "strength": 1,
                                                  "swing": SWING})
    h = do("add_midi_region", track=bass, start=bar(HOOK1), length=2 * BAR,
           notes=notes(*bass_hook_bar(0, False), *bass_hook_bar(BAR, True)))["region"][1]
    do("set_region", track=bass, region=h, name="Hook", loop=True)
    do("edit_notes", track=bass, region=h, edit={"kind": "quantize", "grid": S16, "strength": 1,
                                                  "swing": SWING})
    a = do("add_alias", track=bass, region=v, start=bar(VERSE2))["region"][1]
    do("set_region", track=bass, region=a, loop=True)
    # The break: one long low C, then nothing.
    b = do("add_midi_region", track=bass, start=bar(BREAK), length=2 * BAR,
           notes=notes((0, 1800, C2, 110)))["region"][1]
    do("set_region", track=bass, region=b, name="Break")
    a = do("add_alias", track=bass, region=h, start=bar(HOOK2))["region"][1]
    do("set_region", track=bass, region=a, loop=True)
    b = do("add_midi_region", track=bass, start=bar(OUTRO), length=BAR,
           notes=notes((0, 600, C2, 112)))["region"][1]
    do("set_region", track=bass, region=b, name="Last Note")

    # 808: under the hook's kicks, each on its beat's root.
    roots = [24, 27, 29, 31]  # C1 Eb1 F1 G1
    hits_808 = sorted(kick_hook)
    sub_notes = []
    for b in range(0, 8, 2):
        for i, step in enumerate(hits_808):
            nxt = hits_808[i + 1] if i + 1 < len(hits_808) else 32
            at = b * BAR + step * S16
            sub_notes.append((at, min(nxt - step, 8) * S16 - 30, roots[(step % 16) // 4],
                              kick_hook[step]))
    s = do("add_midi_region", track=sub, start=bar(HOOK1), length=8 * BAR,
           notes=notes(*sub_notes))["region"][1]
    do("edit_notes", track=sub, region=s, edit={"kind": "quantize", "grid": S16, "strength": 1,
                                                 "swing": SWING})
    do("set_region", track=sub, region=s, name="Hook 808")
    do("add_alias", track=sub, region=s, start=bar(HOOK2))
    b = do("add_midi_region", track=sub, start=bar(OUTRO), length=BAR,
           notes=notes((0, 1800, 24, 112)))["region"][1]
    do("set_region", track=sub, region=b, name="Last Hit")

    # Stabs: the C chord on the one, the Eb stab on the Eb beat, the blues lick every four bars.
    st = do("add_pattern_region", track=stabs, start=bar(VERSE2), length=14 * BAR, step=S16,
            swing=SWING, rows=[
                {"pitch": 60, "cells": steps(4, {0: 100, 16: 86, 32: 100, 48: 86})},
                {"pitch": 63, "cells": steps(4, {20: 82, 52: 90})},
                {"pitch": 72, "cells": steps(4, {0: 92})},
            ])["region"][1]
    do("set_region", track=stabs, region=st, name="Stabs")
    for at, length in [(HOOK1, 8), (HOOK2, 8)]:
        a = do("add_alias", track=stabs, region=st, start=bar(at))["region"][1]
        do("trim_region", track=stabs, region=a, end=bar(at) + length * BAR)
    b = do("add_midi_region", track=stabs, start=bar(OUTRO), length=BAR,
           notes=notes((0, 960, 60, 104), (0, 960, 72, 90)))["region"][1]
    do("set_region", track=stabs, region=b, name="Last Hit")

    # Pad: over C9 and Eb9 the notes they share, G Bb (and G above); over F9 and G9, F A and G.
    # Each step down a tone, as strings would.
    pad_notes = []
    for b in range(8):
        at = b * BAR
        pad_notes += [(at, 1900, p, 72) for p in (67, 70, 79)]
        pad_notes += [(at + 1920, 1900, p, 68) for p in (65, 69, 79)]
    p1 = do("add_midi_region", track=pad, start=bar(HOOK1), length=8 * BAR,
            notes=notes(*pad_notes))["region"][1]
    do("set_region", track=pad, region=p1, name="Hook Pad")
    do("add_alias", track=pad, region=p1, start=bar(HOOK2))
    intro = do("add_midi_region", track=pad, start=0, length=4 * BAR,
               notes=notes(*[n for n in pad_notes if n[0] < 4 * BAR]))["region"][1]
    do("set_region", track=pad, region=intro, name="Intro Pad", velocity=-12)
    outro = do("add_midi_region", track=pad, start=bar(OUTRO), length=4 * BAR,
               notes=notes(*[n for n in pad_notes if n[0] < 4 * BAR]))["region"][1]
    do("set_region", track=pad, region=outro, name="Outro Pad", velocity=-8)

    # The whistle: a four-bar hook, a question and its answer, each note a tone of its beat's
    # chord.
    G5, Bb5, F5, C6, D6, A5, Eb5, C5, D5, E5 = 79, 82, 77, 84, 86, 81, 75, 72, 74, 76
    phrase = [
        (0, 660, G5, 100), (720, 260, Bb5, 90), (960, 440, F5, 96), (1440, 440, G5, 92),
        (1920, 900, C6, 102), (2880, 440, D6, 96), (3360, 480, A5, 88),
        (3840, 1700, G5, 100), (5760, 240, Eb5, 84), (6000, 240, F5, 90), (6240, 460, C5, 88),
        (6720, 900, D5, 92),
        (7680, 480, C6, 102), (8160, 460, Bb5, 92), (8640, 460, G5, 96), (9120, 460, Bb5, 90),
        (9600, 900, A5, 96), (10560, 460, G5, 92), (11040, 460, F5, 88),
        (11520, 900, E5, 96), (12480, 440, G5, 90), (12960, 440, F5, 88), (13440, 1700, C5, 94),
    ]
    whistle = [(s + half, l, p, v) for half in (0, 4 * BAR) for s, l, p, v in phrase]
    w = do("add_midi_region", track=lead, start=bar(HOOK1), length=8 * BAR,
           notes=notes(*whistle))["region"][1]
    do("set_region", track=lead, region=w, name="Whistle Hook")
    do("edit_notes", track=lead, region=w, edit={"kind": "humanize", "timing": 12, "velocity": 6,
                                                  "seed": 7})
    do("add_alias", track=lead, region=w, start=bar(HOOK2))

    # Live Loops: the verse and the hook as scenes, to jam the song's parts in any order.
    def cell(track, scene, name, bars, content):
        do("set_cell", track=track, scene=scene,
           region={"name": name, "start": 0, "length": bars * BAR, **content})

    def pattern_cell(rows, bars=2):
        return {"pattern": {"step": S16, "swing": SWING, "rows": [
            {"pitch": p, "cells": steps(bars, h)} for p, h in rows]}}

    def swung(spec):
        """Notes on the off sixteenths a little late, as quantize with swing leaves the regions'."""
        late = round((SWING / 50 - 1) * S16)
        return [(s + late * (s % (2 * S16) == S16), l, p, v) for s, l, p, v in spec]

    keys_cell = {"audio": {"file": "Audio Files/" + keys_loop.name, "offset": 0, "stretch": 1.0}}
    for scene, name in [(0, "Verse"), (1, "Hook")]:
        cell(keys, scene, "Keys Loop", 2, keys_cell)
    cell(kick, 0, "Verse", 2, pattern_cell([(36, kick_verse)]))
    cell(kick, 1, "Hook", 2, pattern_cell([(36, kick_hook)]))
    cell(snare, 0, "Verse", 2, pattern_cell([(38, snare_verse)]))
    cell(snare, 1, "Hook", 2, pattern_cell([(38, snare_verse),
                                           (39, {4: 100, 12: 104, 20: 100, 28: 106})]))
    cell(hats, 0, "Verse", 2, pattern_cell([(42, {**hats_verse, 30: 0}), (46, {30: 84})]))
    cell(hats, 1, "Hook", 2, pattern_cell([(42, {**hats_verse, 14: 0, 30: 0}),
                                          (46, {14: 80, 30: 90}), (70, shaker)]))
    cell(bass, 0, "Verse", 2, {"midi": {"notes": notes(*swung(bass_bar(0, False) +
                                                              bass_bar(BAR, True)))}})
    cell(bass, 1, "Hook", 2, {"midi": {"notes": notes(*swung(bass_hook_bar(0, False) +
                                                             bass_hook_bar(BAR, True)))}})
    cell(sub, 1, "Hook 808", 2, {"midi": {"notes": notes(*swung(
        [n for n in sub_notes if n[0] < 2 * BAR]))}})
    cell(stabs, 1, "Stabs", 4, pattern_cell([(60, {0: 100, 16: 86, 32: 100, 48: 86}),
                                            (63, {20: 82, 52: 90}), (72, {0: 92})], bars=4))
    cell(pad, 1, "Hook Pad", 2, {"midi": {"notes": notes(*[n for n in pad_notes if n[0] < 2 * BAR])}})
    cell(lead, 1, "Whistle Hook", 4, {"midi": {"notes": notes(*phrase)}})

    # The mix. Drums into a stack that glues them; two reverbs on sends, both Dragonfly Room:
    # a tight room and a big dark one.
    drums = do("add_stack", tracks=[kick, snare, hats], name="Drums")["bus"]
    room_bus = do("add_send", track=snare, level_db=-11)["bus"]
    hall_bus = do("add_send", track=keys, level_db=-15)["bus"]
    auxes = show(song)["auxes"]
    aux_of = {a["bus"]: i for i, a in enumerate(auxes)}
    do("set_aux", aux=aux_of[room_bus], name="Room")
    do("set_aux", aux=aux_of[hall_bus], name="Hall")
    for bus_, values in [
        (room_bus, {2: 0.0, 3: 0.55, 4: 0.25, 5: 0.22, 6: 0.25, 8: 0.04, 9: 0.06, 10: 0.8,
                    13: 0.6, 15: 0.45, 18: 0.5}),
        (hall_bus, {2: 0.0, 3: 0.1, 4: 0.3, 5: 0.6, 6: 0.8, 7: 0.6, 8: 0.2, 9: 0.18, 10: 0.85,
                    13: 0.5, 15: 0.3, 18: 1.0}),
    ]:
        ch = {"aux": aux_of[bus_]}
        do("set_insert", channel=ch, index=0, plugin=room)
        for param, value in values.items():
            do("set_plugin_param", channel=ch, plugin={"insert": 0}, param=param, value=value)
    for track, bus_, level in [(hats, room_bus, -18), (kick, room_bus, -24), (snare, hall_bus, -22),
                               (stabs, hall_bus, -12), (pad, hall_bus, -10), (lead, hall_bus, -9),
                               (fx, hall_bus, -14)]:
        do("add_send", track=track, bus=bus_, level_db=level)

    def eq(channel, **bands):
        do("set_eq", channel=channel, eq=bands)

    def comp(channel, **c):
        do("set_comp", channel=channel, comp=c)

    def sat(channel, **s):
        do("set_saturator", channel=channel, sat=s)

    bell = lambda f, g, q=1.0: {"freq": f, "gain_db": g, "q": q}
    shelf = lambda f, g: {"freq": f, "gain_db": g}
    eq({"track": kick}, low_cut=40, low_mid=bell(320, -3.5, 1.4), high_mid=bell(3200, 2.5))
    comp({"track": kick}, threshold_db=-14, ratio=3, attack_ms=12, release_ms=90, makeup_db=1.5)
    eq({"track": snare}, low_cut=110, low_mid=bell(500, -2), high_shelf=shelf(7000, 3.5))
    comp({"track": snare}, threshold_db=-16, ratio=4, attack_ms=8, release_ms=110, makeup_db=2)
    eq({"track": hats}, low_cut=320, high_mid=bell(5000, -1.5, 0.8))
    eq({"track": fx}, low_cut=80)
    eq({"track": sub}, low_cut=24, high_shelf=shelf(1500, -9))
    sat({"track": sub}, drive_db=8, tone=-0.2, mix=0.35)
    eq({"track": bass}, low_cut=36, low_mid=bell(260, -2.5), high_mid=bell(1100, 2))
    comp({"track": bass}, threshold_db=-20, ratio=4, attack_ms=15, release_ms=100, makeup_db=3)
    sat({"track": bass}, drive_db=6, tone=0.1, mix=0.4)
    # The keys loop has its own bass notes: take them out, the Moog bass plays those.
    eq({"track": keys}, low_cut=150, low_mid=bell(420, -1.5), high_shelf=shelf(8000, 2.5))
    sat({"track": keys}, drive_db=7, tone=0.1, mix=0.45)
    eq({"track": stabs}, low_cut=180)
    eq({"track": pad}, low_cut=220, high_shelf=shelf(7000, -3))
    eq({"track": lead}, low_cut=300, high_mid=bell(2400, 1.5), high_shelf=shelf(9000, -2))
    eq({"track": vinyl}, low_cut=200)
    comp({"aux": aux_of[drums]}, threshold_db=-12, ratio=3, attack_ms=10, release_ms=120,
         makeup_db=1.5, rms=True)
    sat({"aux": aux_of[drums]}, drive_db=5, tone=0.1, mix=0.3)
    eq("master", low_cut=25, high_shelf=shelf(11000, 1))
    # The master: glued, then pushed into the saturator, which as a soft clipper keeps the peaks
    # near -1 dBFS (with this drive and output) and brings the beat up to about -13.5 LUFS.
    comp("master", threshold_db=-9, ratio=2, attack_ms=30, release_ms=200, makeup_db=4, rms=True)
    sat("master", drive_db=4, tone=0, mix=1, output_db=1)

    for track, db, pan in [(kick, -5, 0), (snare, -4, 0), (hats, 5, 0.12), (fx, -8, 0),
                           (sub, -11, 0), (bass, -9, 0), (keys, 2, 0), (stabs, -7, 0),
                           (pad, -14, -0.15), (lead, -12, 0.1), (vinyl, -14, 0)]:
        do("set_track", track=track, volume_db=db, pan=pan)

    # Automation: the keys and pad fade in and out; the vinyl sits up front in the intro and
    # outro; the pad's filter opens through each hook.
    def auto(track, points, param="volume", **more):
        do("set_automation", track=track, param=param,
           points=[{"tick": t, "value": v} for t, v in points], **more)

    auto(keys, [(0, -3), (bar(5), 2), (bar(OUTRO), 2), (bar(END), -36)])
    auto(vinyl, [(0, -8), (bar(5), -14), (bar(OUTRO), -14), (bar(END), -6)])
    auto(pad, [(0, -24), (bar(5), -14), (bar(OUTRO), -14), (bar(END), -40)])
    auto(pad, [(0, 0.48), (bar(HOOK1), 0.5), (bar(HOOK1 + 6), 0.66), (bar(VERSE2), 0.5),
               (bar(HOOK2), 0.5), (bar(HOOK2 + 6), 0.66), (bar(OUTRO), 0.6), (bar(END), 0.42)],
         param="plugin", plugin="instrument", id="A Filter 1 Cutoff")

    # The hook on the cycle, ready to loop (off until C is pressed).
    do("set_cycle", start=bar(HOOK1), end=bar(VERSE2), on=False)
    print(f"built {project}: {len(show(song)['tracks'])} tracks, {END - 1} bars at 95 BPM")


def show(song):
    out = subprocess.run([TEMPO, "show", song.dir], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


if __name__ == "__main__":
    main()
