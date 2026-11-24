// A web build leaves plugin hosting, MP3 export and Rubber Band out; what only they used is idle.
#![cfg_attr(target_family = "wasm", allow(dead_code, unused_imports))]

pub mod audio;
#[cfg(not(target_family = "wasm"))]
pub mod composer;
pub mod dsp;
pub mod lyrics;
pub mod music;
pub mod paths;
pub mod plugin;
pub mod samples;
pub mod separate;
pub mod smart;
pub mod song;
pub mod study;
