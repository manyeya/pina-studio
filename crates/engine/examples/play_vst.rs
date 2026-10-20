//! Loads an instrument (VST3, CLAP or VST2 bundle, or with `au` the first Audio Unit
//! instrument), opens its editor, and plays an arpeggio through it.
//!
//!     cargo run -p pina-engine --example play_vst -- "/path/to/Surge XT.vst3"

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use pina_engine::{
    plugin::{self, Libraries, Note},
    scan,
};

const MAX_BLOCK: usize = 512;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: play_vst <plugin.vst3>");
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let found = scan::inspect(Path::new(&path)).unwrap();
    for p in &found {
        println!(
            "plugin: {} ({}, instrument: {})",
            p.class,
            p.format.name(),
            p.instrument
        );
    }
    let info = found
        .iter()
        .find(|p| p.instrument)
        .or(found.first())
        .expect("no plugins there");

    let device = cpal::default_host()
        .default_output_device()
        .expect("no output device");
    let config = device.default_output_config().unwrap().config();
    let channels = config.channels as usize;
    println!("output: {} Hz, {channels} channels", config.sample_rate);

    let rate = config.sample_rate as f64;
    let (plugin, mut processor) = plugin::instantiate(
        &mut Libraries::new(),
        &info.plugin_ref(),
        rate,
        MAX_BLOCK,
        None,
    )
    .unwrap();
    println!("loaded {}", plugin.name());

    let (mut notes_tx, mut notes_rx) = rtrb::RingBuffer::<Note>::new(256);
    // Loudest sample since the last print, so a run shows the plugin is making sound.
    let peak = Arc::new(AtomicU32::new(0));
    let audio_peak = peak.clone();
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], _| {
                for chunk in out.chunks_mut(MAX_BLOCK * channels) {
                    let frames = chunk.len() / channels;
                    let mut notes = [Note::default(); 32];
                    let mut n = 0;
                    while n < notes.len() {
                        let Ok(note) = notes_rx.pop() else { break };
                        notes[n] = note;
                        n += 1;
                    }
                    let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
                    processor.process([&mut l, &mut r], frames, &notes[..n], &[], None);
                    for (f, frame) in chunk.chunks_mut(channels).enumerate() {
                        for (c, s) in frame.iter_mut().enumerate() {
                            *s = if c % 2 == 0 { l[f] } else { r[f] };
                            audio_peak.fetch_max(s.abs().to_bits(), Ordering::Relaxed);
                        }
                    }
                }
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .unwrap();
    stream.play().unwrap();

    let _editor = plugin.open_editor(mtm).unwrap();

    thread::spawn(move || {
        for pitch in [60u8, 64, 67, 72, 67, 64].into_iter().cycle() {
            let _ = notes_tx.push(Note {
                offset: 0,
                pitch,
                velocity: 0.8,
            });
            thread::sleep(Duration::from_millis(180));
            let _ = notes_tx.push(Note {
                offset: 0,
                pitch,
                velocity: 0.0,
            });
            thread::sleep(Duration::from_millis(20));
            println!(
                "note {pitch}: peak {:.3}",
                f32::from_bits(peak.swap(0, Ordering::Relaxed))
            );
        }
    });

    app.activate();
    app.run();
}
