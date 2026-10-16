/* ------------------------------------------------------------
name: "tempo Saturator"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -ct 1 -cn SatDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2
------------------------------------------------------------ */

#[repr(C)]
pub struct SatDsp {
	fVec0: [F32;2],
	fRec7: [F32;2],
	fRec6: [F32;2],
	fRec5: [F32;2],
	fRec4: [F32;2],
	fRec3: [F32;2],
	fRec2: [F32;2],
	fRec1: [F32;2],
	fRec0: [F32;2],
	fRec15: [F32;2],
	fRec14: [F32;2],
	fRec13: [F32;2],
	fRec12: [F32;2],
	fRec11: [F32;2],
	fRec10: [F32;2],
	fRec9: [F32;2],
	fRec8: [F32;2],
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fConst2: F32,
	fConst3: F32,
	fConst4: F32,
	fHslider0: FaustFloat,
	fConst5: F32,
	fRec22: [F32;2],
	fVec1: [F32;2],
	fVec2: [F32;2],
	fConst6: F32,
	fConst7: F32,
	fVec3: [F32;2],
	fRec21: [F32;2],
	fRec20: [F32;2],
	fRec19: [F32;2],
	fRec18: [F32;2],
	fVec4: [F32;2],
	fRec26: [F32;2],
	fRec25: [F32;2],
	fRec24: [F32;2],
	fRec23: [F32;2],
	fVec5: [F32;2],
	fRec17: [F32;2],
	fRec16: [F32;2],
	fHslider1: FaustFloat,
	fRec27: [F32;2],
	fHslider2: FaustFloat,
	fRec28: [F32;2],
	fHslider3: FaustFloat,
	fRec29: [F32;2],
	fVec6: [F32;2],
	fRec37: [F32;2],
	fRec36: [F32;2],
	fRec35: [F32;2],
	fRec34: [F32;2],
	fRec33: [F32;2],
	fRec32: [F32;2],
	fRec31: [F32;2],
	fRec30: [F32;2],
	fRec45: [F32;2],
	fRec44: [F32;2],
	fRec43: [F32;2],
	fRec42: [F32;2],
	fRec41: [F32;2],
	fRec40: [F32;2],
	fRec39: [F32;2],
	fRec38: [F32;2],
	fVec7: [F32;2],
	fVec8: [F32;2],
	fVec9: [F32;2],
	fRec51: [F32;2],
	fRec50: [F32;2],
	fRec49: [F32;2],
	fRec48: [F32;2],
	fVec10: [F32;2],
	fRec55: [F32;2],
	fRec54: [F32;2],
	fRec53: [F32;2],
	fRec52: [F32;2],
	fVec11: [F32;2],
	fRec47: [F32;2],
	fRec46: [F32;2],
}


fn SatDsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 4;
pub const FAUST_PASSIVES: usize = 0;

impl SatDsp {
		
	pub fn new() -> SatDsp { 
		SatDsp {
			fVec0: [0.0;2],
			fRec7: [0.0;2],
			fRec6: [0.0;2],
			fRec5: [0.0;2],
			fRec4: [0.0;2],
			fRec3: [0.0;2],
			fRec2: [0.0;2],
			fRec1: [0.0;2],
			fRec0: [0.0;2],
			fRec15: [0.0;2],
			fRec14: [0.0;2],
			fRec13: [0.0;2],
			fRec12: [0.0;2],
			fRec11: [0.0;2],
			fRec10: [0.0;2],
			fRec9: [0.0;2],
			fRec8: [0.0;2],
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fConst2: 0.0,
			fConst3: 0.0,
			fConst4: 0.0,
			fHslider0: 0.0,
			fConst5: 0.0,
			fRec22: [0.0;2],
			fVec1: [0.0;2],
			fVec2: [0.0;2],
			fConst6: 0.0,
			fConst7: 0.0,
			fVec3: [0.0;2],
			fRec21: [0.0;2],
			fRec20: [0.0;2],
			fRec19: [0.0;2],
			fRec18: [0.0;2],
			fVec4: [0.0;2],
			fRec26: [0.0;2],
			fRec25: [0.0;2],
			fRec24: [0.0;2],
			fRec23: [0.0;2],
			fVec5: [0.0;2],
			fRec17: [0.0;2],
			fRec16: [0.0;2],
			fHslider1: 0.0,
			fRec27: [0.0;2],
			fHslider2: 0.0,
			fRec28: [0.0;2],
			fHslider3: 0.0,
			fRec29: [0.0;2],
			fVec6: [0.0;2],
			fRec37: [0.0;2],
			fRec36: [0.0;2],
			fRec35: [0.0;2],
			fRec34: [0.0;2],
			fRec33: [0.0;2],
			fRec32: [0.0;2],
			fRec31: [0.0;2],
			fRec30: [0.0;2],
			fRec45: [0.0;2],
			fRec44: [0.0;2],
			fRec43: [0.0;2],
			fRec42: [0.0;2],
			fRec41: [0.0;2],
			fRec40: [0.0;2],
			fRec39: [0.0;2],
			fRec38: [0.0;2],
			fVec7: [0.0;2],
			fVec8: [0.0;2],
			fVec9: [0.0;2],
			fRec51: [0.0;2],
			fRec50: [0.0;2],
			fRec49: [0.0;2],
			fRec48: [0.0;2],
			fVec10: [0.0;2],
			fRec55: [0.0;2],
			fRec54: [0.0;2],
			fRec53: [0.0;2],
			fRec52: [0.0;2],
			fVec11: [0.0;2],
			fRec47: [0.0;2],
			fRec46: [0.0;2],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -ct 1 -cn SatDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2");
		m.declare("description", r"Drive into a soft clip, as tape or a tube would: 2x oversampled and antialiased");
		m.declare("filename", r"sat.dsp");
		m.declare("filters.lib/dcblocker:author", r"Julius O. Smith III");
		m.declare("filters.lib/dcblocker:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/dcblocker:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/lowpass0_highpass1:author", r"Julius O. Smith III");
		m.declare("filters.lib/lowpass0_highpass1:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/lowpass0_highpass1:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/lowpass:author", r"Julius O. Smith III");
		m.declare("filters.lib/lowpass:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/lowpass:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/name", r"Faust Filters Library");
		m.declare("filters.lib/pole:author", r"Julius O. Smith III");
		m.declare("filters.lib/pole:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/pole:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/tf1:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf1:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf1:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/tf1s:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf1s:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf1s:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/version", r"1.9.0");
		m.declare("filters.lib/zero:author", r"Julius O. Smith III");
		m.declare("filters.lib/zero:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/zero:license", r"LicenseRef-STK-4.3");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"tempo Saturator");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	
	pub fn class_init(sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (6.0) as FaustFloat;
		self.fHslider1 = (0.0) as FaustFloat;
		self.fHslider2 = (1.0) as FaustFloat;
		self.fHslider3 = (0.0) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.fVec0[l0 as usize] = 0.0;
		}
		for l1 in 0..2 {
			self.fRec7[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec6[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec5[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fRec4[l4 as usize] = 0.0;
		}
		for l5 in 0..2 {
			self.fRec3[l5 as usize] = 0.0;
		}
		for l6 in 0..2 {
			self.fRec2[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec1[l7 as usize] = 0.0;
		}
		for l8 in 0..2 {
			self.fRec0[l8 as usize] = 0.0;
		}
		for l9 in 0..2 {
			self.fRec15[l9 as usize] = 0.0;
		}
		for l10 in 0..2 {
			self.fRec14[l10 as usize] = 0.0;
		}
		for l11 in 0..2 {
			self.fRec13[l11 as usize] = 0.0;
		}
		for l12 in 0..2 {
			self.fRec12[l12 as usize] = 0.0;
		}
		for l13 in 0..2 {
			self.fRec11[l13 as usize] = 0.0;
		}
		for l14 in 0..2 {
			self.fRec10[l14 as usize] = 0.0;
		}
		for l15 in 0..2 {
			self.fRec9[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fRec8[l16 as usize] = 0.0;
		}
		for l17 in 0..2 {
			self.fRec22[l17 as usize] = 0.0;
		}
		for l18 in 0..2 {
			self.fVec1[l18 as usize] = 0.0;
		}
		for l19 in 0..2 {
			self.fVec2[l19 as usize] = 0.0;
		}
		for l20 in 0..2 {
			self.fVec3[l20 as usize] = 0.0;
		}
		for l21 in 0..2 {
			self.fRec21[l21 as usize] = 0.0;
		}
		for l22 in 0..2 {
			self.fRec20[l22 as usize] = 0.0;
		}
		for l23 in 0..2 {
			self.fRec19[l23 as usize] = 0.0;
		}
		for l24 in 0..2 {
			self.fRec18[l24 as usize] = 0.0;
		}
		for l25 in 0..2 {
			self.fVec4[l25 as usize] = 0.0;
		}
		for l26 in 0..2 {
			self.fRec26[l26 as usize] = 0.0;
		}
		for l27 in 0..2 {
			self.fRec25[l27 as usize] = 0.0;
		}
		for l28 in 0..2 {
			self.fRec24[l28 as usize] = 0.0;
		}
		for l29 in 0..2 {
			self.fRec23[l29 as usize] = 0.0;
		}
		for l30 in 0..2 {
			self.fVec5[l30 as usize] = 0.0;
		}
		for l31 in 0..2 {
			self.fRec17[l31 as usize] = 0.0;
		}
		for l32 in 0..2 {
			self.fRec16[l32 as usize] = 0.0;
		}
		for l33 in 0..2 {
			self.fRec27[l33 as usize] = 0.0;
		}
		for l34 in 0..2 {
			self.fRec28[l34 as usize] = 0.0;
		}
		for l35 in 0..2 {
			self.fRec29[l35 as usize] = 0.0;
		}
		for l36 in 0..2 {
			self.fVec6[l36 as usize] = 0.0;
		}
		for l37 in 0..2 {
			self.fRec37[l37 as usize] = 0.0;
		}
		for l38 in 0..2 {
			self.fRec36[l38 as usize] = 0.0;
		}
		for l39 in 0..2 {
			self.fRec35[l39 as usize] = 0.0;
		}
		for l40 in 0..2 {
			self.fRec34[l40 as usize] = 0.0;
		}
		for l41 in 0..2 {
			self.fRec33[l41 as usize] = 0.0;
		}
		for l42 in 0..2 {
			self.fRec32[l42 as usize] = 0.0;
		}
		for l43 in 0..2 {
			self.fRec31[l43 as usize] = 0.0;
		}
		for l44 in 0..2 {
			self.fRec30[l44 as usize] = 0.0;
		}
		for l45 in 0..2 {
			self.fRec45[l45 as usize] = 0.0;
		}
		for l46 in 0..2 {
			self.fRec44[l46 as usize] = 0.0;
		}
		for l47 in 0..2 {
			self.fRec43[l47 as usize] = 0.0;
		}
		for l48 in 0..2 {
			self.fRec42[l48 as usize] = 0.0;
		}
		for l49 in 0..2 {
			self.fRec41[l49 as usize] = 0.0;
		}
		for l50 in 0..2 {
			self.fRec40[l50 as usize] = 0.0;
		}
		for l51 in 0..2 {
			self.fRec39[l51 as usize] = 0.0;
		}
		for l52 in 0..2 {
			self.fRec38[l52 as usize] = 0.0;
		}
		for l53 in 0..2 {
			self.fVec7[l53 as usize] = 0.0;
		}
		for l54 in 0..2 {
			self.fVec8[l54 as usize] = 0.0;
		}
		for l55 in 0..2 {
			self.fVec9[l55 as usize] = 0.0;
		}
		for l56 in 0..2 {
			self.fRec51[l56 as usize] = 0.0;
		}
		for l57 in 0..2 {
			self.fRec50[l57 as usize] = 0.0;
		}
		for l58 in 0..2 {
			self.fRec49[l58 as usize] = 0.0;
		}
		for l59 in 0..2 {
			self.fRec48[l59 as usize] = 0.0;
		}
		for l60 in 0..2 {
			self.fVec10[l60 as usize] = 0.0;
		}
		for l61 in 0..2 {
			self.fRec55[l61 as usize] = 0.0;
		}
		for l62 in 0..2 {
			self.fRec54[l62 as usize] = 0.0;
		}
		for l63 in 0..2 {
			self.fRec53[l63 as usize] = 0.0;
		}
		for l64 in 0..2 {
			self.fRec52[l64 as usize] = 0.0;
		}
		for l65 in 0..2 {
			self.fVec11[l65 as usize] = 0.0;
		}
		for l66 in 0..2 {
			self.fRec47[l66 as usize] = 0.0;
		}
		for l67 in 0..2 {
			self.fRec46[l67 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 1.0 / F32::tan(4712.389 / self.fConst0);
		self.fConst2 = 1.0 / (self.fConst1 + 1.0);
		self.fConst3 = 1.0 - self.fConst1;
		self.fConst4 = 44.1 / self.fConst0;
		self.fConst5 = 1.0 - self.fConst4;
		self.fConst6 = F32::tanh(0.15);
		self.fConst7 = 1.0 - SatDsp_faustpower2_f(self.fConst6);
	}
	pub fn instance_init(&mut self, sample_rate: i32) {
		self.instance_constants(sample_rate);
		self.instance_reset_params();
		self.instance_clear();
	}
	pub fn init(&mut self, sample_rate: i32) {
		SatDsp::class_init(sample_rate);
		self.instance_init(sample_rate);
	}
	
	pub fn build_user_interface(&self, ui_interface: &mut dyn UI<FaustFloat>) {
		Self::build_user_interface_static(ui_interface);
	}
	
	pub fn build_user_interface_static(ui_interface: &mut dyn UI<FaustFloat>) {
		ui_interface.open_vertical_box("tempo Saturator");
		ui_interface.add_horizontal_slider("drive", ParamIndex(0), 6.0, 0.0, 24.0, 0.01);
		ui_interface.add_horizontal_slider("mix", ParamIndex(1), 1.0, 0.0, 1.0, 0.01);
		ui_interface.add_horizontal_slider("output", ParamIndex(2), 0.0, -24.0, 24.0, 0.01);
		ui_interface.add_horizontal_slider("tone", ParamIndex(3), 0.0, -1.0, 1.0, 0.01);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			0 => Some(self.fHslider0),
			3 => Some(self.fHslider1),
			1 => Some(self.fHslider2),
			2 => Some(self.fHslider3),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			0 => { self.fHslider0 = value }
			3 => { self.fHslider1 = value }
			1 => { self.fHslider2 = value }
			2 => { self.fHslider3 = value }
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
		let [outputs0, outputs1, .. ] = outputs.as_mut() else { panic!("wrong number of output buffers"); };
		let outputs0 = outputs0.as_mut()[..count].iter_mut();
		let outputs1 = outputs1.as_mut()[..count].iter_mut();
		let mut fSlow0: F32 = self.fConst4 * (self.fHslider0) as F32;
		let mut fSlow1: F32 = self.fConst4 * (self.fHslider1) as F32;
		let mut fSlow2: F32 = self.fConst4 * (self.fHslider2) as F32;
		let mut fSlow3: F32 = self.fConst4 * (self.fHslider3) as F32;
		let zipped_iterators = inputs0.zip(inputs1).zip(outputs0).zip(outputs1);
		for (((input0, input1), output0), output1) in zipped_iterators {
			let mut fTemp0: F32 = (*input0) as F32;
			self.fVec0[0] = fTemp0;
			let mut fTemp1: F32 = self.fVec0[1] + 0.040633462 * (fTemp0 - self.fRec7[1]);
			self.fRec7[0] = (if (fTemp1.to_bits().cast_signed()) & 2139095040 != 0 {fTemp1} else {0.0});
			let mut fTemp2: F32 = self.fRec7[1] + 0.30075705 * (self.fRec7[0] - self.fRec6[1]);
			self.fRec6[0] = (if (fTemp2.to_bits().cast_signed()) & 2139095040 != 0 {fTemp2} else {0.0});
			let mut fTemp3: F32 = self.fRec6[1] + 0.6095243 * (self.fRec6[0] - self.fRec5[1]);
			self.fRec5[0] = (if (fTemp3.to_bits().cast_signed()) & 2139095040 != 0 {fTemp3} else {0.0});
			let mut fTemp4: F32 = self.fRec5[1] + 0.8492238 * (self.fRec5[0] - self.fRec4[1]);
			self.fRec4[0] = (if (fTemp4.to_bits().cast_signed()) & 2139095040 != 0 {fTemp4} else {0.0});
			let mut fTemp5: F32 = self.fRec4[1] + 0.040633462 * (self.fRec4[0] - self.fRec3[1]);
			self.fRec3[0] = (if (fTemp5.to_bits().cast_signed()) & 2139095040 != 0 {fTemp5} else {0.0});
			let mut fTemp6: F32 = self.fRec3[1] + 0.30075705 * (self.fRec3[0] - self.fRec2[1]);
			self.fRec2[0] = (if (fTemp6.to_bits().cast_signed()) & 2139095040 != 0 {fTemp6} else {0.0});
			let mut fTemp7: F32 = self.fRec2[1] + 0.6095243 * (self.fRec2[0] - self.fRec1[1]);
			self.fRec1[0] = (if (fTemp7.to_bits().cast_signed()) & 2139095040 != 0 {fTemp7} else {0.0});
			let mut fTemp8: F32 = self.fRec1[1] + 0.8492238 * (self.fRec1[0] - self.fRec0[1]);
			self.fRec0[0] = (if (fTemp8.to_bits().cast_signed()) & 2139095040 != 0 {fTemp8} else {0.0});
			let mut fTemp9: F32 = self.fVec0[1] + 0.15050513 * (fTemp0 - self.fRec15[1]);
			self.fRec15[0] = (if (fTemp9.to_bits().cast_signed()) & 2139095040 != 0 {fTemp9} else {0.0});
			let mut fTemp10: F32 = self.fRec15[1] + 0.4607745 * (self.fRec15[0] - self.fRec14[1]);
			self.fRec14[0] = (if (fTemp10.to_bits().cast_signed()) & 2139095040 != 0 {fTemp10} else {0.0});
			let mut fTemp11: F32 = self.fRec14[1] + 0.7385038 * (self.fRec14[0] - self.fRec13[1]);
			self.fRec13[0] = (if (fTemp11.to_bits().cast_signed()) & 2139095040 != 0 {fTemp11} else {0.0});
			let mut fTemp12: F32 = self.fRec13[1] + 0.9497428 * (self.fRec13[0] - self.fRec12[1]);
			self.fRec12[0] = (if (fTemp12.to_bits().cast_signed()) & 2139095040 != 0 {fTemp12} else {0.0});
			let mut fTemp13: F32 = self.fRec12[1] + 0.15050513 * (self.fRec12[0] - self.fRec11[1]);
			self.fRec11[0] = (if (fTemp13.to_bits().cast_signed()) & 2139095040 != 0 {fTemp13} else {0.0});
			let mut fTemp14: F32 = self.fRec11[1] + 0.4607745 * (self.fRec11[0] - self.fRec10[1]);
			self.fRec10[0] = (if (fTemp14.to_bits().cast_signed()) & 2139095040 != 0 {fTemp14} else {0.0});
			let mut fTemp15: F32 = self.fRec10[1] + 0.7385038 * (self.fRec10[0] - self.fRec9[1]);
			self.fRec9[0] = (if (fTemp15.to_bits().cast_signed()) & 2139095040 != 0 {fTemp15} else {0.0});
			let mut fTemp16: F32 = self.fRec9[1] + 0.9497428 * (self.fRec9[0] - self.fRec8[1]);
			self.fRec8[0] = (if (fTemp16.to_bits().cast_signed()) & 2139095040 != 0 {fTemp16} else {0.0});
			let mut fTemp17: F32 = 0.5 * (self.fRec0[0] + self.fRec8[1]);
			let mut fTemp18: F32 = fSlow0 + self.fConst5 * self.fRec22[1];
			self.fRec22[0] = (if (fTemp18.to_bits().cast_signed()) & 2139095040 != 0 {fTemp18} else {0.0});
			let mut fTemp19: F32 = F32::powf(1e+01, 0.05 * self.fRec22[0]);
			let mut fTemp20: F32 = 0.15 + self.fRec12[0] * fTemp19;
			self.fVec1[0] = fTemp20;
			let mut fTemp21: F32 = self.fRec4[0] * fTemp19;
			let mut fTemp22: F32 = 0.15 - self.fVec1[1] + fTemp21;
			let mut iTemp23: i32 = (F32::abs(fTemp22) < 0.001) as i32;
			let mut fTemp24: F32 = F32::abs(fTemp20);
			let mut fTemp25: F32 = fTemp24 + F32::log(1.0 + F32::exp(-(2.0 * fTemp24)), std::f32::consts::E);
			self.fVec2[0] = -0.6931472 + fTemp25;
			let mut fTemp26: F32 = F32::abs(0.15 + fTemp21);
			let mut fTemp27: F32 = fTemp26 + F32::log(1.0 + F32::exp(-(2.0 * fTemp26)), std::f32::consts::E);
			let mut fTemp28: F32 = self.fConst7 * fTemp19;
			let mut fTemp29: F32 = ((if iTemp23 != 0 {F32::tanh(0.5 * (0.15 + fTemp21 + self.fVec1[1]))} else {(-0.6931472 - self.fVec2[1] + fTemp27) / (if iTemp23 != 0 {1.0} else {fTemp22})}) - self.fConst6) / fTemp28;
			self.fVec3[0] = fTemp29;
			let mut fTemp30: F32 = self.fVec3[1] + 0.040633462 * (fTemp29 - self.fRec21[1]);
			self.fRec21[0] = (if (fTemp30.to_bits().cast_signed()) & 2139095040 != 0 {fTemp30} else {0.0});
			let mut fTemp31: F32 = self.fRec21[1] + 0.30075705 * (self.fRec21[0] - self.fRec20[1]);
			self.fRec20[0] = (if (fTemp31.to_bits().cast_signed()) & 2139095040 != 0 {fTemp31} else {0.0});
			let mut fTemp32: F32 = self.fRec20[1] + 0.6095243 * (self.fRec20[0] - self.fRec19[1]);
			self.fRec19[0] = (if (fTemp32.to_bits().cast_signed()) & 2139095040 != 0 {fTemp32} else {0.0});
			let mut fTemp33: F32 = self.fRec19[1] + 0.8492238 * (self.fRec19[0] - self.fRec18[1]);
			self.fRec18[0] = (if (fTemp33.to_bits().cast_signed()) & 2139095040 != 0 {fTemp33} else {0.0});
			let mut fTemp34: F32 = fTemp19 * (self.fRec4[0] - self.fRec12[0]);
			let mut iTemp35: i32 = (F32::abs(-fTemp34) < 0.001) as i32;
			let mut fTemp36: F32 = ((if iTemp35 != 0 {F32::tanh(0.5 * (0.3 + fTemp19 * (self.fRec4[0] + self.fRec12[0])))} else {(fTemp25 - fTemp27) / (if iTemp35 != 0 {1.0} else {-fTemp34})}) - self.fConst6) / fTemp28;
			self.fVec4[0] = fTemp36;
			let mut fTemp37: F32 = self.fVec4[1] + 0.15050513 * (fTemp36 - self.fRec26[1]);
			self.fRec26[0] = (if (fTemp37.to_bits().cast_signed()) & 2139095040 != 0 {fTemp37} else {0.0});
			let mut fTemp38: F32 = self.fRec26[1] + 0.4607745 * (self.fRec26[0] - self.fRec25[1]);
			self.fRec25[0] = (if (fTemp38.to_bits().cast_signed()) & 2139095040 != 0 {fTemp38} else {0.0});
			let mut fTemp39: F32 = self.fRec25[1] + 0.7385038 * (self.fRec25[0] - self.fRec24[1]);
			self.fRec24[0] = (if (fTemp39.to_bits().cast_signed()) & 2139095040 != 0 {fTemp39} else {0.0});
			let mut fTemp40: F32 = self.fRec24[1] + 0.9497428 * (self.fRec24[0] - self.fRec23[1]);
			self.fRec23[0] = (if (fTemp40.to_bits().cast_signed()) & 2139095040 != 0 {fTemp40} else {0.0});
			let mut fTemp41: F32 = self.fRec18[0] + self.fRec23[1];
			self.fVec5[0] = fTemp41;
			let mut fTemp42: F32 = 0.995 * self.fRec17[1] + 0.5 * (fTemp41 - self.fVec5[1]);
			self.fRec17[0] = (if (fTemp42.to_bits().cast_signed()) & 2139095040 != 0 {fTemp42} else {0.0});
			let mut fTemp43: F32 = self.fConst2 * (self.fConst3 * self.fRec16[1] - (self.fRec17[1] + self.fRec17[0]));
			self.fRec16[0] = (if (-fTemp43.to_bits().cast_signed()) & 2139095040 != 0 {-fTemp43} else {0.0});
			let mut fTemp44: F32 = fSlow1 + self.fConst5 * self.fRec27[1];
			self.fRec27[0] = (if (fTemp44.to_bits().cast_signed()) & 2139095040 != 0 {fTemp44} else {0.0});
			let mut fTemp45: F32 = 0.3 * self.fRec27[0];
			let mut fTemp46: F32 = F32::powf(1e+01, -fTemp45);
			let mut fTemp47: F32 = F32::powf(1e+01, fTemp45);
			let mut fTemp48: F32 = fSlow2 + self.fConst5 * self.fRec28[1];
			self.fRec28[0] = (if (fTemp48.to_bits().cast_signed()) & 2139095040 != 0 {fTemp48} else {0.0});
			let mut fTemp49: F32 = fSlow3 + self.fConst5 * self.fRec29[1];
			self.fRec29[0] = (if (fTemp49.to_bits().cast_signed()) & 2139095040 != 0 {fTemp49} else {0.0});
			let mut fTemp50: F32 = F32::powf(1e+01, 0.05 * self.fRec29[0]);
			*output0 = ((fTemp17 + (self.fRec16[0] * fTemp46 + (self.fRec17[0] - self.fRec16[0]) * fTemp47 - fTemp17) * self.fRec28[0]) * fTemp50) as FaustFloat;
			let mut fTemp51: F32 = (*input1) as F32;
			self.fVec6[0] = fTemp51;
			let mut fTemp52: F32 = self.fVec6[1] + 0.040633462 * (fTemp51 - self.fRec37[1]);
			self.fRec37[0] = (if (fTemp52.to_bits().cast_signed()) & 2139095040 != 0 {fTemp52} else {0.0});
			let mut fTemp53: F32 = self.fRec37[1] + 0.30075705 * (self.fRec37[0] - self.fRec36[1]);
			self.fRec36[0] = (if (fTemp53.to_bits().cast_signed()) & 2139095040 != 0 {fTemp53} else {0.0});
			let mut fTemp54: F32 = self.fRec36[1] + 0.6095243 * (self.fRec36[0] - self.fRec35[1]);
			self.fRec35[0] = (if (fTemp54.to_bits().cast_signed()) & 2139095040 != 0 {fTemp54} else {0.0});
			let mut fTemp55: F32 = self.fRec35[1] + 0.8492238 * (self.fRec35[0] - self.fRec34[1]);
			self.fRec34[0] = (if (fTemp55.to_bits().cast_signed()) & 2139095040 != 0 {fTemp55} else {0.0});
			let mut fTemp56: F32 = self.fRec34[1] + 0.040633462 * (self.fRec34[0] - self.fRec33[1]);
			self.fRec33[0] = (if (fTemp56.to_bits().cast_signed()) & 2139095040 != 0 {fTemp56} else {0.0});
			let mut fTemp57: F32 = self.fRec33[1] + 0.30075705 * (self.fRec33[0] - self.fRec32[1]);
			self.fRec32[0] = (if (fTemp57.to_bits().cast_signed()) & 2139095040 != 0 {fTemp57} else {0.0});
			let mut fTemp58: F32 = self.fRec32[1] + 0.6095243 * (self.fRec32[0] - self.fRec31[1]);
			self.fRec31[0] = (if (fTemp58.to_bits().cast_signed()) & 2139095040 != 0 {fTemp58} else {0.0});
			let mut fTemp59: F32 = self.fRec31[1] + 0.8492238 * (self.fRec31[0] - self.fRec30[1]);
			self.fRec30[0] = (if (fTemp59.to_bits().cast_signed()) & 2139095040 != 0 {fTemp59} else {0.0});
			let mut fTemp60: F32 = self.fVec6[1] + 0.15050513 * (fTemp51 - self.fRec45[1]);
			self.fRec45[0] = (if (fTemp60.to_bits().cast_signed()) & 2139095040 != 0 {fTemp60} else {0.0});
			let mut fTemp61: F32 = self.fRec45[1] + 0.4607745 * (self.fRec45[0] - self.fRec44[1]);
			self.fRec44[0] = (if (fTemp61.to_bits().cast_signed()) & 2139095040 != 0 {fTemp61} else {0.0});
			let mut fTemp62: F32 = self.fRec44[1] + 0.7385038 * (self.fRec44[0] - self.fRec43[1]);
			self.fRec43[0] = (if (fTemp62.to_bits().cast_signed()) & 2139095040 != 0 {fTemp62} else {0.0});
			let mut fTemp63: F32 = self.fRec43[1] + 0.9497428 * (self.fRec43[0] - self.fRec42[1]);
			self.fRec42[0] = (if (fTemp63.to_bits().cast_signed()) & 2139095040 != 0 {fTemp63} else {0.0});
			let mut fTemp64: F32 = self.fRec42[1] + 0.15050513 * (self.fRec42[0] - self.fRec41[1]);
			self.fRec41[0] = (if (fTemp64.to_bits().cast_signed()) & 2139095040 != 0 {fTemp64} else {0.0});
			let mut fTemp65: F32 = self.fRec41[1] + 0.4607745 * (self.fRec41[0] - self.fRec40[1]);
			self.fRec40[0] = (if (fTemp65.to_bits().cast_signed()) & 2139095040 != 0 {fTemp65} else {0.0});
			let mut fTemp66: F32 = self.fRec40[1] + 0.7385038 * (self.fRec40[0] - self.fRec39[1]);
			self.fRec39[0] = (if (fTemp66.to_bits().cast_signed()) & 2139095040 != 0 {fTemp66} else {0.0});
			let mut fTemp67: F32 = self.fRec39[1] + 0.9497428 * (self.fRec39[0] - self.fRec38[1]);
			self.fRec38[0] = (if (fTemp67.to_bits().cast_signed()) & 2139095040 != 0 {fTemp67} else {0.0});
			let mut fTemp68: F32 = 0.5 * (self.fRec30[0] + self.fRec38[1]);
			let mut fTemp69: F32 = 0.15 + fTemp19 * self.fRec42[0];
			self.fVec7[0] = fTemp69;
			let mut fTemp70: F32 = fTemp19 * self.fRec34[0];
			let mut fTemp71: F32 = 0.15 - self.fVec7[1] + fTemp70;
			let mut iTemp72: i32 = (F32::abs(fTemp71) < 0.001) as i32;
			let mut fTemp73: F32 = F32::abs(fTemp69);
			let mut fTemp74: F32 = fTemp73 + F32::log(1.0 + F32::exp(-(2.0 * fTemp73)), std::f32::consts::E);
			self.fVec8[0] = -0.6931472 + fTemp74;
			let mut fTemp75: F32 = F32::abs(0.15 + fTemp70);
			let mut fTemp76: F32 = fTemp75 + F32::log(1.0 + F32::exp(-(2.0 * fTemp75)), std::f32::consts::E);
			let mut fTemp77: F32 = ((if iTemp72 != 0 {F32::tanh(0.5 * (0.15 + fTemp70 + self.fVec7[1]))} else {(-0.6931472 - self.fVec8[1] + fTemp76) / (if iTemp72 != 0 {1.0} else {fTemp71})}) - self.fConst6) / fTemp28;
			self.fVec9[0] = fTemp77;
			let mut fTemp78: F32 = self.fVec9[1] + 0.040633462 * (fTemp77 - self.fRec51[1]);
			self.fRec51[0] = (if (fTemp78.to_bits().cast_signed()) & 2139095040 != 0 {fTemp78} else {0.0});
			let mut fTemp79: F32 = self.fRec51[1] + 0.30075705 * (self.fRec51[0] - self.fRec50[1]);
			self.fRec50[0] = (if (fTemp79.to_bits().cast_signed()) & 2139095040 != 0 {fTemp79} else {0.0});
			let mut fTemp80: F32 = self.fRec50[1] + 0.6095243 * (self.fRec50[0] - self.fRec49[1]);
			self.fRec49[0] = (if (fTemp80.to_bits().cast_signed()) & 2139095040 != 0 {fTemp80} else {0.0});
			let mut fTemp81: F32 = self.fRec49[1] + 0.8492238 * (self.fRec49[0] - self.fRec48[1]);
			self.fRec48[0] = (if (fTemp81.to_bits().cast_signed()) & 2139095040 != 0 {fTemp81} else {0.0});
			let mut fTemp82: F32 = fTemp19 * (self.fRec34[0] - self.fRec42[0]);
			let mut iTemp83: i32 = (F32::abs(-fTemp82) < 0.001) as i32;
			let mut fTemp84: F32 = ((if iTemp83 != 0 {F32::tanh(0.5 * (0.3 + fTemp19 * (self.fRec34[0] + self.fRec42[0])))} else {(fTemp74 - fTemp76) / (if iTemp83 != 0 {1.0} else {-fTemp82})}) - self.fConst6) / fTemp28;
			self.fVec10[0] = fTemp84;
			let mut fTemp85: F32 = self.fVec10[1] + 0.15050513 * (fTemp84 - self.fRec55[1]);
			self.fRec55[0] = (if (fTemp85.to_bits().cast_signed()) & 2139095040 != 0 {fTemp85} else {0.0});
			let mut fTemp86: F32 = self.fRec55[1] + 0.4607745 * (self.fRec55[0] - self.fRec54[1]);
			self.fRec54[0] = (if (fTemp86.to_bits().cast_signed()) & 2139095040 != 0 {fTemp86} else {0.0});
			let mut fTemp87: F32 = self.fRec54[1] + 0.7385038 * (self.fRec54[0] - self.fRec53[1]);
			self.fRec53[0] = (if (fTemp87.to_bits().cast_signed()) & 2139095040 != 0 {fTemp87} else {0.0});
			let mut fTemp88: F32 = self.fRec53[1] + 0.9497428 * (self.fRec53[0] - self.fRec52[1]);
			self.fRec52[0] = (if (fTemp88.to_bits().cast_signed()) & 2139095040 != 0 {fTemp88} else {0.0});
			let mut fTemp89: F32 = self.fRec48[0] + self.fRec52[1];
			self.fVec11[0] = fTemp89;
			let mut fTemp90: F32 = 0.995 * self.fRec47[1] + 0.5 * (fTemp89 - self.fVec11[1]);
			self.fRec47[0] = (if (fTemp90.to_bits().cast_signed()) & 2139095040 != 0 {fTemp90} else {0.0});
			let mut fTemp91: F32 = self.fConst2 * (self.fConst3 * self.fRec46[1] - (self.fRec47[1] + self.fRec47[0]));
			self.fRec46[0] = (if (-fTemp91.to_bits().cast_signed()) & 2139095040 != 0 {-fTemp91} else {0.0});
			*output1 = (fTemp50 * (fTemp68 + self.fRec28[0] * (fTemp46 * self.fRec46[0] + fTemp47 * (self.fRec47[0] - self.fRec46[0]) - fTemp68))) as FaustFloat;
			self.fVec0[1] = self.fVec0[0];
			self.fRec7[1] = self.fRec7[0];
			self.fRec6[1] = self.fRec6[0];
			self.fRec5[1] = self.fRec5[0];
			self.fRec4[1] = self.fRec4[0];
			self.fRec3[1] = self.fRec3[0];
			self.fRec2[1] = self.fRec2[0];
			self.fRec1[1] = self.fRec1[0];
			self.fRec0[1] = self.fRec0[0];
			self.fRec15[1] = self.fRec15[0];
			self.fRec14[1] = self.fRec14[0];
			self.fRec13[1] = self.fRec13[0];
			self.fRec12[1] = self.fRec12[0];
			self.fRec11[1] = self.fRec11[0];
			self.fRec10[1] = self.fRec10[0];
			self.fRec9[1] = self.fRec9[0];
			self.fRec8[1] = self.fRec8[0];
			self.fRec22[1] = self.fRec22[0];
			self.fVec1[1] = self.fVec1[0];
			self.fVec2[1] = self.fVec2[0];
			self.fVec3[1] = self.fVec3[0];
			self.fRec21[1] = self.fRec21[0];
			self.fRec20[1] = self.fRec20[0];
			self.fRec19[1] = self.fRec19[0];
			self.fRec18[1] = self.fRec18[0];
			self.fVec4[1] = self.fVec4[0];
			self.fRec26[1] = self.fRec26[0];
			self.fRec25[1] = self.fRec25[0];
			self.fRec24[1] = self.fRec24[0];
			self.fRec23[1] = self.fRec23[0];
			self.fVec5[1] = self.fVec5[0];
			self.fRec17[1] = self.fRec17[0];
			self.fRec16[1] = self.fRec16[0];
			self.fRec27[1] = self.fRec27[0];
			self.fRec28[1] = self.fRec28[0];
			self.fRec29[1] = self.fRec29[0];
			self.fVec6[1] = self.fVec6[0];
			self.fRec37[1] = self.fRec37[0];
			self.fRec36[1] = self.fRec36[0];
			self.fRec35[1] = self.fRec35[0];
			self.fRec34[1] = self.fRec34[0];
			self.fRec33[1] = self.fRec33[0];
			self.fRec32[1] = self.fRec32[0];
			self.fRec31[1] = self.fRec31[0];
			self.fRec30[1] = self.fRec30[0];
			self.fRec45[1] = self.fRec45[0];
			self.fRec44[1] = self.fRec44[0];
			self.fRec43[1] = self.fRec43[0];
			self.fRec42[1] = self.fRec42[0];
			self.fRec41[1] = self.fRec41[0];
			self.fRec40[1] = self.fRec40[0];
			self.fRec39[1] = self.fRec39[0];
			self.fRec38[1] = self.fRec38[0];
			self.fVec7[1] = self.fVec7[0];
			self.fVec8[1] = self.fVec8[0];
			self.fVec9[1] = self.fVec9[0];
			self.fRec51[1] = self.fRec51[0];
			self.fRec50[1] = self.fRec50[0];
			self.fRec49[1] = self.fRec49[0];
			self.fRec48[1] = self.fRec48[0];
			self.fVec10[1] = self.fVec10[0];
			self.fRec55[1] = self.fRec55[0];
			self.fRec54[1] = self.fRec54[0];
			self.fRec53[1] = self.fRec53[0];
			self.fRec52[1] = self.fRec52[0];
			self.fVec11[1] = self.fVec11[0];
			self.fRec47[1] = self.fRec47[0];
			self.fRec46[1] = self.fRec46[0];
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

impl FaustDsp for SatDsp {
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
