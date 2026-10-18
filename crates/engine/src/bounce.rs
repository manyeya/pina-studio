//! Bouncing a song to files, as Logic's Bounce and FL Studio's Export do: the mix, each track as a
//! stem, or each marked section; over the song, the cycle or any bars; with the tail its reverbs
//! and delays leave (found, up to ten seconds, then trimmed where it falls silent); at any sample
//! rate (rendered at it, not converted after); levelled to a peak or to a loudness (LUFS, under a
//! peak ceiling); dithered (TPDF) when the bits get fewer; written as WAV, AIFF, FLAC, MP3, AAC or
//! ALAC, several at once, and the notes as a MIDI file.
//!
//! WAV is written here and MP3 by LAME (built in); AIFF, FLAC, AAC and ALAC by macOS's own
//! encoder, afconvert, from a WAV made for it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    analysis::{self, Analysis},
    engine::Engine,
    project::{Content, Project},
};

/// How long a tail can ring past the song's end, and how quiet counts as silent.
const TAIL_MAX: f64 = 10.0;
const SILENT: f32 = 1e-4;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Bits {
    I16,
    I24,
    F32,
}

impl Bits {
    pub fn name(self) -> &'static str {
        match self {
            Bits::I16 => "16-bit",
            Bits::I24 => "24-bit",
            Bits::F32 => "32-bit float",
        }
    }
}

/// A file format a bounce is written in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum Format {
    Wav,
    Aiff,
    Flac,
    Mp3 { kbps: u32 },
    Aac { kbps: u32 },
    Alac,
    Midi,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Wav => "WAV",
            Format::Aiff => "AIFF",
            Format::Flac => "FLAC",
            Format::Mp3 { .. } => "MP3",
            Format::Aac { .. } => "AAC",
            Format::Alac => "ALAC",
            Format::Midi => "MIDI",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Aiff => "aif",
            Format::Flac => "flac",
            Format::Mp3 { .. } => "mp3",
            Format::Aac { .. } | Format::Alac => "m4a",
            Format::Midi => "mid",
        }
    }

    /// Roughly how many bytes a second of stereo it takes.
    pub fn bytes_a_second(self, bits: Bits, rate: u32) -> f64 {
        let pcm = rate as f64
            * 2.0
            * match bits {
                Bits::I16 => 2.0,
                Bits::I24 => 3.0,
                Bits::F32 => 4.0,
            };
        match self {
            Format::Wav | Format::Aiff => pcm,
            Format::Flac | Format::Alac => pcm * 0.6,
            Format::Mp3 { kbps } | Format::Aac { kbps } => kbps as f64 * 125.0,
            Format::Midi => 0.0,
        }
    }
}

/// How a bounce's level is set.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Level {
    /// As it plays.
    Off,
    /// Its loudest peak brought to `db` (dBFS).
    Peak { db: f32 },
    /// Its integrated loudness brought to `lufs`, its peaks kept under `ceiling` (dBFS): never
    /// louder than that allows.
    Loudness { lufs: f32, ceiling: f32 },
}

/// How files are written.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Options {
    pub formats: Vec<Format>,
    pub bits: Bits,
    pub rate: u32,
    pub level: Level,
    /// The song's tail (reverb, delay, a long release) past its end, until it's silent.
    pub tail: bool,
    /// TPDF dither, writing fewer bits than the mix has.
    pub dither: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            formats: vec![Format::Wav],
            bits: Bits::I24,
            rate: 48_000,
            level: Level::Off,
            tail: true,
            dither: true,
        }
    }
}

/// One file's worth: what plays (the mix, or one track), over which ticks, and through the
/// master's processing or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub name: String,
    /// A track, alone (a stem); or the whole mix.
    pub track: Option<usize>,
    /// Ticks; the whole song without.
    pub range: Option<(u64, u64)>,
    /// Through the master's plugins, EQ, compressor and fader (a stem usually isn't).
    pub master: bool,
}

/// A bounce done: its files, and how it measured once levelled.
#[derive(Clone, Debug)]
pub struct Done {
    pub name: String,
    pub files: Vec<PathBuf>,
    pub seconds: f64,
    pub analysis: Analysis,
    /// The gain levelling applied, dB.
    pub gain_db: f32,
}

/// The project as a job plays it: one track alone (the others muted, nothing soloed), the
/// master's processing taken out if it's not wanted.
pub fn arranged(p: &Project, job: &Job) -> Project {
    let mut p = p.clone();
    if let Some(t) = job.track {
        for (i, track) in p.tracks.iter_mut().enumerate() {
            track.solo = false;
            track.mute = i != t;
        }
    }
    if !job.master {
        p.master_inserts.clear();
        p.master_eq = None;
        p.master_comp = None;
        p.master_sat = None;
        p.master_volume_db = 0.0;
    }
    p
}

/// A job rendered at `rate`: from its start (with a bar's pre-roll, so notes and effects are
/// going), to its end, and with `tail` on until what rings past it falls silent (ten seconds at
/// most). `progress` hears 0 to 1 as it goes.
pub fn render(
    p: &Project,
    dir: &Path,
    job: &Job,
    rate: u32,
    tail: bool,
    progress: &dyn Fn(f32),
) -> Result<[Vec<f32>; 2], String> {
    let p = arranged(p, job);
    let r = rate as f64;
    let (from, to) = job.range.unwrap_or((0, p.end()));
    let lead = p.samples(from.min(p.bar()), r);
    let start = p.samples(from, r).saturating_sub(lead);
    let body = p.samples(to, r).saturating_sub(start);
    let extra = if tail { (TAIL_MAX * r) as u64 } else { 0 };
    let (mut engine, _plugins) = Engine::load(&p, dir, r)?;
    let total = body + extra;
    let mut out = [
        Vec::with_capacity(total as usize),
        Vec::with_capacity(total as usize),
    ];
    // In pieces, so progress can be told.
    let piece = (r * 2.0) as u64;
    let mut done = 0u64;
    engine.start_render(start);
    while done < total {
        let n = piece.min(total - done);
        let [l, rr] = engine.render_on(n);
        out[0].extend(l);
        out[1].extend(rr);
        done += n;
        progress(done as f32 / total as f32);
    }
    engine.stop();
    let [mut l, mut rr] = out;
    l.drain(..lead as usize);
    rr.drain(..lead as usize);
    // The tail: as long as it rings, and a breath after.
    let keep = (body - lead) as usize;
    let end = match tail {
        true => {
            let last = (keep..l.len())
                .rev()
                .find(|&i| l[i].abs() > SILENT || rr[i].abs() > SILENT)
                .map_or(keep, |i| i + 1);
            (last + (0.05 * r) as usize).min(l.len())
        }
        false => keep.min(l.len()),
    };
    l.truncate(end);
    rr.truncate(end);
    // A few milliseconds' fade where it's cut, so it never clicks.
    let fade = ((0.005 * r) as usize).min(l.len());
    for k in 0..fade {
        let g = k as f32 / fade as f32;
        let i = l.len() - 1 - k;
        l[i] *= g;
        rr[i] *= g;
    }
    Ok([l, rr])
}

/// The gain (dB) that levels audio as asked.
pub fn level_gain(l: &[f32], r: &[f32], rate: u32, level: Level) -> f32 {
    let peak = l
        .iter()
        .chain(r)
        .fold(0f32, |m, s| m.max(s.abs()))
        .max(1e-9);
    let peak_db = 20.0 * peak.log10();
    match level {
        Level::Off => 0.0,
        Level::Peak { db } => db - peak_db,
        Level::Loudness { lufs, ceiling } => {
            let now = analysis::analyze(l, r, rate as f64).lufs;
            if now <= -70.0 {
                return 0.0;
            }
            (lufs - now).min(ceiling - peak_db)
        }
    }
}

/// Bounces a job: rendered, levelled and written in every format asked for (MIDI aside: see
/// `midi`), into `out` as `<name>.<ext>`.
pub fn bounce(
    p: &Project,
    dir: &Path,
    job: &Job,
    opts: &Options,
    out: &Path,
    progress: &dyn Fn(f32),
) -> Result<Done, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let audio_formats: Vec<Format> = opts
        .formats
        .iter()
        .copied()
        .filter(|f| *f != Format::Midi)
        .collect();
    let mut files = Vec::new();
    if opts.formats.contains(&Format::Midi) {
        let file = out.join(format!("{}.mid", job.name));
        std::fs::write(&file, midi(&arranged(p, job), job.range))
            .map_err(|e| format!("{}: {e}", file.display()))?;
        files.push(file);
    }
    if audio_formats.is_empty() {
        return Ok(Done {
            name: job.name.clone(),
            files,
            seconds: 0.0,
            analysis: analysis::analyze(&[0.0], &[0.0], opts.rate as f64),
            gain_db: 0.0,
        });
    }
    let [mut l, mut r] = render(p, dir, job, opts.rate, opts.tail, &|f| progress(f * 0.85))?;
    let gain_db = level_gain(&l, &r, opts.rate, opts.level);
    if gain_db != 0.0 {
        let g = 10f32.powf(gain_db / 20.0);
        l.iter_mut().chain(r.iter_mut()).for_each(|s| *s *= g);
    }
    let analysis = analysis::analyze(&l, &r, opts.rate as f64);
    for (k, f) in audio_formats.iter().enumerate() {
        // AAC and ALAC are both .m4a: with both, the ALAC one says so.
        let file = match (
            f,
            audio_formats
                .iter()
                .any(|o| matches!(o, Format::Aac { .. })),
        ) {
            (Format::Alac, true) => out.join(format!("{} (ALAC).m4a", job.name)),
            _ => out.join(format!("{}.{}", job.name, f.extension())),
        };
        write(&l, &r, opts, *f, &file)?;
        files.push(file);
        progress(0.85 + 0.15 * (k + 1) as f32 / audio_formats.len() as f32);
    }
    Ok(Done {
        name: job.name.clone(),
        files,
        seconds: l.len() as f64 / opts.rate as f64,
        analysis,
        gain_db,
    })
}

/// Samples as integers of `bits`, TPDF-dithered when asked.
fn quantize(x: &[f32], bits: u32, dither: bool, seed: &mut u64) -> Vec<i32> {
    let scale = (1i64 << (bits - 1)) as f32 - 1.0;
    let mut rand = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        (*seed >> 40) as f32 / (1u64 << 24) as f32
    };
    x.iter()
        .map(|&s| {
            let d = if dither { rand() - rand() } else { 0.0 };
            (s * scale + d).round().clamp(-scale - 1.0, scale) as i32
        })
        .collect()
}

/// A WAV of the audio at `bits` (32 is float).
fn write_wav(
    l: &[f32],
    r: &[f32],
    rate: u32,
    bits: Bits,
    dither: bool,
    file: &Path,
) -> Result<(), String> {
    let err = |e: hound::Error| format!("{}: {e}", file.display());
    let (bits_per_sample, sample_format) = match bits {
        Bits::I16 => (16, hound::SampleFormat::Int),
        Bits::I24 => (24, hound::SampleFormat::Int),
        Bits::F32 => (32, hound::SampleFormat::Float),
    };
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample,
        sample_format,
    };
    let mut w = hound::WavWriter::create(file, spec).map_err(err)?;
    match bits {
        Bits::F32 => {
            for (a, b) in l.iter().zip(r) {
                w.write_sample(*a).map_err(err)?;
                w.write_sample(*b).map_err(err)?;
            }
        }
        _ => {
            let n = bits_per_sample as u32;
            let mut seed = 0x9e37_79b9_7f4a_7c15;
            let (a, b) = (
                quantize(l, n, dither, &mut seed),
                quantize(r, n, dither, &mut seed),
            );
            for (x, y) in a.into_iter().zip(b) {
                w.write_sample(x).map_err(err)?;
                w.write_sample(y).map_err(err)?;
            }
        }
    }
    w.finalize().map_err(err)
}

/// One file of the audio in one format.
fn write(l: &[f32], r: &[f32], opts: &Options, f: Format, file: &Path) -> Result<(), String> {
    match f {
        Format::Wav => write_wav(l, r, opts.rate, opts.bits, opts.dither, file),
        Format::Mp3 { kbps } => write_mp3(l, r, opts.rate, kbps, file),
        Format::Midi => Ok(()),
        _ => {
            // macOS's encoder, from a WAV: integer for the lossless ones (24-bit for a float
            // mix), float for AAC to encode from.
            let bits = match (f, opts.bits) {
                (Format::Aac { .. }, _) => Bits::F32,
                (Format::Aiff, b) => b,
                (_, Bits::F32) => Bits::I24,
                (_, b) => b,
            };
            let tmp = std::env::temp_dir().join(format!(
                "tempo-bounce-{}-{}.wav",
                std::process::id(),
                file.file_stem()
                    .map_or(String::new(), |s| s.to_string_lossy().into_owned())
            ));
            write_wav(l, r, opts.rate, bits, opts.dither, &tmp)?;
            let rate = opts.rate;
            let mut args: Vec<String> = match f {
                Format::Aiff => vec![
                    "-f".into(),
                    match bits {
                        Bits::F32 => "AIFC".into(),
                        _ => "AIFF".into(),
                    },
                    "-d".into(),
                    match bits {
                        Bits::I16 => "BEI16".into(),
                        Bits::I24 => "BEI24".into(),
                        Bits::F32 => "BEF32".into(),
                    },
                ],
                Format::Flac => vec!["-f".into(), "flac".into(), "-d".into(), "flac".into()],
                // AAC goes up to 48 kHz.
                Format::Aac { kbps } => vec![
                    "-f".into(),
                    "m4af".into(),
                    "-d".into(),
                    format!("aac@{}", rate.min(48_000)),
                    "-b".into(),
                    (kbps * 1000).to_string(),
                    "-q".into(),
                    "127".into(),
                ],
                _ => vec!["-f".into(), "m4af".into(), "-d".into(), "alac".into()],
            };
            args.push(tmp.display().to_string());
            args.push(file.display().to_string());
            let _ = std::fs::remove_file(file);
            let run = std::process::Command::new("/usr/bin/afconvert")
                .args(&args)
                .output();
            let _ = std::fs::remove_file(&tmp);
            match run {
                Ok(o) if o.status.success() && file.is_file() => Ok(()),
                Ok(o) => Err(format!(
                    "{}: {}",
                    file.display(),
                    String::from_utf8_lossy(&o.stderr).trim()
                )),
                Err(e) => Err(format!("afconvert: {e}")),
            }
        }
    }
}

/// An MP3 by LAME at a constant `kbps`; above 48 kHz (MP3's most) taken down to half its rate.
fn write_mp3(l: &[f32], r: &[f32], rate: u32, kbps: u32, file: &Path) -> Result<(), String> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};
    let (l, r, rate) = match rate > 48_000 {
        true => (halve(l), halve(r), rate / 2),
        false => (l.to_vec(), r.to_vec(), rate),
    };
    let e = |what: &str| format!("{}: MP3 {what}", file.display());
    let mut b = Builder::new().ok_or_else(|| e("encoder"))?;
    b.set_num_channels(2).map_err(|_| e("channels"))?;
    b.set_sample_rate(rate).map_err(|_| e("sample rate"))?;
    let bitrate = match kbps {
        k if k >= 320 => Bitrate::Kbps320,
        k if k >= 256 => Bitrate::Kbps256,
        k if k >= 192 => Bitrate::Kbps192,
        k if k >= 160 => Bitrate::Kbps160,
        _ => Bitrate::Kbps128,
    };
    b.set_brate(bitrate).map_err(|_| e("bitrate"))?;
    b.set_quality(Quality::Best).map_err(|_| e("quality"))?;
    let mut enc = b.build().map_err(|_| e("encoder"))?;
    let mut out: Vec<u8> =
        Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(l.len()) + 7200);
    let n = enc
        .encode(
            DualPcm {
                left: &l,
                right: &r,
            },
            out.spare_capacity_mut(),
        )
        .map_err(|_| e("encoding"))?;
    // Safety: the encoder wrote `n` bytes into the space it was given.
    unsafe { out.set_len(out.len() + n) };
    out.reserve(7200);
    let n = enc
        .flush::<FlushNoGap>(out.spare_capacity_mut())
        .map_err(|_| e("encoding"))?;
    unsafe { out.set_len(out.len() + n) };
    std::fs::write(file, out).map_err(|err| format!("{}: {err}", file.display()))
}

/// Half the samples: low-passed below the new Nyquist (a windowed-sinc, 31 taps), every second
/// kept.
fn halve(x: &[f32]) -> Vec<f32> {
    const TAPS: usize = 31;
    let mid = (TAPS / 2) as f32;
    let taps: Vec<f32> = (0..TAPS)
        .map(|k| {
            let t = k as f32 - mid;
            let sinc = if t == 0.0 {
                0.5
            } else {
                (std::f32::consts::PI * t * 0.5).sin() / (std::f32::consts::PI * t)
            };
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * k as f32 / (TAPS - 1) as f32).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    (0..x.len() / 2)
        .map(|i| {
            let c = (i * 2) as isize;
            taps.iter()
                .enumerate()
                .map(|(k, t)| {
                    x.get((c + k as isize - mid as isize).max(0) as usize)
                        .copied()
                        .unwrap_or(0.0)
                        * t
                })
                .sum::<f32>()
                / sum
        })
        .collect()
}

/// The song's notes as a standard MIDI file (type 1): its tempo and meter, then a track for each
/// track that plays notes (muted ones left out), each region's notes as it plays them, looped
/// regions round their loops; from `range`'s start (ticks), if given, to its end.
pub fn midi(p: &Project, range: Option<(u64, u64)>) -> Vec<u8> {
    let (from, to) = range.unwrap_or((0, p.end()));
    let ppq = crate::project::PPQ as u16;
    let vlq = |mut v: u64, out: &mut Vec<u8>| {
        let mut bytes = vec![(v & 0x7f) as u8];
        v >>= 7;
        while v > 0 {
            bytes.push((v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        out.extend(bytes.iter().rev());
    };
    let chunk = |events: Vec<(u64, Vec<u8>)>| -> Vec<u8> {
        let mut events = events;
        events.sort_by_key(|e| e.0);
        let mut body = Vec::new();
        let mut now = 0u64;
        for (at, bytes) in events {
            vlq(at - now, &mut body);
            body.extend(bytes);
            now = at;
        }
        body.extend([0x00, 0xff, 0x2f, 0x00]);
        let mut out = b"MTrk".to_vec();
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
        out
    };
    let name = |s: &str| {
        let mut e = vec![0xff, 0x03];
        let b = s.as_bytes();
        e.push(b.len().min(127) as u8);
        e.extend(&b[..b.len().min(127)]);
        e
    };
    // The conductor: tempo and time signature.
    let micros = (60_000_000.0 / p.tempo).round() as u32;
    let [beats, unit] = p.time_signature;
    let mut conductor = vec![(0u64, {
        let mut e = vec![0xff, 0x51, 0x03];
        e.extend(&micros.to_be_bytes()[1..]);
        e
    })];
    conductor.push((
        0,
        vec![
            0xff,
            0x58,
            0x04,
            beats as u8,
            (unit as f32).log2() as u8,
            24,
            8,
        ],
    ));
    let mut tracks = vec![chunk(conductor)];
    for (i, t) in p.tracks.iter().enumerate() {
        if t.mute || t.instrument.is_none() {
            continue;
        }
        let mut events: Vec<(u64, Vec<u8>)> = vec![(0, name(&t.name))];
        let channel = (i % 16) as u8;
        for (r, region) in t.regions.iter().enumerate() {
            let Some(notes) = region.content.notes(region.length) else {
                continue;
            };
            let until = match region.looped {
                true => p.loop_end((i, r)),
                false => region.start + region.length,
            };
            let mut start = region.start;
            while start < until {
                for n in notes.iter().map(|n| region.played(n)) {
                    let on = start as i64 + n.start;
                    let off = on + n.length as i64;
                    if on < start as i64
                        || on >= (start + region.length).min(until) as i64
                        || on < from as i64
                        || on >= to as i64
                    {
                        continue;
                    }
                    let (on, off) = (
                        (on as u64) - from,
                        (off.min(to as i64) as u64)
                            .saturating_sub(from)
                            .max((on as u64) - from + 1),
                    );
                    events.push((
                        on,
                        vec![0x90 | channel, n.pitch & 127, n.velocity.clamp(1, 127)],
                    ));
                    events.push((off, vec![0x80 | channel, n.pitch & 127, 0]));
                }
                start += region.length.max(1);
            }
        }
        if events.len() > 1 {
            tracks.push(chunk(events));
        }
    }
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((tracks.len() as u16).to_be_bytes());
    out.extend(ppq.to_be_bytes());
    for t in tracks {
        out.extend(t);
    }
    out
}

/// The tracks a stem bounce has something in: those with regions.
pub fn tracks_with_regions(p: &Project) -> Vec<usize> {
    (0..p.tracks.len())
        .filter(|&i| !p.tracks[i].regions.is_empty())
        .collect()
}

/// Each marked section, as (name, from, to) ticks: from its marker to the next, the last to the
/// song's end.
pub fn sections(p: &Project) -> Vec<(String, u64, u64)> {
    let mut marks: Vec<_> = p.markers.iter().filter(|m| m.tick < p.end()).collect();
    marks.sort_by_key(|m| m.tick);
    marks
        .iter()
        .enumerate()
        .map(|(k, m)| {
            (
                m.name.clone(),
                m.tick,
                marks.get(k + 1).map_or(p.end(), |n| n.tick),
            )
        })
        .filter(|s| s.2 > s.1)
        .collect()
}

/// A name a file can have: no slashes or colons.
pub fn file_safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '/' | ':' | '\\') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Whether a region holds notes (for a MIDI-only bounce).
pub fn has_notes(p: &Project) -> bool {
    p.tracks
        .iter()
        .flat_map(|t| &t.regions)
        .any(|r| matches!(r.content, Content::Midi { .. } | Content::Pattern { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Instrument, Marker, Note, PPQ, Region, Track};

    /// Two synth tracks, a bar each: a C on the first, an E on the second.
    fn song() -> Project {
        let mut p = Project::new(48_000);
        for (name, pitch) in [("Keys", 60u8), ("Lead", 64)] {
            p.tracks.push(Track {
                name: name.into(),
                instrument: Some(Instrument::Synth),
                regions: vec![Region {
                    start: 0,
                    length: PPQ * 4,
                    content: Content::Midi {
                        notes: vec![Note {
                            start: 0,
                            length: PPQ * 2,
                            pitch,
                            velocity: 100,
                        }],
                        controls: Vec::new(),
                    },
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        p.markers = vec![
            Marker {
                tick: 0,
                name: "A".into(),
            },
            Marker {
                tick: PPQ * 2,
                name: "B".into(),
            },
        ];
        p.assign_ids();
        p
    }

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tempo-bounce-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_mix_is_rendered_at_its_rate_with_its_tail_and_levelled() {
        let p = song();
        let job = Job {
            name: "Mix".into(),
            track: None,
            range: None,
            master: true,
        };
        let bar = 2.0; // a 4/4 bar at 120 BPM
        let [l, _] = render(&p, &dir(), &job, 44_100, false, &|_| {}).unwrap();
        assert_eq!(l.len(), (bar * 44_100.0) as usize);
        // With the tail: as long as the synth's release rings, no more than ten seconds over.
        let [l, _] = render(&p, &dir(), &job, 44_100, true, &|_| {}).unwrap();
        assert!(
            l.len() >= (bar * 44_100.0) as usize && l.len() < ((bar + 10.0) * 44_100.0) as usize,
            "{}",
            l.len()
        );
        let peak = |x: &[f32]| x.iter().fold(0f32, |m, s| m.max(s.abs()));
        let g = level_gain(&l, &l, 44_100, Level::Peak { db: -1.0 });
        assert!((20.0 * (peak(&l) * 10f32.powf(g / 20.0)).log10() + 1.0).abs() < 0.01);
        // Loudness never pushes the peaks past the ceiling.
        let g = level_gain(
            &l,
            &l,
            44_100,
            Level::Loudness {
                lufs: 0.0,
                ceiling: -1.0,
            },
        );
        assert!(20.0 * (peak(&l) * 10f32.powf(g / 20.0)).log10() <= -0.99);
    }

    #[test]
    fn a_stem_is_its_track_alone_and_sections_split_at_markers() {
        let p = song();
        let one = arranged(
            &p,
            &Job {
                name: "Keys".into(),
                track: Some(0),
                range: None,
                master: false,
            },
        );
        assert!(!one.tracks[0].mute && one.tracks[1].mute);
        assert_eq!(
            sections(&p),
            [
                ("A".to_string(), 0, PPQ * 2),
                ("B".to_string(), PPQ * 2, PPQ * 4)
            ]
        );
        assert_eq!(tracks_with_regions(&p), [0, 1]);
    }

    #[test]
    fn files_are_written_in_every_format_asked() {
        let p = song();
        let out = dir().join("out");
        let opts = Options {
            formats: vec![
                Format::Wav,
                Format::Mp3 { kbps: 192 },
                Format::Flac,
                Format::Aiff,
                Format::Aac { kbps: 256 },
                Format::Midi,
            ],
            bits: Bits::I16,
            rate: 48_000,
            level: Level::Peak { db: -1.0 },
            tail: false,
            dither: true,
        };
        let job = Job {
            name: "Song".into(),
            track: None,
            range: None,
            master: true,
        };
        let done = bounce(&p, &dir(), &job, &opts, &out, &|_| {}).unwrap();
        assert_eq!(done.files.len(), 6);
        for f in &done.files {
            assert!(std::fs::metadata(f).unwrap().len() > 30, "{}", f.display());
        }
        let wav = hound::WavReader::open(out.join("Song.wav")).unwrap();
        assert_eq!(
            (wav.spec().bits_per_sample, wav.spec().sample_rate),
            (16, 48_000)
        );
        assert!((done.analysis.peak_db[0].max(done.analysis.peak_db[1]) + 1.0).abs() < 0.05);
        // The MIDI file: a header, a conductor and the two tracks.
        let mid = std::fs::read(out.join("Song.mid")).unwrap();
        assert_eq!(&mid[..4], b"MThd");
        assert_eq!(u16::from_be_bytes([mid[10], mid[11]]), 3);
    }

    #[test]
    fn halving_keeps_what_is_under_the_new_nyquist() {
        // 1 kHz at 96 kHz, halved: still 1 kHz at 48, as loud.
        let x: Vec<f32> = (0..9600)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 96_000.0).sin())
            .collect();
        let y = halve(&x);
        assert_eq!(y.len(), 4800);
        let peak = y[100..4700].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((peak - 1.0).abs() < 0.02, "{peak}");
    }
}
