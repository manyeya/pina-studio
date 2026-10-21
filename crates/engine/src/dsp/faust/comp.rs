/* ------------------------------------------------------------
name: "Pina Studio Compressor"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -ct 1 -cn CompDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2
------------------------------------------------------------ */

#[repr(C)]
pub struct CompDsp {
	fHslider0: FaustFloat,
	fCheckbox0: FaustFloat,
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fConst2: F32,
	fRec1: [F32;2],
	fHslider1: FaustFloat,
	fHslider2: FaustFloat,
	fConst3: F32,
	fHslider3: FaustFloat,
	fRec0: [F32;2],
	fRec3: [F32;2],
	fRec2: [F32;2],
	fConst4: F32,
	fHslider4: FaustFloat,
	fConst5: F32,
	fRec4: [F32;2],
}


fn CompDsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 3;
pub const FAUST_ACTIVES: usize = 6;
pub const FAUST_PASSIVES: usize = 0;

impl CompDsp {
		
	pub fn new() -> CompDsp { 
		CompDsp {
			fHslider0: 0.0,
			fCheckbox0: 0.0,
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fConst2: 0.0,
			fRec1: [0.0;2],
			fHslider1: 0.0,
			fHslider2: 0.0,
			fConst3: 0.0,
			fHslider3: 0.0,
			fRec0: [0.0;2],
			fRec3: [0.0;2],
			fRec2: [0.0;2],
			fConst4: 0.0,
			fHslider4: 0.0,
			fConst5: 0.0,
			fRec4: [0.0;2],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("analyzers.lib/ms_envelope_tau:author", r"Dario Sanfilippo and Julius O. Smith III");
		m.declare("analyzers.lib/ms_envelope_tau:copyright", r"Copyright (C) 2020 Dario Sanfilippo        <sanfilippo.dario@gmail.com> and         2003-2020 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("analyzers.lib/ms_envelope_tau:license", r"LicenseRef-STK-4.3");
		m.declare("analyzers.lib/name", r"Faust Analyzer Library");
		m.declare("analyzers.lib/rms_envelope_tau:author", r"Dario Sanfilippo and Julius O. Smith III");
		m.declare("analyzers.lib/rms_envelope_tau:copyright", r"Copyright (C) 2020 Dario Sanfilippo        <sanfilippo.dario@gmail.com> and         2003-2020 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("analyzers.lib/rms_envelope_tau:license", r"LicenseRef-STK-4.3");
		m.declare("analyzers.lib/version", r"1.4.0");
		m.declare("basics.lib/bypass1:author", r"Julius Smith");
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/parallelMin:author", r"Bart Brouns");
		m.declare("basics.lib/parallelMin:copyright", r"Copyright (c) 2020 Bart Brouns <bart@magnetophon.nl>");
		m.declare("basics.lib/parallelMin:licence", r"GPL-3.0-only");
		m.declare("basics.lib/parallelOp:author", r"Bart Brouns");
		m.declare("basics.lib/parallelOp:copyright", r"Copyright (c) 2020 Bart Brouns <bart@magnetophon.nl>");
		m.declare("basics.lib/parallelOp:licence", r"GPL-3.0-only");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -ct 1 -cn CompDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2");
		m.declare("compressors.lib/name", r"Faust Compressor Effect Library");
		m.declare("compressors.lib/peak_compression_gain_N_chan_db:author", r"Bart Brouns");
		m.declare("compressors.lib/peak_compression_gain_N_chan_db:license", r"GPL-3.0-only");
		m.declare("compressors.lib/peak_compression_gain_mono_db:author", r"Bart Brouns");
		m.declare("compressors.lib/peak_compression_gain_mono_db:license", r"GPL-3.0-only");
		m.declare("compressors.lib/version", r"1.6.0");
		m.declare("description", r"A soft-knee feed-forward compressor, both sides linked, peak or RMS");
		m.declare("filename", r"comp.dsp");
		m.declare("filters.lib/avg_tau:author", r"Dario Sanfilippo and Julius O. Smith III");
		m.declare("filters.lib/avg_tau:copyright", r"Copyright (C) 2020 Dario Sanfilippo       <sanfilippo.dario@gmail.com> and        2003-2020 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/avg_tau:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/lptN:author", r"Julius O. Smith III");
		m.declare("filters.lib/lptN:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/lptN:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/name", r"Faust Filters Library");
		m.declare("filters.lib/version", r"1.9.0");
		m.declare("interpolators.lib/interpolate_linear:author", r"Stéphane Letz");
		m.declare("interpolators.lib/interpolate_linear:licence", r"MIT");
		m.declare("interpolators.lib/name", r"Faust Interpolator Library");
		m.declare("interpolators.lib/version", r"1.6.0");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"Pina Studio Compressor");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("routes.lib/name", r"Faust Signal Routing Library");
		m.declare("routes.lib/version", r"1.4.0");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/onePoleSwitching:author", r"Jonatan Liljedahl, revised by Dario Sanfilippo");
		m.declare("signals.lib/onePoleSwitching:licence", r"LicenseRef-STK-4.3");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	
	pub fn class_init(sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (4.0) as FaustFloat;
		self.fCheckbox0 = (0.0) as FaustFloat;
		self.fHslider1 = (-18.0) as FaustFloat;
		self.fHslider2 = (1e+01) as FaustFloat;
		self.fHslider3 = (1.2e+02) as FaustFloat;
		self.fHslider4 = (0.0) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.fRec1[l0 as usize] = 0.0;
		}
		for l1 in 0..2 {
			self.fRec0[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec3[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec2[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fRec4[l4 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = F32::exp(-(66.666664 / self.fConst0));
		self.fConst2 = 1.0 - self.fConst1;
		self.fConst3 = 1.0 / self.fConst0;
		self.fConst4 = 44.1 / self.fConst0;
		self.fConst5 = 1.0 - self.fConst4;
	}
	pub fn instance_init(&mut self, sample_rate: i32) {
		self.instance_constants(sample_rate);
		self.instance_reset_params();
		self.instance_clear();
	}
	pub fn init(&mut self, sample_rate: i32) {
		CompDsp::class_init(sample_rate);
		self.instance_init(sample_rate);
	}
	
	pub fn build_user_interface(&self, ui_interface: &mut dyn UI<FaustFloat>) {
		Self::build_user_interface_static(ui_interface);
	}
	
	pub fn build_user_interface_static(ui_interface: &mut dyn UI<FaustFloat>) {
		ui_interface.open_vertical_box("Pina Studio Compressor");
		ui_interface.add_horizontal_slider("attack", ParamIndex(0), 1e+01, 0.05, 1e+03, 0.01);
		ui_interface.add_horizontal_slider("makeup", ParamIndex(1), 0.0, -24.0, 24.0, 0.01);
		ui_interface.add_horizontal_slider("ratio", ParamIndex(2), 4.0, 1.0, 2e+01, 0.01);
		ui_interface.add_horizontal_slider("release", ParamIndex(3), 1.2e+02, 1.0, 5e+03, 0.01);
		ui_interface.add_check_button("rms", ParamIndex(4));
		ui_interface.add_horizontal_slider("threshold", ParamIndex(5), -18.0, -6e+01, 0.0, 0.01);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			4 => Some(self.fCheckbox0),
			2 => Some(self.fHslider0),
			5 => Some(self.fHslider1),
			0 => Some(self.fHslider2),
			3 => Some(self.fHslider3),
			1 => Some(self.fHslider4),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			4 => { self.fCheckbox0 = value }
			2 => { self.fHslider0 = value }
			5 => { self.fHslider1 = value }
			0 => { self.fHslider2 = value }
			3 => { self.fHslider3 = value }
			1 => { self.fHslider4 = value }
			_ => {}
		}
	}
	
	pub fn compute(
		&mut self,
		count: usize,
		inputs: &[impl AsRef<[FaustFloat]>],
		outputs: &mut[impl AsMut<[FaustFloat]>],
	) {
		
		// Obtaining locks on 0 static var(s)
		let [inputs0, inputs1, .. ] = inputs.as_ref() else { panic!("wrong number of input buffers"); };
		let inputs0 = inputs0.as_ref()[..count].iter();
		let inputs1 = inputs1.as_ref()[..count].iter();
		let [outputs0, outputs1, outputs2, .. ] = outputs.as_mut() else { panic!("wrong number of output buffers"); };
		let outputs0 = outputs0.as_mut()[..count].iter_mut();
		let outputs1 = outputs1.as_mut()[..count].iter_mut();
		let outputs2 = outputs2.as_mut()[..count].iter_mut();
		let mut fSlow0: F32 = 1.0 - 1.0 / F32::max(1.0, (self.fHslider0) as F32);
		let mut iSlow1: i32 = (self.fCheckbox0) as F32 as i32;
		let mut fSlow2: F32 = (self.fHslider1) as F32;
		let mut fSlow3: F32 = fSlow2 + -3.0;
		let mut fSlow4: F32 = fSlow2 + 3.0;
		let mut fSlow5: F32 = 3.0 - fSlow2;
		let mut fSlow6: F32 = 0.001 * (self.fHslider2) as F32;
		let mut iSlow7: i32 = (F32::abs(fSlow6) < 1.1920929e-07) as i32;
		let mut fSlow8: F32 = (if iSlow7 != 0 {0.0} else {F32::exp(-(self.fConst3 / (if iSlow7 != 0 {1.0} else {fSlow6})))});
		let mut fSlow9: F32 = 0.001 * (self.fHslider3) as F32;
		let mut iSlow10: i32 = (F32::abs(fSlow9) < 1.1920929e-07) as i32;
		let mut fSlow11: F32 = (if iSlow10 != 0 {0.0} else {F32::exp(-(self.fConst3 / (if iSlow10 != 0 {1.0} else {fSlow9})))});
		let mut fSlow12: F32 = self.fConst4 * (self.fHslider4) as F32;
		let zipped_iterators = inputs0.zip(inputs1).zip(outputs0).zip(outputs1).zip(outputs2);
		for ((((input0, input1), output0), output1), output2) in zipped_iterators {
			let mut fTemp0: F32 = (*input0) as F32;
			let mut fTemp1: F32 = self.fConst2 * CompDsp_faustpower2_f(fTemp0) + self.fConst1 * self.fRec1[1];
			self.fRec1[0] = (if (fTemp1.to_bits().cast_signed()) & 2139095040 != 0 {fTemp1} else {0.0});
			let mut fTemp2: F32 = 2e+01 * F32::log10(F32::max(1.1754944e-38, F32::abs((if iSlow1 != 0 {F32::sqrt(self.fRec1[0])} else {fTemp0}))));
			let mut iTemp3: i32 = ((fTemp2 > fSlow3) as i32) + ((fTemp2 > fSlow4) as i32);
			let mut fTemp4: F32 = F32::max(0.0, (if (iTemp3 == 0) as i32 != 0 {0.0} else {(if (iTemp3 == 1) as i32 != 0 {0.083333336 * CompDsp_faustpower2_f(fSlow5 + fTemp2)} else {fTemp2 - fSlow2})}));
			let mut fTemp5: F32 = (if (-(fSlow0 * fTemp4) > self.fRec0[1]) as i32 != 0 {fSlow11} else {fSlow8});
			let mut fTemp6: F32 = self.fRec0[1] * fTemp5 - fSlow0 * fTemp4 * (1.0 - fTemp5);
			self.fRec0[0] = (if (fTemp6.to_bits().cast_signed()) & 2139095040 != 0 {fTemp6} else {0.0});
			let mut fTemp7: F32 = (*input1) as F32;
			let mut fTemp8: F32 = self.fConst2 * CompDsp_faustpower2_f(fTemp7) + self.fConst1 * self.fRec3[1];
			self.fRec3[0] = (if (fTemp8.to_bits().cast_signed()) & 2139095040 != 0 {fTemp8} else {0.0});
			let mut fTemp9: F32 = 2e+01 * F32::log10(F32::max(1.1754944e-38, F32::abs((if iSlow1 != 0 {F32::sqrt(self.fRec3[0])} else {fTemp7}))));
			let mut iTemp10: i32 = ((fTemp9 > fSlow3) as i32) + ((fTemp9 > fSlow4) as i32);
			let mut fTemp11: F32 = F32::max(0.0, (if (iTemp10 == 0) as i32 != 0 {0.0} else {(if (iTemp10 == 1) as i32 != 0 {0.083333336 * CompDsp_faustpower2_f(fSlow5 + fTemp9)} else {fTemp9 - fSlow2})}));
			let mut fTemp12: F32 = (if (-(fSlow0 * fTemp11) > self.fRec2[1]) as i32 != 0 {fSlow11} else {fSlow8});
			let mut fTemp13: F32 = self.fRec2[1] * fTemp12 - fSlow0 * fTemp11 * (1.0 - fTemp12);
			self.fRec2[0] = (if (fTemp13.to_bits().cast_signed()) & 2139095040 != 0 {fTemp13} else {0.0});
			let mut fTemp14: F32 = F32::min(self.fRec0[0], self.fRec2[0]);
			let mut fTemp15: F32 = fSlow12 + self.fConst5 * self.fRec4[1];
			self.fRec4[0] = (if (fTemp15.to_bits().cast_signed()) & 2139095040 != 0 {fTemp15} else {0.0});
			let mut fTemp16: F32 = F32::powf(1e+01, 0.05 * (fTemp14 + self.fRec4[0]));
			*output0 = (fTemp0 * fTemp16) as FaustFloat;
			*output1 = (fTemp7 * fTemp16) as FaustFloat;
			*output2 = (-(F32::min(fTemp14, fTemp14))) as FaustFloat;
			self.fRec1[1] = self.fRec1[0];
			self.fRec0[1] = self.fRec0[0];
			self.fRec3[1] = self.fRec3[0];
			self.fRec2[1] = self.fRec2[0];
			self.fRec4[1] = self.fRec4[0];
		}
		
	}

}

#[cfg(not(target_arch = "wasm32"))] // Compile ffi bindings only on non-wasm targets
mod ffi {
	use core::ffi::c_float;
	// Conditionally compile the link attribute only on non-Windows platforms
	#[cfg_attr(not(target_os = "windows"), link(name = "m"))]
	unsafe extern "C" {
		pub fn remainderf(from: c_float, to: c_float) -> c_float;
		pub fn rintf(val: c_float) -> c_float;
	}
}
fn remainderf(from: f32, to: f32) -> f32 {
	#[cfg(not(target_arch = "wasm32"))] // non-wasm targets use ffi bindings
	unsafe { ffi::remainderf(from, to) }
	#[cfg(target_arch = "wasm32")] // wasm relies on libm
	libm::remainderf(from, to)
}
fn rintf(val: f32) -> f32 {
	#[cfg(not(target_arch = "wasm32"))] // non-wasm targets use ffi bindings
	unsafe { ffi::rintf(val) }
	#[cfg(target_arch = "wasm32")] // wasm relies on libm
	libm::rintf(val)
}

impl FaustDsp for CompDsp {
	type T = FaustFloat;
	fn new() -> Self where Self: Sized {
		Self::new()
	}
	fn metadata(&self, m: &mut dyn Meta) {
		self.metadata(m)
	}
	fn get_sample_rate(&self) -> i32 {
		self.get_sample_rate()
	}
	fn get_num_inputs(&self) -> i32 {
		FAUST_INPUTS as i32
	}
	fn get_num_outputs(&self) -> i32 {
		FAUST_OUTPUTS as i32
	}
	fn class_init(sample_rate: i32) where Self: Sized {
		Self::class_init(sample_rate);
	}
	fn instance_reset_params(&mut self) {
		self.instance_reset_params()
	}
	fn instance_clear(&mut self) {
		self.instance_clear()
	}
	fn instance_constants(&mut self, sample_rate: i32) {
		self.instance_constants(sample_rate)
	}
	fn instance_init(&mut self, sample_rate: i32) {
		self.instance_init(sample_rate)
	}
	fn init(&mut self, sample_rate: i32) {
		self.init(sample_rate)
	}
	fn build_user_interface(&self, ui_interface: &mut dyn UI<Self::T>) {
		self.build_user_interface(ui_interface)
	}
	fn build_user_interface_static(ui_interface: &mut dyn UI<Self::T>) where Self: Sized {
		Self::build_user_interface_static(ui_interface);
	}
	fn get_param(&self, param: ParamIndex) -> Option<Self::T> {
		self.get_param(param)
	}
	fn set_param(&mut self, param: ParamIndex, value: Self::T) {
		self.set_param(param, value)
	}
	fn compute(&mut self, count: i32, inputs: &[&[Self::T]], outputs: &mut [&mut [Self::T]]) {
		self.compute(count as usize, inputs, outputs)
	}
}
