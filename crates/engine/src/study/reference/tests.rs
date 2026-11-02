//! Blueprints from known sounds (a tone, a sweep, a beat, a song that changes halfway), from
//! drawn spectrograms (margins, colour scales and axes of several kinds, given or not), and
//! songs compared with their references.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};

use super::audio::{Shape, from_audio};
use super::compare::{self, Measure, Verdict};
use super::image::{Picture, find, from_picture, table};
use super::*;

const RATE: f32 = 44_100.0;

fn tone(hz: f32, seconds: f32, amp: f32) -> Vec<f32> {
    (0..(seconds * RATE) as usize)
        .map(|i| amp * (TAU * hz * i as f32 / RATE).sin())
        .collect()
}

/// A beat at `bpm`: a kick (a falling 60 Hz thump) on each beat, hats (bright noise) on the
/// eighths between, for `seconds`.
fn beat(bpm: f32, seconds: f32, hats: bool) -> Vec<f32> {
    let n = (seconds * RATE) as usize;
    let mut x = vec![0f32; n];
    let beat = 60.0 / bpm;
    let mut seed = 1u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0
    };
    let mut t = 0.0;
    while t < seconds {
        let at = (t * RATE) as usize;
        for i in 0..(0.25 * RATE) as usize {
            if at + i < n {
                let s = i as f32 / RATE;
                x[at + i] +=
                    0.8 * (-s * 18.0).exp() * (TAU * (60.0 + 80.0 * (-s * 30.0).exp()) * s).sin();
            }
        }
        if hats {
            let off = ((t + beat / 2.0) * RATE) as usize;
            let mut last = 0.0;
            for i in 0..(0.05 * RATE) as usize {
                if off + i < n {
                    let w = noise();
                    // A crude high pass: the noise less its last value.
                    x[off + i] += 0.3 * (-(i as f32 / RATE) * 60.0).exp() * (w - last);
                    last = w;
                }
            }
        }
        t += beat;
    }
    x
}

fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0))
        .collect()
}

fn blueprint(x: &[f32], shape: Option<&Shape>) -> Blueprint {
    from_audio(
        x,
        x,
        RATE,
        Source::Audio,
        PathBuf::from("test.wav"),
        shape,
        None,
    )
}

fn shape(bpm: f32, seconds: f32, sections: &[(&str, f32, f32)]) -> Shape {
    let bar = 4.0 * 60.0 / bpm;
    Shape {
        bpm,
        meter: [4, 4],
        bars: (0..(seconds / bar) as usize)
            .map(|i| i as f32 * bar)
            .collect(),
        sections: sections
            .iter()
            .enumerate()
            .map(|(i, (n, a, z))| {
                (
                    n.to_string(),
                    ((b'A' + i as u8) as char).to_string(),
                    *a,
                    *z,
                )
            })
            .collect(),
    }
}

#[test]
fn a_held_tone_is_a_line_on_its_pitch_and_its_pitch_class() {
    let bp = blueprint(&tone(440.0, 6.0, 0.5), None);
    assert_eq!(bp.version, VERSION);
    assert_eq!(bp.frames, 120);
    let longest = bp
        .contours
        .iter()
        .max_by_key(|c| c.end - c.start)
        .expect("a held line");
    let hz = longest.hz.unwrap();
    assert!((hz - 440.0).abs() < 8.0, "{hz}");
    assert!(longest.end - longest.start > 80, "{longest:?}");
    let pc = bp.pitch_classes(0, bp.frames).unwrap();
    let top = (0..12).max_by(|&a, &b| pc[a].total_cmp(&pc[b])).unwrap();
    assert_eq!(top, 9, "A: {pc:?}");
    // Its energy is in the band round 440 Hz, the mid range louder than the high.
    let b = bp.balance(0, bp.frames);
    assert!(b[1] > b[2] + 10.0, "{b:?}");
    assert!(
        bp.facts
            .iter()
            .any(|f| f.name == "Length" && f.basis == Basis::Measured)
    );
}

#[test]
fn a_sweep_climbs_the_bands() {
    // 100 Hz to 8 kHz over eight seconds, up a log scale.
    let x: Vec<f32> = {
        let (f0, f1, len) = (100.0f32, 8000.0f32, 8.0f32);
        let k = (f1 / f0).ln() / len;
        (0..(len * RATE) as usize)
            .map(|i| {
                let t = i as f32 / RATE;
                0.5 * (TAU * f0 * ((k * t).exp() - 1.0) / k).sin()
            })
            .collect()
    };
    let bp = blueprint(&x, None);
    let loudest = |f: usize| {
        (0..BANDS)
            .max_by(|&a, &b| bp.energy[f][a].total_cmp(&bp.energy[f][b]))
            .unwrap()
    };
    let (early, late) = (loudest(10), loudest(150));
    assert!(
        bp.bands_hz[early] < 300.0 && bp.bands_hz[late] > 3000.0,
        "{early} {late}"
    );
}

#[test]
fn a_beat_starts_low_on_its_beats_and_high_between() {
    let x = beat(120.0, 12.0, true);
    let sh = shape(120.0, 12.0, &[("Verse", 0.0, 12.0)]);
    let bp = blueprint(&x, Some(&sh));
    let d = bp.density(0, bp.frames);
    assert!((1.6..2.5).contains(&d[0]), "two kicks a second: {d:?}");
    assert!(d[2] > 1.5, "hats between: {d:?}");
    // On the bar's steps: the kick on 1, 5, 9 and 13; the hats on 3, 7, 11 and 15.
    let g = bp.sections[0].grid.expect("its bars are known");
    for s in [0, 4, 8, 12] {
        assert!(g[0][s] > 0.6, "kick on step {}: {:?}", s + 1, g[0]);
    }
    for s in [2, 6, 10, 14] {
        assert!(
            g[2][s] > 0.5 && g[0][s] < 0.4,
            "hat on step {}: {:?}",
            s + 1,
            g[2]
        );
    }
    assert_eq!(bp.sections[0].bars, Some(6));
    assert_eq!(bp.tempo, Some(120.0));
    let brief = bp.brief();
    let low = brief["sections"][0]["steps"]["low"]
        .as_str()
        .unwrap()
        .replace(' ', "");
    for s in [0, 4, 8, 12] {
        assert!(low.as_bytes()[s] >= b'5', "{low}");
    }
}

#[test]
fn where_a_song_changes_is_a_new_section_and_the_louder_plays_harder() {
    let quiet = tone(220.0, 16.0, 0.05);
    let loud = mix(&beat(100.0, 16.0, true), &tone(330.0, 16.0, 0.3));
    let x: Vec<f32> = quiet.iter().chain(&loud).copied().collect();
    let bp = blueprint(&x, None);
    assert!(bp.sections.len() >= 2, "{:?}", bp.sections);
    let edge = bp.sections[1].start as f32 / FPS;
    assert!((edge - 16.0).abs() < 2.5, "{edge}");
    assert!(bp.sections[1].energy > bp.sections[0].energy + 0.3);
    assert_ne!(bp.sections[0].label, bp.sections[1].label);
}

/// A drawn spectrogram: `w` by `h` pixels of plot in `colors`, from `value(x, y)` (0 quiet, 1
/// loud; y 0 at the top), inside a white margin (with a black frame round the plot) if asked.
fn picture(
    w: u32,
    h: u32,
    colors: Colors,
    margin: u32,
    value: impl Fn(u32, u32) -> f32,
) -> Picture {
    let t = table(colors);
    let (pw, ph) = (w + 2 * margin, h + 2 * margin);
    let mut pixels = vec![[255u8; 3]; (pw * ph) as usize];
    for y in 0..h {
        for x in 0..w {
            let v = value(x, y).clamp(0.0, 1.0);
            let c = t[(v * 63.0).round() as usize];
            pixels[((y + margin) * pw + x + margin) as usize] = c.map(|c| c as u8);
        }
    }
    if margin > 0 {
        for x in margin - 1..=margin + w {
            pixels[((margin - 1) * pw + x) as usize] = [0; 3];
            pixels[((margin + h) * pw + x) as usize] = [0; 3];
        }
        for y in margin - 1..=margin + h {
            pixels[(y * pw + margin - 1) as usize] = [0; 3];
            pixels[(y * pw + margin + w) as usize] = [0; 3];
        }
        // Some "labels": dark marks in the margin.
        for x in (margin..margin + w).step_by(50) {
            for d in 0..6 {
                pixels[((margin + h + 4 + d) * pw + x) as usize] = [40; 3];
            }
        }
    }
    Picture {
        width: pw,
        height: ph,
        pixels,
    }
}

/// A song's spectrogram: a held line at 1 kHz on a log axis from 50 Hz to 10 kHz, a kick's
/// stripe low down twice a second, over a quiet floor, `seconds` long across `w` pixels.
fn song_plot(w: u32, h: u32, seconds: f32) -> impl Fn(u32, u32) -> f32 {
    let scale = FreqScale::Log;
    let line = ((1.0 - scale.up(50.0, 10_000.0, 1000.0)) * h as f32) as u32;
    move |x, y| {
        let t = x as f32 / w as f32 * seconds;
        let kick = (t * 2.0).fract() < 0.06 && y > h * 3 / 4;
        let held = y.abs_diff(line) <= 1;
        0.12 + if held { 0.7 } else { 0.0 } + if kick { 0.75 } else { 0.0 }
    }
}

#[test]
fn a_spectrograms_plot_and_colours_are_found_in_its_margins() {
    for colors in [Colors::Viridis, Colors::Magma, Colors::Jet, Colors::Gray] {
        let p = picture(600, 240, colors, 40, song_plot(600, 240, 30.0));
        let f = find(&p);
        let [x, y, w, h] = f.crop;
        assert!(
            x.abs_diff(40) <= 2
                && y.abs_diff(40) <= 2
                && w.abs_diff(600) <= 4
                && h.abs_diff(240) <= 4,
            "{colors:?}: {:?}",
            f.crop
        );
        assert_eq!(f.colors, colors, "{colors:?}");
        assert!(f.colors_sure > 0.7, "{colors:?} {}", f.colors_sure);
    }
    // Gray the other way round (white quiet) reads the right way round; with no margin, it's all
    // plot.
    let p = picture(400, 200, Colors::GrayInverted, 0, song_plot(400, 200, 20.0));
    let f = find(&p);
    assert_eq!(f.colors, Colors::GrayInverted);
    assert_eq!(f.crop, [0, 0, 400, 200]);
}

#[test]
fn a_spectrogram_with_its_axes_given_says_when_and_at_what_pitch() {
    let (w, h, seconds) = (800u32, 300u32, 20.0f32);
    let p = picture(w, h, Colors::Viridis, 30, song_plot(w, h, seconds));
    let given = Calibration {
        seconds: Some(seconds),
        low_hz: Some(50.0),
        high_hz: Some(10_000.0),
        scale: Some(FreqScale::Log),
        ..Default::default()
    };
    let bp = from_picture(&p, Path::new("plot.png"), &given);
    assert_eq!(bp.source, Source::Image);
    assert_eq!((bp.seconds, bp.frames), (Some(20.0), 400));
    assert_eq!(bp.bands_hz.len(), BANDS);
    let line = bp
        .contours
        .iter()
        .max_by_key(|c| c.end - c.start)
        .expect("the held line");
    let hz = line.hz.unwrap();
    assert!((hz / 1000.0 - 1.0).abs() < 0.08, "{hz}");
    // The kick: two low starts a second.
    let d = bp.density(0, bp.frames);
    assert!((1.5..2.6).contains(&d[0]), "{d:?}");
    // What a picture can't show stays unknown; its levels are relative.
    for what in [
        "Chords",
        "Instruments",
        "Loudness (LUFS)",
        "Stereo width",
        "Lyrics",
    ] {
        assert!(
            bp.facts
                .iter()
                .any(|f| f.name == what && f.basis == Basis::Unknown),
            "{what}"
        );
    }
    assert!(
        bp.facts
            .iter()
            .any(|f| f.name == "Level scale" && f.basis == Basis::Estimated)
    );
    assert!(bp.needs.is_empty(), "{:?}", bp.needs);
    // At this resolution the tempo can be guessed: two beats a second.
    assert!(
        bp.tempo
            .is_some_and(|t| (t - 120.0).abs() < 8.0 || (t - 60.0).abs() < 4.0),
        "{:?}",
        bp.tempo
    );
}

#[test]
fn a_spectrogram_without_its_axes_asks_for_them_and_claims_no_pitch() {
    let p = picture(500, 200, Colors::Magma, 0, song_plot(500, 200, 25.0));
    let bp = from_picture(&p, Path::new("plot.png"), &Calibration::default());
    assert_eq!(bp.seconds, None);
    assert_eq!(bp.frames, 400);
    assert!(bp.bands_hz.is_empty());
    assert!(bp.chroma.is_empty() && bp.key.is_none() && bp.tempo.is_none());
    assert!(bp.contours.iter().all(|c| c.hz.is_none()));
    assert_eq!(bp.needs.len(), 2, "{:?}", bp.needs);
    for what in ["Length", "Frequencies", "Tempo"] {
        assert!(
            bp.facts
                .iter()
                .any(|f| f.name == what && f.basis == Basis::Unknown),
            "{what}"
        );
    }
    // Its brief places sections through it rather than in time.
    let brief = bp.brief();
    assert!(brief["sections"][0].get("from").is_some());
    assert!(brief["sections"][0].get("start_s").is_none());
}

#[test]
fn a_picture_round_trips_through_a_png_file() {
    let dir = std::env::temp_dir().join(format!("pina-ref-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = picture(300, 120, Colors::Inferno, 20, song_plot(300, 120, 10.0));
    let file = dir.join("plot.png");
    ::image::RgbImage::from_fn(p.width, p.height, |x, y| ::image::Rgb(p.at(x, y)))
        .save(&file)
        .unwrap();
    assert!(image::is_picture(&file));
    let bp = image::from_image(&file, &Calibration::default()).unwrap();
    assert_eq!(
        bp.calibration.as_ref().unwrap().colors,
        Some(Colors::Inferno)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_song_compared_with_itself_is_alike_and_a_darker_sparser_one_says_where() {
    let bpm = 120.0;
    let seconds = 16.0;
    let reference = mix(&beat(bpm, seconds, true), &tone(523.25, seconds, 0.2));
    let sh = shape(bpm, seconds, &[("Verse", 0.0, 8.0), ("Chorus", 8.0, 16.0)]);
    let r = blueprint(&reference, Some(&sh));
    let same = compare::compare(&r, &blueprint(&reference, Some(&sh)));
    for m in Measure::ALL {
        assert!(
            same.overall.get(m).is_some_and(|v| v > 0.95),
            "{m:?}: {:?}",
            same.overall
        );
    }
    assert!(same.differences.is_empty(), "{:?}", same.differences);
    // No hats and no line in the chorus: darker, sparser up top, fewer held lines.
    let mut song = reference.clone();
    let half = (8.0 * RATE) as usize;
    let bare = beat(bpm, seconds, false);
    song[half..].copy_from_slice(&bare[half..]);
    let s = blueprint(&song, Some(&sh));
    let report = compare::compare(&r, &s);
    let chorus = &report.sections[1];
    assert!(chorus.scores.groove.unwrap() < 0.8, "{chorus:?}");
    assert!(report.sections[0].scores.groove.unwrap() > 0.9);
    assert!(
        report.differences.iter().any(|d| d.section == 1
            && d.measure == Measure::Groove
            && d.parts.contains(&"hats".to_string())),
        "{:#?}",
        report.differences
    );
    assert!(
        report.differences.iter().all(|d| d.section == 1),
        "{:#?}",
        report.differences
    );
    // "Closer drums in the chorus" aims at the chorus's groove; putting the hats back reaches it.
    let t = compare::target("closer drums in the chorus", &report);
    assert_eq!(
        (t.measures.clone(), t.sections.clone()),
        (vec![Measure::Groove], vec![1])
    );
    let fixed = compare::compare(&r, &blueprint(&reference, Some(&sh)));
    let p = compare::progress(&report, &fixed, &t);
    assert_eq!(p.verdict, Verdict::Reached);
    assert!(p.after > p.before);
    // Nothing changed is no better.
    assert_eq!(
        compare::progress(&report, &report, &t).verdict,
        Verdict::Stalled
    );
}

#[test]
fn a_render_is_compared_with_a_picture_of_its_reference_by_place_and_relative_level() {
    // The reference, drawn: quiet first half, a busy loud second half.
    let (w, h) = (600u32, 200u32);
    let p = picture(w, h, Colors::Viridis, 0, |x, y| {
        let loud = x >= w / 2;
        let kick = loud && (x % 30) < 3 && y > h * 2 / 3;
        0.15 + if loud { 0.35 } else { 0.0 } + if kick { 0.4 } else { 0.0 }
    });
    let reference = from_picture(&p, Path::new("plot.png"), &Calibration::default());
    assert!(reference.sections.len() >= 2, "{:?}", reference.sections);
    // A song that stays quiet throughout: its second half falls short.
    let song = blueprint(&tone(200.0, 20.0, 0.1), None);
    let report = compare::compare(&reference, &song);
    assert!(report.relative);
    assert!(
        report.sections.iter().all(|s| s.scores.balance.is_none()),
        "no frequency axis"
    );
    let last = report.sections.last().unwrap();
    assert!(last.energy_db < -2.0, "{last:?}");
    assert!(
        report
            .differences
            .iter()
            .any(|d| d.measure == Measure::Energy)
    );
}

/// A spectrogram of `x` drawn as a plotting tool draws one: a 4096-point spectrum a column, rows
/// up a log axis from 40 Hz to 16 kHz, magma over 70 dB.
fn drawn(x: &[f32], w: u32, h: u32) -> Picture {
    drawn_at(x, RATE, w, h)
}

fn drawn_at(x: &[f32], rate: f32, w: u32, h: u32) -> Picture {
    let size = 4096usize;
    let mut spectra = crate::samples::dsp::Spectra::new(size);
    let mut power = vec![0f32; size / 2 + 1];
    let bin = rate / size as f32;
    let mut db = vec![0f32; (w * h) as usize];
    for col in 0..w {
        let at = (col as f32 + 0.5) / w as f32 * x.len() as f32;
        spectra.power(x, at as isize - (size / 2) as isize, &mut power);
        for row in 0..h {
            let hz = FreqScale::Log.hz(40.0, 16_000.0, 1.0 - (row as f32 + 0.5) / h as f32);
            let k = (hz / bin).clamp(1.0, (size / 2 - 1) as f32) as usize;
            db[(row * w + col) as usize] = 10.0 * power[k].max(1e-12).log10();
        }
    }
    let top = db.iter().copied().fold(f32::MIN, f32::max);
    let t = table(Colors::Magma);
    Picture {
        width: w,
        height: h,
        pixels: db
            .iter()
            .map(|&v| {
                t[(((v - (top - 70.0)) / 70.0).clamp(0.0, 1.0) * 63.0) as usize].map(|c| c as u8)
            })
            .collect(),
    }
}

#[test]
fn a_song_against_a_picture_of_itself_reads_alike() {
    let x = mix(&beat(110.0, 16.0, true), &tone(392.0, 16.0, 0.2));
    let song = blueprint(&x, None);
    let given = Calibration {
        seconds: Some(16.0),
        low_hz: Some(40.0),
        high_hz: Some(16_000.0),
        scale: Some(FreqScale::Log),
        db_range: Some(70.0),
        ..Default::default()
    };
    let picture = from_picture(&drawn(&x, 800, 300), Path::new("self.png"), &given);
    let b = (
        song.balance(0, song.frames),
        picture.balance(0, picture.frames),
    );
    for k in 0..3 {
        assert!((b.0[k] - b.1[k]).abs() < 4.0, "{:?} against {:?}", b.0, b.1);
    }
    // Its starts, balance and energy read alike whichever it came from.
    let report = compare::compare(&picture, &song);
    let o = report.overall;
    assert!(
        o.balance.unwrap() > 0.7 && o.groove.unwrap() > 0.75 && o.energy.unwrap() > 0.9,
        "{o:?}"
    );
    assert!(report.differences.is_empty(), "{:#?}", report.differences);
}

#[test]
fn a_picture_of_no_known_length_compares_its_starts_against_its_own() {
    // The same beat as audio and as a picture with its frequencies but not its length.
    let x = mix(&beat(120.0, 30.0, true), &tone(392.0, 30.0, 0.2));
    let song = blueprint(&x, None);
    let given = Calibration {
        low_hz: Some(40.0),
        high_hz: Some(16_000.0),
        scale: Some(FreqScale::Log),
        ..Default::default()
    };
    let picture = from_picture(&drawn(&x, 800, 300), Path::new("untimed.png"), &given);
    assert_eq!(picture.seconds, None);
    let report = compare::compare(&picture, &song);
    assert!(
        report.overall.groove.unwrap() > 0.75,
        "{:?}",
        report.overall
    );
    assert!(
        report
            .differences
            .iter()
            .all(|d| d.measure != Measure::Groove),
        "{:#?}",
        report.differences
    );
}

#[test]
fn a_request_names_a_numbered_section_or_every_one_of_its_name() {
    let section = |name: &str, label: &str| compare::Section {
        name: name.into(),
        label: label.into(),
        reference: (0, 1),
        song: (0, 1),
        scores: compare::Scores::default(),
        balance_db: None,
        energy_db: 0.0,
        density: [None; 3],
    };
    let report = compare::Report {
        relative: false,
        overall: compare::Scores::default(),
        sections: vec![
            section("Verse 1", "A"),
            section("Chorus", "B"),
            section("Verse 2", "A"),
            section("Chorus 2", "B"),
        ],
        differences: Vec::new(),
    };
    let t = |r: &str| compare::target(r, &report);
    assert_eq!(t("closer groove in verse 2").sections, [2]);
    assert_eq!(t("brighter chorus").sections, [1, 3]);
    assert_eq!(t("more energy in the verses").sections, [0, 2]);
    assert_eq!(t("closer drums").sections, [0, 1, 2, 3]);
    assert_eq!(t("closer drums").measures, [Measure::Groove]);
}

/// How readings hold up on real songs (`PINA_EVAL_SONGS`, paths split by `|`): each read from
/// its audio (with its study) and from pictures of its spectrogram of several qualities, each
/// picture compared with the audio (the same song: alike is right); then each song against the
/// others (different songs: less alike is right). Prints a table; asserts nothing.
#[test]
#[ignore]
fn evaluate_on_real_songs() {
    let Ok(list) = std::env::var("PINA_EVAL_SONGS") else {
        return;
    };
    let pct = |v: Option<f32>| v.map_or(" – ".into(), |v| format!("{:>3.0}", v * 100.0));
    let row = |o: &compare::Scores| {
        format!(
            "{} {} {} {}",
            pct(o.groove),
            pct(o.tonal),
            pct(o.balance),
            pct(o.energy)
        )
    };
    let mut audio = Vec::new();
    println!("| song | reading | groove tonal balance energy | differences | tempo | key |");
    for path in list.split('|').map(Path::new) {
        let s = crate::study::study(path).expect("studied");
        let (mut sides, rate) = crate::audio::engine::read_audio(path).unwrap();
        let l = sides.remove(0);
        let r = sides.pop().unwrap_or_else(|| l.clone());
        let a = from_audio(
            &l,
            &r,
            rate as f32,
            Source::Audio,
            path.into(),
            Some(&Shape::of_study(&s)),
            Some(&s),
        );
        let name = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .chars()
            .take(24)
            .collect::<String>();
        println!(
            "| {name} | audio | {} sections | | {:.0} | {} |",
            a.sections.len(),
            s.tempo.bpm,
            s.key.name
        );
        let seconds = l.len() as f32 / rate as f32;
        let mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
        let axes = Calibration {
            seconds: Some(seconds),
            low_hz: Some(40.0),
            high_hz: Some(16_000.0),
            scale: Some(FreqScale::Log),
            ..Default::default()
        };
        let clean = drawn_at(&mono, rate as f32, 1200, 400);
        let jpeg = {
            let img = ::image::RgbImage::from_fn(clean.width, clean.height, |x, y| {
                ::image::Rgb(clean.at(x, y))
            });
            let mut buf = Vec::new();
            ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 40)
                .encode_image(&img)
                .unwrap();
            let back = ::image::load_from_memory(&buf).unwrap().to_rgb8();
            Picture {
                width: back.width(),
                height: back.height(),
                pixels: back.pixels().map(|p| p.0).collect(),
            }
        };
        let small = drawn_at(&mono, rate as f32, 300, 100);
        for (what, pic, cal) in [
            ("PNG 1200×400, axes", &clean, axes.clone()),
            (
                "PNG 1200×400, axes and dB",
                &clean,
                Calibration {
                    db_range: Some(70.0),
                    ..axes.clone()
                },
            ),
            ("JPEG q40, axes", &jpeg, axes.clone()),
            ("PNG 300×100, axes", &small, axes.clone()),
            ("PNG, no axes", &clean, Calibration::default()),
        ] {
            let p = from_picture(pic, path, &cal);
            let rep = compare::compare(&p, &a);
            let tonal = p
                .facts
                .iter()
                .find(|f| f.name == "Tonal centre")
                .map_or("–".into(), |f| f.value.clone());
            let mut by: Vec<&str> = rep.differences.iter().map(|d| d.measure.name()).collect();
            by.sort();
            by.dedup();
            println!(
                "| {name} | {what} | {} | {} ({}) | {} | {tonal} |",
                row(&rep.overall),
                rep.differences.len(),
                by.join(", "),
                p.tempo.map_or("–".into(), |t| format!("{t:.0}")),
            );
        }
        audio.push((name, a));
    }
    println!();
    println!("| reference | song | groove tonal balance energy | differences |");
    for (rn, r) in &audio {
        for (sn, s) in &audio {
            if rn != sn {
                let rep = compare::compare(r, s);
                println!(
                    "| {rn} | {sn} | {} | {} |",
                    row(&rep.overall),
                    rep.differences.len()
                );
            }
        }
    }
}

/// Sines at MIDI notes, each `(note, start beat, beats)` at 120 BPM (a beat half a second).
fn notes_at(notes: &[(u8, f32, f32)], seconds: f32, amp: f32) -> Vec<f32> {
    let mut x = vec![0f32; (seconds * RATE) as usize];
    for &(n, at, beats) in notes {
        let hz = 440.0 * 2f32.powf((n as f32 - 69.0) / 12.0);
        let (a, z) = (
            (at * 0.5 * RATE) as usize,
            ((at + beats) * 0.5 * RATE) as usize,
        );
        for i in a..z.min(x.len()) {
            let t = (i - a) as f32 / RATE;
            x[i] += amp * (TAU * hz * t).sin() * (1.0 - (-t * 200.0).exp());
        }
    }
    x
}

#[test]
fn a_bass_line_and_a_melody_are_read_note_by_note_and_compared() {
    let seconds = 16.0;
    // Four bars twice over: the bass D, F#, A, D a beat each; a melody A4, B4, D5, B4 on beats
    // 1, 2, 2.5 and 3 of each bar.
    let mut bass = Vec::new();
    let mut tune = Vec::new();
    let mut other = Vec::new();
    for bar in 0..8 {
        let b = bar as f32 * 4.0;
        for (k, n) in [38u8, 42, 45, 38].into_iter().enumerate() {
            bass.push((n, b + k as f32, 1.0));
        }
        for &(n, at, len) in &[
            (69u8, 0.0, 1.0),
            (71, 1.0, 0.5),
            (74, 1.5, 0.5),
            (71, 2.0, 2.0),
        ] {
            tune.push((n, b + at, len));
        }
        // Another melody: C5 and E5 a half note each.
        other.push((72u8, b, 2.0));
        other.push((76u8, b + 2.0, 2.0));
    }
    let low = notes_at(&bass, seconds, 0.4);
    let reference = mix(&low, &notes_at(&tune, seconds, 0.25));
    let sh = shape(
        120.0,
        seconds,
        &[("Verse", 0.0, 8.0), ("Chorus", 8.0, 16.0)],
    );
    let r = blueprint(&reference, Some(&sh));
    let pcs = r.line_classes(&r.bass, 0, r.frames).expect("a bass line");
    let (d, fs, a) = (pcs[2], pcs[6], pcs[9]);
    assert!(d + fs + a > 0.8, "D, F#, A: {pcs:?}");
    let pitches: Vec<u8> = r.melody.iter().map(|n| n.pitch).collect();
    for n in [69u8, 71, 74] {
        assert!(pitches.contains(&n), "{n} in {pitches:?}");
    }
    let brief = r.brief();
    let b = brief["sections"][0]["bass"].as_str().unwrap();
    assert!(b.starts_with("D2:"), "{b}");
    assert!(
        brief["sections"][0]["melody"]
            .as_str()
            .unwrap()
            .contains("A4:")
    );
    // The same song: no line differs. Another melody over the same bass: the melody does.
    let same = compare::compare(&r, &blueprint(&reference, Some(&sh)));
    assert!(same.differences.is_empty(), "{:#?}", same.differences);
    let song = mix(&low, &notes_at(&other, seconds, 0.25));
    let report = compare::compare(&r, &blueprint(&song, Some(&sh)));
    assert!(
        report
            .differences
            .iter()
            .any(|d| d.says.contains("melody") && d.parts.contains(&"lead".to_string())),
        "{:#?}",
        report.differences
    );
    assert!(
        !report
            .differences
            .iter()
            .any(|d| d.says.contains("bass leans")),
        "{:#?}",
        report.differences
    );
}
