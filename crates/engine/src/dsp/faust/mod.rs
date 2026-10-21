//! The channel strip's compressor and saturator, written in Faust (`crates/engine/faust/*.dsp`)
//! and compiled to Rust by `scripts/faust.sh`. The generated code is checked in, so building Pina Studio
//! doesn't need Faust. Around it, this is what it expects: Faust's DSP and UI traits, as its Rust
//! architecture files define them.

#![allow(
    clippy::all,
    clippy::pedantic,
    unused,
    non_snake_case,
    non_camel_case_types
)]
#![allow(non_upper_case_globals)]

pub type F32 = f32;
pub type F64 = f64;
pub type FaustFloat = F32;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParamIndex(pub i32);

pub trait FaustDsp {
    type T;

    fn new() -> Self
    where
        Self: Sized;
    fn metadata(&self, m: &mut dyn Meta);
    fn get_sample_rate(&self) -> i32;
    fn get_num_inputs(&self) -> i32;
    fn get_num_outputs(&self) -> i32;
    fn class_init(sample_rate: i32)
    where
        Self: Sized;
    fn instance_reset_params(&mut self);
    fn instance_clear(&mut self);
    fn instance_constants(&mut self, sample_rate: i32);
    fn instance_init(&mut self, sample_rate: i32);
    fn init(&mut self, sample_rate: i32);
    fn build_user_interface(&self, ui_interface: &mut dyn UI<Self::T>);
    fn build_user_interface_static(ui_interface: &mut dyn UI<Self::T>)
    where
        Self: Sized;
    fn get_param(&self, param: ParamIndex) -> Option<Self::T>;
    fn set_param(&mut self, param: ParamIndex, value: Self::T);
    fn compute(&mut self, count: i32, inputs: &[&[Self::T]], outputs: &mut [&mut [Self::T]]);
}

pub trait Meta {
    fn declare(&mut self, key: &str, value: &str);
}

pub trait UI<T> {
    fn open_tab_box(&mut self, label: &str);
    fn open_horizontal_box(&mut self, label: &str);
    fn open_vertical_box(&mut self, label: &str);
    fn close_box(&mut self);
    fn add_button(&mut self, label: &str, param: ParamIndex);
    fn add_check_button(&mut self, label: &str, param: ParamIndex);
    fn add_vertical_slider(
        &mut self,
        label: &str,
        param: ParamIndex,
        init: T,
        min: T,
        max: T,
        step: T,
    );
    fn add_horizontal_slider(
        &mut self,
        label: &str,
        param: ParamIndex,
        init: T,
        min: T,
        max: T,
        step: T,
    );
    fn add_num_entry(&mut self, label: &str, param: ParamIndex, init: T, min: T, max: T, step: T);
    fn add_horizontal_bargraph(&mut self, label: &str, param: ParamIndex, min: T, max: T);
    fn add_vertical_bargraph(&mut self, label: &str, param: ParamIndex, min: T, max: T);
    fn declare(&mut self, param: Option<ParamIndex>, key: &str, value: &str);
}

pub mod comp {
    use super::*;
    include!("comp.rs");
}

pub mod sat {
    use super::*;
    include!("sat.rs");
}

/// Collects a DSP's controls by label.
#[derive(Default)]
struct Controls(Vec<(String, ParamIndex)>);

impl UI<f32> for Controls {
    fn open_tab_box(&mut self, _: &str) {}
    fn open_horizontal_box(&mut self, _: &str) {}
    fn open_vertical_box(&mut self, _: &str) {}
    fn close_box(&mut self) {}
    fn add_button(&mut self, label: &str, param: ParamIndex) {
        self.0.push((label.into(), param));
    }
    fn add_check_button(&mut self, label: &str, param: ParamIndex) {
        self.0.push((label.into(), param));
    }
    fn add_vertical_slider(&mut self, label: &str, p: ParamIndex, _: f32, _: f32, _: f32, _: f32) {
        self.0.push((label.into(), p));
    }
    fn add_horizontal_slider(
        &mut self,
        label: &str,
        p: ParamIndex,
        _: f32,
        _: f32,
        _: f32,
        _: f32,
    ) {
        self.0.push((label.into(), p));
    }
    fn add_num_entry(&mut self, label: &str, p: ParamIndex, _: f32, _: f32, _: f32, _: f32) {
        self.0.push((label.into(), p));
    }
    fn add_horizontal_bargraph(&mut self, _: &str, _: ParamIndex, _: f32, _: f32) {}
    fn add_vertical_bargraph(&mut self, _: &str, _: ParamIndex, _: f32, _: f32) {}
    fn declare(&mut self, _: Option<ParamIndex>, _: &str, _: &str) {}
}

/// The indices of a DSP's controls, in the order of `labels`. Panics on a label it hasn't got:
/// the .dsp and its caller disagree.
pub fn controls<const N: usize>(dsp: &dyn FaustDsp<T = f32>, labels: [&str; N]) -> [ParamIndex; N] {
    let mut found = Controls::default();
    dsp.build_user_interface(&mut found);
    labels.map(|label| {
        found
            .0
            .iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("the Faust DSP has no control {label:?}"))
            .1
    })
}

#[cfg(test)]
mod tests;
