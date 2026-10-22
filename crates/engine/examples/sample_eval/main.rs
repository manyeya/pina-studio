//! Measures the sample analysis against what packs' file names say: category (with names, and
//! from the audio alone), loop or one-shot, pitch, tempo, key, how alike the embedding's
//! neighbours are, duplicates by fingerprint, and speed.
//!
//!     cargo run --release -p pina-engine --example sample_eval -- [options] [pack folders]
//!
//! It always measures MusicRadar's free "90s Hip Hop" pack, where Pina Studio keeps it
//! (`~/Music/Pina Studio/Samples/MusicRadar 90s Hip Hop`); folders of the other free SampleRadar
//! packs `truth` knows the names of can be given too, each measured on its own.
//!
//! - `--list`: every file's result too.
//! - `--scan`: times a whole-library scan of the 90s pack on every core.
//! - `--time`: times analysis file by file on one core.
//! - `--tempo`, `--keys`: tempo, or key, alone, for files whose names give one.
//! - `--fit <model folder>`: fits the classifiers, the embedding and the traits' statistics on
//!   the labelled packs given (the 90s pack is held out, only measured) and writes them to
//!   `src/samples/model/`, a file for each table. `LOPO=1` also scores each pack by a model fitted on the others; `CONFUSION=1`
//!   and `KNN=1` print what the held-out pack's sounds are taken for, and their neighbours.
//! - `--cache <file>`: keeps every file's measures there, to fit again without analysing again.

mod checks;
mod fitting;
mod scores;
mod truth;

use checks::*;
use fitting::*;
use scores::*;
use truth::*;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use pina_engine::samples::{
    Analysis, Category, Kind, NOTES,
    analyze::{analyze, trait_inputs},
    classify, features,
    library::{Library, audio_files},
    note_name,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut folders: Vec<PathBuf> = Vec::new();
    let (mut fit_to, mut show, mut scan, mut tempo_only) = (None, false, false, false);
    let mut cache: Option<PathBuf> = None;
    let mut keys_only = false;
    let mut time_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fit" => {
                i += 1;
                fit_to = Some(PathBuf::from(&args[i]));
            }
            "--list" => show = true,
            "--scan" => scan = true,
            "--tempo" => tempo_only = true,
            "--keys" => keys_only = true,
            "--time" => time_only = true,
            "--cache" => {
                i += 1;
                cache = Some(PathBuf::from(&args[i]));
            }
            other => folders.push(PathBuf::from(other)),
        }
        i += 1;
    }
    let pack = PathBuf::from(std::env::var("HOME").unwrap())
        .join("Music/Pina Studio/Samples/MusicRadar 90s Hip Hop");
    if !folders.contains(&pack) {
        folders.insert(0, pack.clone());
    }
    let files: Vec<PathBuf> = folders.iter().flat_map(|f| audio_files(f)).collect();
    println!("{} files", files.len());
    if tempo_only {
        tempo_check(&files);
        return;
    }
    if keys_only {
        key_check(&files);
        return;
    }
    if time_only {
        time_check(&files);
        return;
    }
    let start = Instant::now();
    // A cache of every file's measures, to fit again without analysing again.
    let rows = match cache.as_ref().and_then(|c| std::fs::read(c).ok()) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap(),
        None => {
            let rows = run(&files, fit_to.is_some());
            if let Some(c) = &cache {
                std::fs::write(c, serde_json::to_vec(&rows).unwrap()).unwrap();
            }
            rows
        }
    };
    println!(
        "analysed in {:.1} s (each file twice: with its name and without)",
        start.elapsed().as_secs_f32()
    );
    if let Some(out) = fit_to {
        fit(&rows, &out);
        return;
    }
    let mut packs: Vec<String> = rows.iter().map(|r| r.truth.pack.clone()).collect();
    packs.sort();
    packs.dedup();
    for pack in packs {
        let of: Vec<&Row> = rows.iter().filter(|r| r.truth.pack == pack).collect();
        println!(
            "\n==== {} ({} files)",
            if pack.is_empty() { "unlabelled" } else { &pack },
            of.len()
        );
        if show {
            list(&of);
        }
        categories(&of, false, "Category with names");
        categories(&of, true, "Category from audio alone");
        kinds(&of);
        pitches(&of);
        tempos(&of);
        keys(&of);
        neighbours(&of);
        duplicates(&of);
        speed(&of);
    }
    if scan {
        let dir = std::env::temp_dir().join(format!("pina-eval-{}", std::process::id()));
        // SAFETY: nothing else in this process reads the environment now.
        unsafe { std::env::set_var("PINA_LIBRARY", &dir) };
        let mut library = Library {
            folders: vec![pack],
            samples: Vec::new(),
        };
        let start = Instant::now();
        let report = library.scan(&|_, _| {});
        println!(
            "\nLibrary::scan of the 90s pack: {} samples in {:.1} s ({} failed)",
            report.samples,
            start.elapsed().as_secs_f32(),
            report.failed.len()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
