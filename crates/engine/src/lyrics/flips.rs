//! Flips: what a producer makes of a vocal. Words from anywhere in it, cut out and put in a new
//! order on the grid so they say something the singer never sang; pitched up and sped up as soul
//! samples are, or down and slowed; stuttered, answered, reversed into the beat. A language model
//! (the Studio Agent's) finds them from the words and how long each is sung (`prompt`), one flip
//! a line of JSON (`read_line`, so each shows as it's written); Pina Studio hears them (`render`)
//! and makes one a Sampler and its notes (`Project::flip_words`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Line, Word};

/// The ways to flip a vocal: an id, its name, and what it means, for the model.
pub const STYLES: &[(&str, &str, &str)] = &[
    ("any", "Any style", ""),
    (
        "chipmunk",
        "Chipmunk soul",
        "the loop pitched up 3 to 7 semitones (sped up with it, as soul samples were), a phrase or \
         two looped so a soulful line becomes the hook",
    ),
    (
        "chop",
        "Chopped hook",
        "words from different lines in a new order, so they say something the singer never sang: \
         short, punchy, on the grid",
    ),
    (
        "stutter",
        "Stutter",
        "a word, or its start, repeated on 16ths or 32nds, building into the phrase",
    ),
    (
        "call",
        "Call & response",
        "a phrase, then its answer a bar later: another phrase, pitched or reversed",
    ),
    (
        "dark",
        "Dark & slowed",
        "pitched down 2 to 5 semitones (slowed with it), sparse, space between the words, a \
         reversed word swelling into the downbeat",
    ),
    (
        "reverse",
        "Reverse swells",
        "reversed words swelling into a phrase, ending right on the beat they lead into",
    ),
];

/// How a model is told to answer.
pub const SYSTEM: &str = "You're a hip-hop producer who flips vocal samples into hooks. You \
answer with JSON lines only, a flip a line, as asked: no other words, no code fences.";

/// The ways to flip a beat or a sample sliced as it is (Smart Slice).
pub const BEAT_STYLES: &[(&str, &str, &str)] = &[
    ("any", "Any style", ""),
    (
        "mpc",
        "MPC flip",
        "the hits played back in a new order, a groove the original never had: kicks and \
         snares where a drummer wouldn't put them, swung, with space",
    ),
    (
        "soul",
        "Chopped soul",
        "the tonal chops (chords, keys, phrases, a voice) cut short and re-sequenced into a new \
         progression, pitched up 2 to 5 semitones",
    ),
    (
        "halftime",
        "Half-time",
        "the snare on beat 3, the kick sparse, heavier, pitched down 1 to 3 semitones",
    ),
    (
        "roll",
        "Rolls",
        "hat and snare rolls on 16ths, 32nds and triplets building into the downbeat, rising in \
         pitch as they roll",
    ),
    (
        "bounce",
        "Bounce",
        "a head-nodding pocket: kicks off the beat, ghost snares, a chop answering on the and",
    ),
    (
        "reverse",
        "Reverse swells",
        "reversed chops and cymbals swelling into the downbeat",
    ),
];

/// A beat's slices as what a model picks from: each its name (a voice in it said), from and to.
pub fn slice_words(slices: &[crate::smart::Slice]) -> Vec<Word> {
    slices
        .iter()
        .map(|s| Word {
            text: match s.voice {
                true => format!("{} (voice)", s.name),
                false => s.name.clone(),
            },
            start: s.start,
            end: s.end,
        })
        .collect()
}

/// What a model is asked to flip a beat or sample sliced as it is: its slices, numbered, by the
/// bar they fall in (from the start of what was sliced), each with how long it lasts in beats;
/// how producers flip a sample; `count` flips, in `style` (an id of `BEAT_STYLES`), to `vibe`,
/// none called `made`.
#[allow(clippy::too_many_arguments)]
pub fn beat_prompt(
    slices: &[crate::smart::Slice],
    tempo: f64,
    beats_per_bar: u32,
    key: Option<&str>,
    style: &str,
    vibe: &str,
    count: usize,
    made: &[String],
) -> String {
    let beat = 60.0 / tempo.max(1.0) as f32;
    let first = slices.first().map_or(0.0, |s| s.start);
    let words = slice_words(slices);
    let mut listed = String::new();
    let mut bar = None;
    for (n, (s, w)) in slices.iter().zip(&words).enumerate().take(MOST_WORDS) {
        let b = ((s.start - first) / beat / beats_per_bar.max(1) as f32) as usize + 1;
        if bar != Some(b) {
            listed += &format!("\nBar {b}:");
            bar = Some(b);
        }
        listed += &format!(" {n} {} ({:.2})", w.text, (s.end - s.start) / beat);
    }
    let style = match BEAT_STYLES.iter().find(|s| s.0 == style && !s.2.is_empty()) {
        Some((_, name, what)) => format!(" All of them {name}: {what}."),
        None => " Each in a different style, from these.".into(),
    };
    let vibe = match vibe.trim() {
        "" => String::new(),
        v => format!(" What the person wants: \"{v}\". Make that the brief."),
    };
    let made = match made.is_empty() {
        true => String::new(),
        false => format!(
            " These are made already, so make different ones: {}.",
            made.join("; ")
        ),
    };
    let ways: String = BEAT_STYLES
        .iter()
        .filter(|s| !s.2.is_empty())
        .map(|(id, name, what)| format!("- {name} (\"{id}\"): {what}\n"))
        .collect();
    format!(
        "You're a hip-hop producer flipping a sample on an MPC: it's chopped at its hits into \
slices, each named for what it sounds like, and you play them back in a new order on the grid \
so it's a new beat.

The song is {tempo:.0} BPM, {beats_per_bar} beats a bar{key}. The slices, numbered, by the bar \
they fall in, each with how long it lasts in beats (\"(voice)\": a voice is in it):{listed}

Make {count} flips.{style}{vibe}{made}

How producers flip a sample:
{ways}
A flip loops over `bars` bars (1, 2 or 4). Each hit plays the slices numbered `from` to `to` \
(slices in a row play as one phrase) at `beat` (0 is the loop's downbeat; steps of 0.25) for \
`beats`, and stops when the next hit starts: one voice at a time, as on an MPC. The flip's \
`pitch` (semitones, -12 to 12) moves all of it by speed: +12 plays twice as fast and lasts half \
as long. A hit's own `pitch` adds to it; `reverse` plays it backwards. A hit lasts its slices' \
beats divided by its speed: keep `beats` at or under that, unless you mean to cut it short.

Make each one a beat a producer would keep: a groove that sits, the strongest hit on the \
downbeat, space and repetition, a loop without a seam. `says` is what plays, in order, in a \
few words (\"kick, kick, snare, chord stab, hat roll\"). Name each like a track. `why` is one \
short sentence a producer would say about it.

Answer with JSON lines only, a flip a line, nothing else (no code fences):
{{\"name\":\"Back Door\",\"style\":\"mpc\",\"says\":\"kick, chord, kick kick, snare\",\"why\":\"…\",\"bars\":2,\"pitch\":0,\"hits\":[{{\"from\":0,\"to\":0,\"beat\":0,\"beats\":0.5}},{{\"from\":5,\"to\":6,\"beat\":1,\"beats\":1}}]}}",
        key = key.map_or(String::new(), |k| format!(" in {k}")),
    )
}

/// The most words a model is shown: a whole song's are a few hundred.
const MOST_WORDS: usize = 700;

/// A chop of the vocal and where it plays in the flip.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hit {
    /// Where it plays from and to in the recording, in seconds.
    pub from: f32,
    pub to: f32,
    /// Where it plays in the loop (0 its downbeat) and for how long, in beats.
    pub beat: f32,
    pub beats: f32,
    /// Semitones on top of the flip's, by speed as the flip's are.
    #[serde(default)]
    pub pitch: i8,
    #[serde(default)]
    pub reverse: bool,
    /// The words it plays.
    #[serde(default)]
    pub words: String,
}

/// A flip: what it's called, the way it's flipped, what it says now and why it works, how many
/// bars it loops over, how far it's pitched (semitones, by speed: up is faster) and its hits.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Flip {
    pub name: String,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub says: String,
    #[serde(default)]
    pub why: String,
    pub bars: u32,
    #[serde(default)]
    pub pitch: i8,
    pub hits: Vec<Hit>,
}

impl Flip {
    /// Brought inside what it can play, as a flip from a model is: 1 to 8 bars (of
    /// `beats_per_bar`), pitches to an octave each way (two at most together), hits inside the
    /// loop and the recording (`seconds` long), one voice at a time; none if nothing's left.
    pub fn fit(mut self, beats_per_bar: u32, seconds: f32) -> Option<Flip> {
        self.bars = self.bars.clamp(1, 8);
        self.pitch = self.pitch.clamp(-12, 12);
        let length = (self.bars * beats_per_bar.max(1)) as f32;
        self.hits.retain_mut(|h| {
            h.pitch = h.pitch.clamp(-12, 12);
            (h.from, h.to) = (h.from.clamp(0.0, seconds), h.to.clamp(0.0, seconds));
            if h.from > h.to {
                std::mem::swap(&mut h.from, &mut h.to);
            }
            h.beats = h.beats.min(length - h.beat).max(1.0 / 16.0);
            h.beat.is_finite() && (0.0..length).contains(&h.beat) && h.to - h.from > 0.01
        });
        self.hits.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        self.hits.dedup_by(|b, a| (b.beat - a.beat).abs() < 1e-3);
        self.hits.truncate(64);
        (!self.hits.is_empty()).then_some(self)
    }

    /// How fast a hit plays: the flip's pitch and its own, as a tape's speed.
    pub fn speed(&self, h: &Hit) -> f32 {
        2f32.powf((self.pitch as f32 + h.pitch as f32) / 12.0)
    }
}

/// A line as it's heard, whatever its capitals and punctuation: lines that say the same are one.
pub fn line_key(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each line once (where it's first sung), with its words: a line heard before words were kept,
/// one word.
fn once(lines: &[Line]) -> Vec<(&Line, Vec<Word>)> {
    let mut seen = std::collections::HashSet::new();
    let mut count = 0;
    let mut out = Vec::new();
    for l in lines {
        if !seen.insert(line_key(&l.text)) || count >= MOST_WORDS {
            continue;
        }
        let words = match l.words.is_empty() {
            true => vec![Word {
                text: l.text.clone(),
                start: l.start,
                end: l.end,
            }],
            false => l.words.clone(),
        };
        count += words.len();
        out.push((l, words));
    }
    out
}

/// The words a model picks from, by number: each line once, in order.
pub fn numbered(lines: &[Line]) -> Vec<Word> {
    once(lines).into_iter().flat_map(|(_, w)| w).collect()
}

/// What a model is asked: the words, numbered, each with how long it's sung in beats at the
/// song's tempo; how producers flip a vocal; `count` flips, in `style` (an id of `STYLES`) and
/// to `vibe` if the person said one, none called `made` (asked for more).
#[allow(clippy::too_many_arguments)]
pub fn prompt(
    lines: &[Line],
    tempo: f64,
    beats_per_bar: u32,
    key: Option<&str>,
    style: &str,
    vibe: &str,
    count: usize,
    made: &[String],
) -> String {
    let beat = 60.0 / tempo.max(1.0) as f32;
    let mut words = String::new();
    let mut n = 0;
    for (k, (l, ws)) in once(lines).into_iter().enumerate() {
        let hook = match l.sung {
            0 | 1 => String::new(),
            s => format!(" ×{s}"),
        };
        words += &format!("L{}{hook} \"{}\":", k + 1, l.text.trim());
        for w in ws {
            words += &format!(" {n} {} ({:.2})", w.text.trim(), (w.end - w.start) / beat);
            n += 1;
        }
        words.push('\n');
    }
    let style = match STYLES.iter().find(|s| s.0 == style && !s.2.is_empty()) {
        Some((_, name, what)) => format!(" All of them {name}: {what}."),
        None => " Each in a different style, from these.".into(),
    };
    let vibe = match vibe.trim() {
        "" => String::new(),
        v => format!(" What the person wants: \"{v}\". Make that the brief."),
    };
    let made = match made.is_empty() {
        true => String::new(),
        false => format!(
            " These are made already, so make different ones: {}.",
            made.join("; ")
        ),
    };
    let ways: String = STYLES
        .iter()
        .filter(|s| !s.2.is_empty())
        .map(|(id, name, what)| format!("- {name} (\"{id}\"): {what}\n"))
        .collect();
    format!(
        "You're a hip-hop producer flipping a vocal sample: finding the words in it that, cut \
out and put in a new order on the grid, make a hook that says something new.

The song is {tempo:.0} BPM, {beats_per_bar} beats a bar{key}. The vocal's lines, each once \
(×N: sung N times, a hook), then each word's number and how long it's sung, in beats:
{words}
Make {count} flips.{style}{vibe}{made}

How producers flip a vocal:
{ways}
A flip loops over `bars` bars (1, 2 or 4). Each hit plays the words numbered `from` to `to` \
(one line's words in a row, as sung) at `beat` (0 is the loop's downbeat; quarters, eighths \
or sixteenths: steps of 0.25) for `beats`, and stops when the next hit starts: one voice at a \
time, as on an MPC. The flip's `pitch` (semitones, -12 to 12) moves all of it by speed: +12 \
plays twice as fast and lasts half as long, -12 the other way. A hit's own `pitch` adds to it; \
`reverse` plays it backwards. A hit lasts its words' beats divided by its speed: keep `beats` \
at or under that, unless you mean to cut it short.

Make each one a hook a producer would keep: the main phrase lands on the downbeat (or a pickup \
ending on it); rhythm with space and repetition; it loops without a seam; and what it says now \
(`says`, the words in the order they play) makes sense and hits. Name each like a track. `why` \
is one short sentence a producer would say about it.

Answer with JSON lines only, a flip a line, nothing else (no code fences):
{{\"name\":\"Let It Burn\",\"style\":\"chop\",\"says\":\"let it go, let it go, burn\",\"why\":\"…\",\"bars\":2,\"pitch\":0,\"hits\":[{{\"from\":12,\"to\":14,\"beat\":0,\"beats\":1.5}},{{\"from\":3,\"to\":3,\"beat\":3.5,\"beats\":0.5,\"pitch\":-12,\"reverse\":true}}]}}",
        key = key.map_or(String::new(), |k| format!(" in {k}")),
    )
}

/// A flip from a line of a model's answer, its hits by word number (`words`, from `numbered`)
/// taken to seconds; none if the line isn't one. What a model says out of bounds is brought in:
/// bars to 1, 2 or 4, pitch to an octave, hits inside the loop and one voice at a time.
pub fn read_line(line: &str, words: &[Word], beats_per_bar: u32) -> Option<Flip> {
    let (a, b) = (line.find('{')?, line.rfind('}')?);
    let v: Value = serde_json::from_str(line.get(a..=b)?).ok()?;
    from_value(&v, words, beats_per_bar)
}

/// Every flip in a model's whole answer: a line each, or all as one JSON (`{"flips": […]}`, or
/// a list).
pub fn read(text: &str, words: &[Word], beats_per_bar: u32) -> Vec<Flip> {
    let lines: Vec<Flip> = text
        .lines()
        .filter_map(|l| read_line(l, words, beats_per_bar))
        .collect();
    if !lines.is_empty() {
        return lines;
    }
    let start = text.find(['{', '[']).unwrap_or(0);
    let Ok(v) = serde_json::from_str::<Value>(text.get(start..).unwrap_or_default().trim()) else {
        return Vec::new();
    };
    let all = match &v {
        Value::Array(a) => a.clone(),
        v => v["flips"].as_array().cloned().unwrap_or_default(),
    };
    all.iter()
        .filter_map(|f| from_value(f, words, beats_per_bar))
        .collect()
}

/// A model's answer as it streams in: each flip as soon as its line ends.
#[derive(Default)]
pub struct Reading {
    pub text: String,
    read_to: usize,
    found: usize,
}

impl Reading {
    /// `piece` of the answer, and the flips whose lines it ended.
    pub fn add(&mut self, piece: &str, words: &[Word], beats_per_bar: u32) -> Vec<Flip> {
        self.text += piece;
        // The last line ended since the last piece: none, nothing new to read.
        let Some(end) = self.text[self.read_to..]
            .rfind('\n')
            .map(|e| e + self.read_to)
        else {
            return Vec::new();
        };
        let lines = &self.text[self.read_to..end];
        let found: Vec<Flip> = lines
            .lines()
            .filter_map(|l| read_line(l, words, beats_per_bar))
            .collect();
        self.read_to = end + 1;
        self.found += found.len();
        found
    }

    /// The answer's end: its last line's flip; or, if it found none a line, every flip in it
    /// as one JSON.
    pub fn end(&mut self, words: &[Word], beats_per_bar: u32) -> Vec<Flip> {
        let last = read_line(&self.text[self.read_to..], words, beats_per_bar);
        self.read_to = self.text.len();
        let found: Vec<Flip> = match (last, self.found) {
            (Some(f), _) => vec![f],
            (None, 0) => read(&self.text, words, beats_per_bar),
            (None, _) => Vec::new(),
        };
        self.found += found.len();
        found
    }
}

fn from_value(v: &Value, words: &[Word], beats_per_bar: u32) -> Option<Flip> {
    let s = |k: &str| v[k].as_str().unwrap_or_default().trim().to_string();
    let num = |v: &Value| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok());
    let bars = match num(&v["bars"]).unwrap_or(2.0).round() as i64 {
        ..=1 => 1,
        2 | 3 => 2,
        _ => 4,
    };
    let pitch = num(&v["pitch"]).unwrap_or(0.0).round().clamp(-12.0, 12.0) as i8;
    let length = bars as f32 * beats_per_bar.max(1) as f32;
    let mut hits: Vec<Hit> = v["hits"]
        .as_array()?
        .iter()
        .filter_map(|h| {
            let (mut a, mut b) = (num(&h["from"])? as usize, num(&h["to"])? as usize);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            // One line's worth at most.
            let b = b.min(a + 15);
            let (first, last) = (words.get(a)?, words.get(b.min(words.len() - 1))?);
            let beat = num(&h["beat"])? as f32;
            if !(0.0..length).contains(&beat) {
                return None;
            }
            let beats = (num(&h["beats"]).unwrap_or(1.0) as f32).clamp(1.0 / 16.0, length - beat);
            Some(Hit {
                from: first.start,
                to: last.end.max(first.start + 0.02),
                beat,
                beats,
                pitch: num(&h["pitch"]).unwrap_or(0.0).round().clamp(-12.0, 12.0) as i8,
                reverse: h["reverse"].as_bool().unwrap_or(false),
                words: words[a..=b.min(words.len() - 1)]
                    .iter()
                    .map(|w| w.text.trim())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
        })
        .take(64)
        .collect();
    hits.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    // One voice: two on one beat, the first.
    hits.dedup_by(|b, a| (b.beat - a.beat).abs() < 1e-3);
    if hits.is_empty() {
        return None;
    }
    let name = match s("name") {
        n if n.is_empty() => hits[0].words.clone(),
        n => n,
    };
    Some(Flip {
        name,
        style: s("style"),
        says: s("says"),
        why: s("why"),
        bars: bars as u32,
        pitch,
        hits,
    })
}

/// The flip as it sounds, from the recording (`sides` at `rate`), at `tempo` with
/// `beats_per_bar`: `passes` times round, at the recording's rate. Each hit sped by its pitch
/// (or slowed), backwards if it's reversed, and cut off by the next as the Sampler does.
pub fn render(
    flip: &Flip,
    sides: &[Vec<f32>],
    rate: f64,
    tempo: f64,
    beats_per_bar: u32,
    passes: usize,
) -> Vec<Vec<f32>> {
    let beat = 60.0 / tempo.max(1.0) * rate;
    let length = ((flip.bars * beats_per_bar) as f64 * beat).round() as usize;
    let mut out = vec![vec![0.0f32; length * passes.max(1)]; sides.len().max(1)];
    let (fade_in, fade_out) = ((0.003 * rate) as usize, (0.008 * rate) as usize);
    for (k, h) in flip.hits.iter().enumerate() {
        let at = (h.beat as f64 * beat).round() as usize;
        let until = flip
            .hits
            .get(k + 1)
            .map_or(length, |n| (n.beat as f64 * beat).round() as usize)
            .min(length);
        let wants = ((h.beats as f64 * beat).round() as usize).min(until.saturating_sub(at));
        let (a, b) = (
            (h.from as f64 * rate) as usize,
            (h.to as f64 * rate) as usize,
        );
        let speed = flip.speed(h) as f64;
        let long = ((b.saturating_sub(a)) as f64 / speed) as usize;
        let n = wants.min(long);
        for (side, o) in sides.iter().zip(out.iter_mut()) {
            let read = |i: f64| {
                let j = i as usize;
                let f = (i - j as f64) as f32;
                let s = |j: usize| side.get(j).copied().unwrap_or(0.0);
                s(j) * (1.0 - f) + s(j + 1) * f
            };
            for i in 0..n {
                let pos = i as f64 * speed;
                let x = match h.reverse {
                    true => read((b as f64 - 1.0 - pos).max(a as f64)),
                    false => read(a as f64 + pos),
                };
                let gain = (i as f32 / fade_in.max(1) as f32)
                    .min((n - i) as f32 / fade_out.max(1) as f32)
                    .min(1.0);
                for pass in 0..passes.max(1) {
                    o[pass * length + at + i] += x * gain;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f32, end: f32) -> Word {
        Word {
            text: text.into(),
            start,
            end,
        }
    }

    fn lines() -> Vec<Line> {
        let line = |text: &str, words: Vec<Word>, sung| Line {
            text: text.into(),
            start: words[0].start,
            end: words.last().unwrap().end,
            sung,
            words,
        };
        vec![
            line(
                "We're letting go",
                vec![
                    word("We're", 1.0, 1.3),
                    word("letting", 1.3, 1.8),
                    word("go", 1.8, 2.6),
                ],
                2,
            ),
            line(
                "Hand to the sky",
                vec![
                    word("Hand", 3.0, 3.4),
                    word("to", 3.4, 3.5),
                    word("the", 3.5, 3.6),
                    word("sky", 3.6, 4.4),
                ],
                1,
            ),
            // Sung again: shown once, its words not numbered twice.
            line(
                "we're letting go!",
                vec![
                    word("we're", 5.0, 5.3),
                    word("letting", 5.3, 5.8),
                    word("go", 5.8, 6.6),
                ],
                2,
            ),
        ]
    }

    #[test]
    fn the_model_is_shown_each_line_once_its_words_numbered_with_their_beats() {
        let l = lines();
        let words = numbered(&l);
        assert_eq!(words.len(), 7, "the hook sung again isn't numbered twice");
        let p = prompt(
            &l,
            120.0,
            4,
            Some("A minor"),
            "chipmunk",
            "for a soul beat",
            6,
            &[],
        );
        assert!(
            p.contains("L1 ×2 \"We're letting go\": 0 We're (0.60) 1 letting (1.00) 2 go (1.60)")
        );
        assert!(p.contains("L2 \"Hand to the sky\": 3 Hand (0.80)"));
        assert!(!p.contains("L3"));
        assert!(p.contains("120 BPM, 4 beats a bar in A minor"));
        assert!(p.contains("All of them Chipmunk soul"));
        assert!(p.contains("\"for a soul beat\""));
        let more = prompt(&l, 120.0, 4, None, "any", "", 4, &["Let It Burn".into()]);
        assert!(more.contains("Each in a different style"));
        assert!(more.contains("make different ones: Let It Burn"));
    }

    #[test]
    fn a_models_line_is_a_flip_in_seconds_brought_inside_the_loop() {
        let words = numbered(&lines());
        let said = r#"Sure! {"name":"Sky Go","style":"chop","says":"sky, letting go","why":"The sky lands on the one.","bars":3,"pitch":5,"hits":[{"from":6,"to":6,"beat":0,"beats":1},{"from":1,"to":2,"beat":1.5,"beats":40,"reverse":true},{"from":4,"to":3,"beat":1.5,"beats":1},{"from":99,"to":99,"beat":2,"beats":1},{"from":0,"to":0,"beat":9,"beats":1}]}"#;
        let f = read_line(said, &words, 4).unwrap();
        assert_eq!((f.name.as_str(), f.bars, f.pitch), ("Sky Go", 2, 5));
        assert_eq!(f.says, "sky, letting go");
        // Word 6 is "sky": seconds 3.6 to 4.4.
        assert_eq!((f.hits[0].from, f.hits[0].to), (3.6, 4.4));
        assert_eq!(f.hits[0].words, "sky");
        // Two at 1.5: the first kept; its length kept inside the 8-beat loop.
        assert_eq!(f.hits.len(), 2, "{:?}", f.hits);
        assert_eq!((f.hits[1].from, f.hits[1].to), (1.3, 2.6));
        assert_eq!(f.hits[1].beats, 6.5);
        assert!(f.hits[1].reverse);
        assert_eq!(f.hits[1].words, "letting go");
        assert!(read_line("Here are some flips:", &words, 4).is_none());
        // All at once, as one JSON, also reads.
        let all = format!("{{\"flips\":[{}]}}", &said[6..]);
        assert_eq!(read(&all, &words, 4).len(), 1);
        // Streamed: a flip as its line ends, the last at the end.
        let mut r = Reading::default();
        let (a, b) = said.split_at(40);
        assert!(r.add(a, &words, 4).is_empty());
        assert_eq!(r.add(&format!("{b}\n{}", &said[..30]), &words, 4).len(), 1);
        assert!(r.add(&said[30..], &words, 4).is_empty());
        assert_eq!(r.end(&words, 4).len(), 1);
        // All as one JSON at the end, read then.
        let mut r = Reading::default();
        assert!(
            r.add(&format!("```json\n{}", &all[..50]), &words, 4)
                .is_empty()
        );
        r.add(&all[50..], &words, 4);
        assert_eq!(r.end(&words, 4).len(), 1);
    }

    #[test]
    fn a_beats_slices_are_shown_by_bar_its_voiced_ones_said_and_read_back_in_seconds() {
        use crate::{samples::Category, smart::Slice};
        let slice = |start: f32, name: &str, voice| Slice {
            start,
            end: start + 0.25,
            name: name.into(),
            category: Category::Kick,
            confidence: 0.9,
            voice,
        };
        // At 120 bpm a bar is 2 s: from 10 s, three slices in bar 1, one in bar 2.
        let s = vec![
            slice(10.0, "Kick", false),
            slice(10.5, "Snare", false),
            slice(11.0, "Chord Am", true),
            slice(12.0, "Kick", false),
        ];
        let p = beat_prompt(&s, 120.0, 4, None, "mpc", "", 6, &[]);
        assert!(
            p.contains("Bar 1: 0 Kick (0.50) 1 Snare (0.50) 2 Chord Am (voice) (0.50)"),
            "{p}"
        );
        assert!(p.contains("Bar 2: 3 Kick (0.50)"));
        assert!(p.contains("All of them MPC flip"));
        let words = slice_words(&s);
        let f = read_line(
            r#"{"name":"Back Door","bars":1,"hits":[{"from":2,"to":3,"beat":0,"beats":1}]}"#,
            &words,
            4,
        )
        .unwrap();
        assert_eq!((f.hits[0].from, f.hits[0].to), (11.0, 12.25));
        assert_eq!(f.hits[0].words, "Chord Am (voice) Kick");
    }

    #[test]
    fn a_flip_sounds_its_hits_in_place_sped_by_their_pitch_and_cut_by_the_next() {
        // A second of a rising ramp at 1 kHz: a sample's value says where in it it was read.
        let rate = 1000.0;
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let hit = |from: f32, to: f32, beat: f32, beats: f32, pitch: i8, reverse: bool| Hit {
            from,
            to,
            beat,
            beats,
            pitch,
            reverse,
            words: String::new(),
        };
        let flip = Flip {
            name: "Test".into(),
            style: String::new(),
            says: String::new(),
            why: String::new(),
            bars: 1,
            pitch: 0,
            hits: vec![
                hit(0.2, 0.6, 0.0, 1.0, 12, false),
                hit(0.2, 0.6, 1.0, 4.0, 0, true),
                hit(0.0, 0.5, 2.0, 1.0, 0, false),
            ],
        };
        // 60 BPM: a beat a second, 1000 samples; a 4-beat bar, twice round.
        let out = render(&flip, &[ramp], rate, 60.0, 4, 2);
        assert_eq!(out[0].len(), 8000);
        // An octave up: twice as fast, so 0.4 s of it lasts 200 samples.
        assert!((out[0][100] - 0.4).abs() < 0.01, "{}", out[0][100]);
        assert_eq!(out[0][300], 0.0, "it's run out");
        // Backwards: from 0.6 down; cut by the next hit at beat 2, not playing on to 5.
        assert!((out[0][1100] - 0.499).abs() < 0.01, "{}", out[0][1100]);
        assert!((out[0][2100] - 0.1).abs() < 0.01, "the next hit, from 0.0");
        assert_eq!(out[0][3100], 0.0);
        // The second time round, the same.
        assert_eq!(out[0][4100], out[0][100]);
    }
}
