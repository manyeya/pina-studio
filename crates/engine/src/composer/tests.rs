use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::atomic::AtomicBool,
};

use serde_json::{Value, json};

use super::{
    catalog::{KNOWN, Known},
    score::{self, Ask, Part},
    server,
};
use crate::song::project::{Content, Instrument, Note, Project, Region, Track};

fn note(start: i64, length: u64, pitch: u8) -> Note {
    Note {
        start,
        length,
        pitch,
        velocity: 100,
        ..Default::default()
    }
}

/// A song in A minor, 4/4 at 120: drums, and a bass over its first two bars.
fn song() -> Project {
    let mut p = Project::new(48_000);
    p.tempo = 120.0;
    p.time_signature = [4, 4];
    p.key = Some("A minor".into());
    let bar = p.bar() as i64;
    let region = |notes: Vec<Note>| Region {
        start: 0,
        length: (bar * 4) as u64,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    };
    p.tracks.push(Track {
        name: "Drums".into(),
        instrument: Some(Instrument::Drums),
        regions: vec![region(
            (0..16).map(|b| note(b * bar / 4, 120, 36)).collect(),
        )],
        ..Default::default()
    });
    p.tracks.push(Track {
        name: "Bass".into(),
        instrument: Some(Instrument::Synth),
        regions: vec![region(vec![note(0, 480, 33), note(bar + 960, 480, 36)])],
        ..Default::default()
    });
    p
}

/// What MIDI-GPT's /info says, cut down to what a prompt reads.
fn caps() -> Value {
    json!({
        "capabilities": { "pitch_mask_scale_presets": ["blues", "major", "minor", "dorian"] },
        "attributes": { "note_density": 10, "min_polyphony": 10, "max_polyphony": 10 },
        "attribute_track_types": {
            "note_density": "drum", "min_polyphony": "melodic", "max_polyphony": "melodic"
        },
    })
}

#[test]
fn parts_are_told_by_their_notes() {
    let bass = [note(0, 480, 36), note(480, 480, 38)];
    assert_eq!(Part::heard(&bass), Part::Bass);
    let chord = [note(0, 960, 60), note(0, 960, 64), note(0, 960, 67)];
    assert_eq!(Part::heard(&chord), Part::Keys);
    let lead = [note(0, 240, 72), note(240, 240, 74)];
    assert_eq!(Part::heard(&lead), Part::Lead);
    assert_eq!(Part::heard(&[]), Part::Keys);
    assert_eq!(Part::named("bass"), Some(Part::Bass));
}

#[test]
fn a_song_becomes_a_score_and_an_ask() {
    let p = song();
    let bar = p.bar();
    let mut ask = Ask {
        tracks: vec![(1, None)],
        bars: (2, 4),
        temperature: 0.9,
        ..Default::default()
    };
    ask.attributes.insert("max_polyphony".into(), 4);
    ask.attributes.insert("note_density".into(), 7);
    ask.attributes.insert("min_polyphony".into(), 40);
    // The bass's own steering over what every part's asked.
    ask.steer
        .insert(1, [("max_polyphony".to_string(), 2)].into());
    let pr = score::prompt(&p, &ask, &caps()).unwrap();
    assert_eq!(pr.order, [0, 1]);
    assert_eq!(pr.bar, bar);
    let s = &pr.body["score"];
    assert_eq!(s["resolution"], 960);
    assert_eq!(s["tempo"], 500_000);
    // Drums heard as drums, the bass as a bass (by its notes), each bar's notes from its start.
    assert_eq!(s["tracks"][0]["track_type"], "drum");
    assert_eq!(s["tracks"][1]["track_type"], "melodic");
    assert_eq!(s["tracks"][1]["instrument"], 33);
    assert_eq!(s["tracks"][1]["bars"].as_array().unwrap().len(), 4);
    assert_eq!(s["tracks"][1]["bars"][1]["notes"][0]["onset_ticks"], 960);
    assert_eq!(
        s["tracks"][0]["bars"][2]["notes"].as_array().unwrap().len(),
        4
    );
    // The drums heard, the bass written over bars 3 and 4: in A minor, with its own controls.
    let tracks = &pr.body["request"]["tracks"];
    assert_eq!(tracks[0], json!({ "id": 0, "bars": [] }));
    assert_eq!(tracks[1]["bars"], json!([2, 3]));
    assert_eq!(
        tracks[1]["controls"]["pitch_mask"],
        json!({ "scale": "minor", "root": 9 })
    );
    // Only a melodic part's controls, each within its levels.
    assert_eq!(
        tracks[1]["attributes"],
        json!({ "max_polyphony": 2, "min_polyphony": 9 })
    );
    let config = &pr.body["request"]["config"];
    assert_eq!(config["seed"], -1);
    // Each take its own request, a seed asked for followed take by take.
    assert_eq!(pr.take(2, Some(10))["request"]["config"]["seed"], 12);
    assert_eq!(pr.take(2, None)["request"]["config"]["seed"], -1);
    assert!((config["temperature"].as_f64().unwrap() - 0.9).abs() < 1e-6);
    assert_eq!(pr.body["stream"], true);
}

#[test]
fn what_cant_be_written_says_why() {
    let p = song();
    let none = |ask: Ask| score::prompt(&p, &ask, &caps()).err().unwrap();
    assert!(none(Ask::default()).contains("pick a track"));
    assert!(
        none(Ask {
            tracks: vec![(1, None)],
            bars: (3, 3),
            ..Default::default()
        })
        .contains("first must come before")
    );
    assert!(
        none(Ask {
            tracks: vec![(7, None)],
            ..Default::default()
        })
        .contains("no track 7")
    );
}

#[test]
fn an_empty_track_is_written_as_the_part_asked_for() {
    let mut p = song();
    p.tracks.push(Track {
        name: "Keys".into(),
        instrument: Some(Instrument::Synth),
        ..Default::default()
    });
    let ask = Ask {
        tracks: vec![(2, Some(Part::Strings))],
        bars: (0, 4),
        ..Default::default()
    };
    let pr = score::prompt(&p, &ask, &caps()).unwrap();
    assert_eq!(pr.order, [0, 1, 2]);
    assert_eq!(pr.body["score"]["tracks"][2]["instrument"], 48);
    assert_eq!(pr.body["request"]["tracks"][2]["bars"], json!([0, 1, 2, 3]));
}

#[test]
fn takes_come_back_as_the_songs_notes() {
    let p = song();
    let ask = Ask {
        tracks: vec![(1, None)],
        bars: (2, 4),
        ..Default::default()
    };
    let pr = score::prompt(&p, &ask, &caps()).unwrap();
    let bar = pr.bar as i64;
    // Written at the model's own resolution (12 to the quarter), a note pushed by a tick.
    let wrote = |pitch: u8| {
        json!({
            "resolution": 12, "tempo": 500000,
            "tracks": [
                { "instrument": 0, "track_type": "drum", "bars": [] },
                { "instrument": 33, "track_type": "melodic", "bars": [
                    { "ts_numerator": 4, "ts_denominator": 4, "notes": [] },
                    { "ts_numerator": 4, "ts_denominator": 4, "notes": [] },
                    { "ts_numerator": 4, "ts_denominator": 4, "notes": [
                        { "pitch": pitch, "velocity": 90, "onset_ticks": 12, "duration_ticks": 6, "delta": 1 }
                    ] },
                    { "ts_numerator": 4, "ts_denominator": 4, "notes": [] },
                ] },
            ],
        })
    };
    let one = json!({ "score": wrote(40), "seed": 7, "generated_bars": { "1": [2] } });
    let takes = score::takes(&one, &pr, &ask).unwrap();
    assert_eq!(takes.len(), 1);
    assert_eq!(takes[0].seed, 7);
    let n = &takes[0].notes[&1][0];
    assert_eq!(n.start, 2 * bar + 13 * 80);
    assert_eq!((n.length, n.pitch, n.velocity), (480, 40, 90));
    // Several: each written take, one that failed left out.
    let many = json!({
        "generated_bars": { "1": [2, 3] },
        "candidates": [
            { "score": wrote(41), "seed": 1 },
            { "score": null, "seed": 2, "error": "no notes" },
            { "score": wrote(43), "seed": 3 },
        ],
    });
    let takes = score::takes(&many, &pr, &ask).unwrap();
    assert_eq!(takes.iter().map(|t| t.seed).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(takes[1].notes[&1][0].pitch, 43);
    let failed = json!({ "candidates": [{ "score": null, "seed": 1, "error": "out of range" }] });
    assert_eq!(
        score::takes(&failed, &pr, &ask).err().unwrap(),
        "out of range"
    );
}

/// A stand-in model server: answers one request with `reply` (headers and body), and hands back
/// what it was asked.
fn serve(reply: String) -> (String, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut head = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap();
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        r.read_exact(&mut body).unwrap();
        s.write_all(reply.as_bytes()).unwrap();
        head + &String::from_utf8(body).unwrap()
    });
    (url, handle)
}

#[test]
fn the_models_info_is_read() {
    let body = json!({ "model": "yellow_small", "attributes": { "note_density": 10 } }).to_string();
    let (url, asked) = serve(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    let info = server::info(&url).unwrap();
    assert_eq!(info["attributes"]["note_density"], 10);
    assert!(asked.join().unwrap().starts_with("GET /info"));
}

#[test]
fn a_take_streams_and_comes_back_whole() {
    let note = json!({ "track": 1, "bar": 2, "pitch": 40, "onset_ticks": 6, "duration_ticks": 3, "velocity": 90 });
    let events = [
        json!({ "type": "progress", "progress": { "generated_tokens": 32 } }),
        json!({ "type": "notes", "notes": [note, note], "progress": { "generated_tokens": 64 } }),
        json!({ "type": "done", "response": { "status": "completed", "seed": 5 } }),
    ];
    let body: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
    let (url, asked) = serve(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
    ));
    let (mut seen, mut notes) = (Vec::new(), Vec::new());
    let answer = server::generate(
        &url,
        &json!({ "stream": true }),
        &mut |p, new| {
            seen.push(p);
            notes.extend(new);
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(answer["seed"], 5);
    assert_eq!(seen.last().map(|p| (p.tokens, p.notes)), Some((64, 2)));
    // As the song's notes: the bass (the score's track 1) in bar 3, at the model's 12 a quarter.
    let p = song();
    let ask = Ask {
        tracks: vec![(1, None)],
        bars: (2, 4),
        ..Default::default()
    };
    let pr = score::prompt(&p, &ask, &caps()).unwrap();
    let w = score::written(&notes, &pr, 12);
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].0, 1);
    assert_eq!(
        (w[0].1.start, w[0].1.length),
        (2 * pr.bar as i64 + 480, 240)
    );
    let asked = asked.join().unwrap();
    assert!(asked.starts_with("POST /generate"));
    assert!(asked.ends_with(r#"{"stream":true}"#));
}

#[test]
fn a_refused_ask_says_why() {
    let body = json!({ "detail": "bar 40 is past the score's end" }).to_string();
    let (url, _) = serve(format!(
        "HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    let err = server::generate(&url, &json!({}), &mut |_, _| {}, &AtomicBool::new(false));
    assert_eq!(err.err().unwrap(), "bar 40 is past the score's end");
    assert!(!server::healthy("http://127.0.0.1:9"));
}

#[test]
fn the_catalogue_is_pinned() {
    let mut ids: Vec<&str> = KNOWN.iter().map(|k| k.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), KNOWN.len());
    for k in KNOWN {
        // A file at one commit, known to the byte.
        assert!(k.url.contains("/resolve/9a21a39"), "{}", k.id);
        assert!(k.bytes > 1_000_000 && !k.bars.is_empty(), "{}", k.id);
        assert!(k.gated && k.licence.contains("Non-Commercial"), "{}", k.id);
    }
    assert!(Known::named("midigpt-yellow-small").is_some());
    let dir = std::env::temp_dir().join(format!("pina-composers-{}", std::process::id()));
    // Not installed, nothing listed; a gated one asked for without signing in says how.
    assert!(super::installed(&dir).is_empty());
    let err = super::catalog::download(&KNOWN[0], &dir, None, |_| {}, &AtomicBool::new(false));
    assert!(err.err().unwrap().contains("sign in"));
    assert!(!dir.exists());
}
