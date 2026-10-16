/* ------------------------------------------------------------
name: "tempo Channel EQ"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -ct 1 -cn EqDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2
------------------------------------------------------------ */

#[repr(C)]
pub struct EqDsp {
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fHslider0: FaustFloat,
	fConst2: F32,
	fRec0: [F32;2],
	fConst3: F32,
	fConst4: F32,
	fHslider1: FaustFloat,
	fRec3: [F32;2],
	fRec1: [F32;2],
	fRec2: [F32;2],
	fRec4: [F32;2],
	fHslider2: FaustFloat,
	fHslider3: FaustFloat,
	fRec5: [F32;2],
	fHslider4: FaustFloat,
	fRec8: [F32;2],
	fRec6: [F32;2],
	fRec7: [F32;2],
	fHslider5: FaustFloat,
	fHslider6: FaustFloat,
	fRec9: [F32;2],
	fHslider7: FaustFloat,
	fRec12: [F32;2],
	fRec10: [F32;2],
	fRec11: [F32;2],
	fHslider8: FaustFloat,
	fHslider9: FaustFloat,
	fRec15: [F32;2],
	fRec13: [F32;2],
	fRec14: [F32;2],
	fRec16: [F32;2],
	fRec17: [F32;2],
	fRec18: [F32;2],
	fRec19: [F32;2],
	fRec20: [F32;2],
	fRec21: [F32;2],
	fRec22: [F32;2],
	fRec23: [F32;2],
}


fn EqDsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 10;
pub const FAUST_PASSIVES: usize = 0;

impl EqDsp {
		
	pub fn new() -> EqDsp { 
		EqDsp {
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fHslider0: 0.0,
			fConst2: 0.0,
			fRec0: [0.0;2],
			fConst3: 0.0,
			fConst4: 0.0,
			fHslider1: 0.0,
			fRec3: [0.0;2],
			fRec1: [0.0;2],
			fRec2: [0.0;2],
			fRec4: [0.0;2],
			fHslider2: 0.0,
			fHslider3: 0.0,
			fRec5: [0.0;2],
			fHslider4: 0.0,
			fRec8: [0.0;2],
			fRec6: [0.0;2],
			fRec7: [0.0;2],
			fHslider5: 0.0,
			fHslider6: 0.0,
			fRec9: [0.0;2],
			fHslider7: 0.0,
			fRec12: [0.0;2],
			fRec10: [0.0;2],
			fRec11: [0.0;2],
			fHslider8: 0.0,
			fHslider9: 0.0,
			fRec15: [0.0;2],
			fRec13: [0.0;2],
			fRec14: [0.0;2],
			fRec16: [0.0;2],
			fRec17: [0.0;2],
			fRec18: [0.0;2],
			fRec19: [0.0;2],
			fRec20: [0.0;2],
			fRec21: [0.0;2],
			fRec22: [0.0;2],
			fRec23: [0.0;2],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -ct 1 -cn EqDsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 2");
		m.declare("description", r"A low cut, two bells and a high shelf, as Logic's Channel EQ");
		m.declare("filename", r"eq.dsp");
		m.declare("filters.lib/name", r"Faust Filters Library");
		m.declare("filters.lib/svf:author", r"Oleg Nesterov");
		m.declare("filters.lib/svf:copyright", r"Copyright (C) 2020 Oleg Nesterov <oleg@redhat.com>");
		m.declare("filters.lib/svf:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/version", r"1.9.0");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"tempo Channel EQ");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("routes.lib/name", r"Faust Signal Routing Library");
		m.declare("routes.lib/version", r"1.4.0");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	
	pub fn class_init(sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (0.0) as FaustFloat;
		self.fHslider1 = (0.0) as FaustFloat;
		self.fHslider2 = (1.0) as FaustFloat;
		self.fHslider3 = (0.0) as FaustFloat;
		self.fHslider4 = (4e+02) as FaustFloat;
		self.fHslider5 = (1.0) as FaustFloat;
		self.fHslider6 = (0.0) as FaustFloat;
		self.fHslider7 = (2.5e+03) as FaustFloat;
		self.fHslider8 = (0.7071) as FaustFloat;
		self.fHslider9 = (1e+04) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.fRec0[l0 as usize] = 0.0;
		}
		for l1 in 0..2 {
			self.fRec3[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec1[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec2[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fRec4[l4 as usize] = 0.0;
		}
		for l5 in 0..2 {
			self.fRec5[l5 as usize] = 0.0;
		}
		for l6 in 0..2 {
			self.fRec8[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec6[l7 as usize] = 0.0;
		}
		for l8 in 0..2 {
			self.fRec7[l8 as usize] = 0.0;
		}
		for l9 in 0..2 {
			self.fRec9[l9 as usize] = 0.0;
		}
		for l10 in 0..2 {
			self.fRec12[l10 as usize] = 0.0;
		}
		for l11 in 0..2 {
			self.fRec10[l11 as usize] = 0.0;
		}
		for l12 in 0..2 {
			self.fRec11[l12 as usize] = 0.0;
		}
		for l13 in 0..2 {
			self.fRec15[l13 as usize] = 0.0;
		}
		for l14 in 0..2 {
			self.fRec13[l14 as usize] = 0.0;
		}
		for l15 in 0..2 {
			self.fRec14[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fRec16[l16 as usize] = 0.0;
		}
		for l17 in 0..2 {
			self.fRec17[l17 as usize] = 0.0;
		}
		for l18 in 0..2 {
			self.fRec18[l18 as usize] = 0.0;
		}
		for l19 in 0..2 {
			self.fRec19[l19 as usize] = 0.0;
		}
		for l20 in 0..2 {
			self.fRec20[l20 as usize] = 0.0;
		}
		for l21 in 0..2 {
			self.fRec21[l21 as usize] = 0.0;
		}
		for l22 in 0..2 {
			self.fRec22[l22 as usize] = 0.0;
		}
		for l23 in 0..2 {
			self.fRec23[l23 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 44.1 / self.fConst0;
		self.fConst2 = 1.0 - self.fConst1;
		self.fConst3 = 3.1415927 / self.fConst0;
		self.fConst4 = 0.45 * self.fConst0;
	}
	pub fn instance_init(&mut self, sample_rate: i32) {
		self.instance_constants(sample_rate);
		self.instance_reset_params();
		self.instance_clear();
	}
	pub fn init(&mut self, sample_rate: i32) {
		EqDsp::class_init(sample_rate);
		self.instance_init(sample_rate);
	}
	
	pub fn build_user_interface(&self, ui_interface: &mut dyn UI<FaustFloat>) {
		Self::build_user_interface_static(ui_interface);
	}
	
	pub fn build_user_interface_static(ui_interface: &mut dyn UI<FaustFloat>) {
		ui_interface.open_vertical_box("tempo Channel EQ");
		ui_interface.add_horizontal_slider("high_mid_freq", ParamIndex(0), 2.5e+03, 1e+01, 2e+04, 0.01);
		ui_interface.add_horizontal_slider("high_mid_gain", ParamIndex(1), 0.0, -3e+01, 3e+01, 0.01);
		ui_interface.add_horizontal_slider("high_mid_q", ParamIndex(2), 1.0, 0.1, 18.0, 0.001);
		ui_interface.add_horizontal_slider("low_cut", ParamIndex(3), 0.0, 0.0, 2e+04, 0.01);
		ui_interface.add_horizontal_slider("low_mid_freq", ParamIndex(4), 4e+02, 1e+01, 2e+04, 0.01);
		ui_interface.add_horizontal_slider("low_mid_gain", ParamIndex(5), 0.0, -3e+01, 3e+01, 0.01);
		ui_interface.add_horizontal_slider("low_mid_q", ParamIndex(6), 1.0, 0.1, 18.0, 0.001);
		ui_interface.add_horizontal_slider("shelf_freq", ParamIndex(7), 1e+04, 1e+01, 2e+04, 0.01);
		ui_interface.add_horizontal_slider("shelf_gain", ParamIndex(8), 0.0, -3e+01, 3e+01, 0.01);
		ui_interface.add_horizontal_slider("shelf_q", ParamIndex(9), 0.7071, 0.1, 18.0, 0.001);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			8 => Some(self.fHslider0),
			3 => Some(self.fHslider1),
			6 => Some(self.fHslider2),
			5 => Some(self.fHslider3),
			4 => Some(self.fHslider4),
			2 => Some(self.fHslider5),
			1 => Some(self.fHslider6),
			0 => Some(self.fHslider7),
			9 => Some(self.fHslider8),
			7 => Some(self.fHslider9),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			8 => { self.fHslider0 = value }
			3 => { self.fHslider1 = value }
			6 => { self.fHslider2 = value }
			5 => { self.fHslider3 = value }
			4 => { self.fHslider4 = value }
			2 => { self.fHslider5 = value }
			1 => { self.fHslider6 = value }
			0 => { self.fHslider7 = value }
			9 => { self.fHslider8 = value }
			7 => { self.fHslider9 = value }
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
		let mut fSlow0: F32 = self.fConst1 * (self.fHslider0) as F32;
		let mut fSlow1: F32 = (self.fHslider1) as F32;
		let mut fSlow2: F32 = self.fConst1 * F32::min(self.fConst4, F32::max(1e+01, fSlow1));
		let mut fSlow3: F32 = self.fConst1 * ((fSlow1 > 2e+01) as i32) as u32 as F32;
		let mut fSlow4: F32 = 1.0 / (self.fHslider2) as F32;
		let mut fSlow5: F32 = self.fConst1 * (self.fHslider3) as F32;
		let mut fSlow6: F32 = self.fConst1 * F32::min(self.fConst4, F32::max(1e+01, (self.fHslider4) as F32));
		let mut fSlow7: F32 = 1.0 / (self.fHslider5) as F32;
		let mut fSlow8: F32 = self.fConst1 * (self.fHslider6) as F32;
		let mut fSlow9: F32 = self.fConst1 * F32::min(self.fConst4, F32::max(1e+01, (self.fHslider7) as F32));
		let mut fSlow10: F32 = 1.0 / (self.fHslider8) as F32;
		let mut fSlow11: F32 = self.fConst1 * F32::min(self.fConst4, F32::max(1e+01, (self.fHslider9) as F32));
		let zipped_iterators = inputs0.zip(inputs1).zip(outputs0).zip(outputs1);
		for (((input0, input1), output0), output1) in zipped_iterators {
			let mut fTemp0: F32 = fSlow0 + self.fConst2 * self.fRec0[1];
			self.fRec0[0] = (if (fTemp0.to_bits().cast_signed()) & 2139095040 != 0 {fTemp0} else {0.0});
			let mut fTemp1: F32 = F32::powf(1e+01, 0.025 * self.fRec0[0]);
			let mut fTemp2: F32 = (*input0) as F32;
			let mut fTemp3: F32 = fSlow2 + self.fConst2 * self.fRec3[1];
			self.fRec3[0] = (if (fTemp3.to_bits().cast_signed()) & 2139095040 != 0 {fTemp3} else {0.0});
			let mut fTemp4: F32 = F32::tan(self.fConst3 * self.fRec3[0]);
			let mut fTemp5: F32 = self.fRec2[1] + fTemp4 * (fTemp2 - self.fRec1[1]);
			let mut fTemp6: F32 = 1.0 + fTemp4 * (1.4142135 + fTemp4);
			let mut fTemp7: F32 = 2.0 * (self.fRec1[1] + fTemp4 * fTemp5 / fTemp6) - self.fRec1[1];
			self.fRec1[0] = (if (fTemp7.to_bits().cast_signed()) & 2139095040 != 0 {fTemp7} else {0.0});
			let mut fTemp8: F32 = 2.0 * (fTemp5 / fTemp6) - self.fRec2[1];
			self.fRec2[0] = (if (fTemp8.to_bits().cast_signed()) & 2139095040 != 0 {fTemp8} else {0.0});
			let mut fTemp9: F32 = -1.4142135 - fTemp4;
			let mut fTemp10: F32 = fSlow3 + self.fConst2 * self.fRec4[1];
			self.fRec4[0] = (if (fTemp10.to_bits().cast_signed()) & 2139095040 != 0 {fTemp10} else {0.0});
			let mut fTemp11: F32 = (fTemp5 * fTemp9 / fTemp6 - self.fRec1[1]) * self.fRec4[0];
			let mut fTemp12: F32 = fSlow5 + self.fConst2 * self.fRec5[1];
			self.fRec5[0] = (if (fTemp12.to_bits().cast_signed()) & 2139095040 != 0 {fTemp12} else {0.0});
			let mut fTemp13: F32 = F32::powf(1e+01, 0.025 * self.fRec5[0]);
			let mut fTemp14: F32 = -1.0 + EqDsp_faustpower2_f(fTemp13);
			let mut fTemp15: F32 = fSlow6 + self.fConst2 * self.fRec8[1];
			self.fRec8[0] = (if (fTemp15.to_bits().cast_signed()) & 2139095040 != 0 {fTemp15} else {0.0});
			let mut fTemp16: F32 = F32::tan(self.fConst3 * self.fRec8[0]);
			let mut fTemp17: F32 = self.fRec7[1] + fTemp16 * (fTemp2 - self.fRec6[1] + fTemp11);
			let mut fTemp18: F32 = 1.0 + fTemp16 * (fSlow4 / fTemp13 + fTemp16);
			let mut fTemp19: F32 = 2.0 * (self.fRec6[1] + fTemp16 * fTemp17 / fTemp18) - self.fRec6[1];
			self.fRec6[0] = (if (fTemp19.to_bits().cast_signed()) & 2139095040 != 0 {fTemp19} else {0.0});
			let mut fTemp20: F32 = 2.0 * (fTemp17 / fTemp18) - self.fRec7[1];
			self.fRec7[0] = (if (fTemp20.to_bits().cast_signed()) & 2139095040 != 0 {fTemp20} else {0.0});
			let mut fTemp21: F32 = fTemp13 * fTemp18;
			let mut fTemp22: F32 = fTemp11 + fSlow4 * (fTemp14 * fTemp17 / fTemp21);
			let mut fTemp23: F32 = fSlow8 + self.fConst2 * self.fRec9[1];
			self.fRec9[0] = (if (fTemp23.to_bits().cast_signed()) & 2139095040 != 0 {fTemp23} else {0.0});
			let mut fTemp24: F32 = F32::powf(1e+01, 0.025 * self.fRec9[0]);
			let mut fTemp25: F32 = -1.0 + EqDsp_faustpower2_f(fTemp24);
			let mut fTemp26: F32 = fSlow9 + self.fConst2 * self.fRec12[1];
			self.fRec12[0] = (if (fTemp26.to_bits().cast_signed()) & 2139095040 != 0 {fTemp26} else {0.0});
			let mut fTemp27: F32 = F32::tan(self.fConst3 * self.fRec12[0]);
			let mut fTemp28: F32 = self.fRec10[1] + fTemp27 * (fTemp2 - self.fRec11[1] + fTemp22);
			let mut fTemp29: F32 = 1.0 + fTemp27 * (fSlow7 / fTemp24 + fTemp27);
			let mut fTemp30: F32 = 2.0 * (fTemp28 / fTemp29) - self.fRec10[1];
			self.fRec10[0] = (if (fTemp30.to_bits().cast_signed()) & 2139095040 != 0 {fTemp30} else {0.0});
			let mut fTemp31: F32 = 2.0 * (self.fRec11[1] + fTemp27 * fTemp28 / fTemp29) - self.fRec11[1];
			self.fRec11[0] = (if (fTemp31.to_bits().cast_signed()) & 2139095040 != 0 {fTemp31} else {0.0});
			let mut fTemp32: F32 = fTemp24 * fTemp29;
			let mut fTemp33: F32 = fTemp22 + fSlow7 * (fTemp25 * fTemp28 / fTemp32);
			let mut fTemp34: F32 = 1.0 - fTemp1;
			let mut fTemp35: F32 = fSlow11 + self.fConst2 * self.fRec15[1];
			self.fRec15[0] = (if (fTemp35.to_bits().cast_signed()) & 2139095040 != 0 {fTemp35} else {0.0});
			let mut fTemp36: F32 = F32::tan(self.fConst3 * self.fRec15[0]) * F32::sqrt(fTemp1);
			let mut fTemp37: F32 = self.fRec14[1] + fTemp36 * (fTemp2 - self.fRec13[1] + fTemp33);
			let mut fTemp38: F32 = 1.0 + fTemp36 * (fSlow10 + fTemp36);
			let mut fTemp39: F32 = self.fRec13[1] + fTemp36 * fTemp37 / fTemp38;
			let mut fTemp40: F32 = 2.0 * fTemp39 - self.fRec13[1];
			self.fRec13[0] = (if (fTemp40.to_bits().cast_signed()) & 2139095040 != 0 {fTemp40} else {0.0});
			let mut fTemp41: F32 = 2.0 * (fTemp37 / fTemp38) - self.fRec14[1];
			self.fRec14[0] = (if (fTemp41.to_bits().cast_signed()) & 2139095040 != 0 {fTemp41} else {0.0});
			let mut fTemp42: F32 = 1.0 - EqDsp_faustpower2_f(fTemp1);
			*output0 = (fTemp1 * (fTemp1 * (fTemp2 + fTemp33) + fSlow10 * (fTemp34 * fTemp37 / fTemp38)) + fTemp39 * fTemp42) as FaustFloat;
			let mut fTemp43: F32 = (*input1) as F32;
			let mut fTemp44: F32 = self.fRec16[1] + fTemp4 * (fTemp43 - self.fRec17[1]);
			let mut fTemp45: F32 = 2.0 * (fTemp44 / fTemp6) - self.fRec16[1];
			self.fRec16[0] = (if (fTemp45.to_bits().cast_signed()) & 2139095040 != 0 {fTemp45} else {0.0});
			let mut fTemp46: F32 = 2.0 * (self.fRec17[1] + fTemp4 * fTemp44 / fTemp6) - self.fRec17[1];
			self.fRec17[0] = (if (fTemp46.to_bits().cast_signed()) & 2139095040 != 0 {fTemp46} else {0.0});
			let mut fTemp47: F32 = self.fRec4[0] * (fTemp9 * fTemp44 / fTemp6 - self.fRec17[1]);
			let mut fTemp48: F32 = self.fRec19[1] + fTemp16 * (fTemp43 - self.fRec18[1] + fTemp47);
			let mut fTemp49: F32 = 2.0 * (self.fRec18[1] + fTemp16 * fTemp48 / fTemp18) - self.fRec18[1];
			self.fRec18[0] = (if (fTemp49.to_bits().cast_signed()) & 2139095040 != 0 {fTemp49} else {0.0});
			let mut fTemp50: F32 = 2.0 * (fTemp48 / fTemp18) - self.fRec19[1];
			self.fRec19[0] = (if (fTemp50.to_bits().cast_signed()) & 2139095040 != 0 {fTemp50} else {0.0});
			let mut fTemp51: F32 = fTemp47 + fSlow4 * (fTemp14 * fTemp48 / fTemp21);
			let mut fTemp52: F32 = self.fRec21[1] + fTemp27 * (fTemp43 - self.fRec20[1] + fTemp51);
			let mut fTemp53: F32 = 2.0 * (self.fRec20[1] + fTemp27 * fTemp52 / fTemp29) - self.fRec20[1];
			self.fRec20[0] = (if (fTemp53.to_bits().cast_signed()) & 2139095040 != 0 {fTemp53} else {0.0});
			let mut fTemp54: F32 = 2.0 * (fTemp52 / fTemp29) - self.fRec21[1];
			self.fRec21[0] = (if (fTemp54.to_bits().cast_signed()) & 2139095040 != 0 {fTemp54} else {0.0});
			let mut fTemp55: F32 = fTemp51 + fSlow7 * (fTemp25 * fTemp52 / fTemp32);
			let mut fTemp56: F32 = self.fRec22[1] + fTemp36 * (fTemp43 - self.fRec23[1] + fTemp55);
			let mut fTemp57: F32 = 2.0 * (fTemp56 / fTemp38) - self.fRec22[1];
			self.fRec22[0] = (if (fTemp57.to_bits().cast_signed()) & 2139095040 != 0 {fTemp57} else {0.0});
			let mut fTemp58: F32 = self.fRec23[1] + fTemp36 * fTemp56 / fTemp38;
			let mut fTemp59: F32 = 2.0 * fTemp58 - self.fRec23[1];
			self.fRec23[0] = (if (fTemp59.to_bits().cast_signed()) & 2139095040 != 0 {fTemp59} else {0.0});
			*output1 = (fTemp1 * (fTemp1 * (fTemp43 + fTemp55) + fSlow10 * (fTemp34 * fTemp56 / fTemp38)) + fTemp42 * fTemp58) as FaustFloat;
			self.fRec0[1] = self.fRec0[0];
			self.fRec3[1] = self.fRec3[0];
			self.fRec1[1] = self.fRec1[0];
			self.fRec2[1] = self.fRec2[0];
			self.fRec4[1] = self.fRec4[0];
			self.fRec5[1] = self.fRec5[0];
			self.fRec8[1] = self.fRec8[0];
			self.fRec6[1] = self.fRec6[0];
			self.fRec7[1] = self.fRec7[0];
			self.fRec9[1] = self.fRec9[0];
			self.fRec12[1] = self.fRec12[0];
			self.fRec10[1] = self.fRec10[0];
			self.fRec11[1] = self.fRec11[0];
			self.fRec15[1] = self.fRec15[0];
			self.fRec13[1] = self.fRec13[0];
			self.fRec14[1] = self.fRec14[0];
			self.fRec16[1] = self.fRec16[0];
			self.fRec17[1] = self.fRec17[0];
			self.fRec18[1] = self.fRec18[0];
			self.fRec19[1] = self.fRec19[0];
			self.fRec20[1] = self.fRec20[0];
			self.fRec21[1] = self.fRec21[0];
			self.fRec22[1] = self.fRec22[0];
			self.fRec23[1] = self.fRec23[0];
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

impl FaustDsp for EqDsp {
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
