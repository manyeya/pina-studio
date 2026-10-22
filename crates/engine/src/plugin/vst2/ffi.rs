//! The VST2 interface as plugins see it: its structs, opcodes and callbacks.

use super::*;

pub(crate) type Dispatcher =
    unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;

pub(crate) type ProcessFn = unsafe extern "C" fn(*mut AEffect, *mut *mut f32, *mut *mut f32, i32);

pub(crate) type ProcessDoubleFn =
    unsafe extern "C" fn(*mut AEffect, *mut *mut f64, *mut *mut f64, i32);

pub(crate) type SetParameter = unsafe extern "C" fn(*mut AEffect, i32, f32);

pub(crate) type GetParameter = unsafe extern "C" fn(*mut AEffect, i32) -> f32;

pub(crate) type HostCallback =
    unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;

pub(crate) type Entry = unsafe extern "C" fn(HostCallback) -> *mut AEffect;

/// A plugin instance, as its entry point makes it.
#[repr(C)]
pub(crate) struct AEffect {
    pub(crate) magic: i32,
    pub(crate) dispatcher: Option<Dispatcher>,
    /// The old accumulating `process`, for plugins without `process_replacing`.
    pub(crate) process: Option<ProcessFn>,
    pub(crate) set_parameter: Option<SetParameter>,
    pub(crate) get_parameter: Option<GetParameter>,
    pub(crate) num_programs: i32,
    pub(crate) num_params: i32,
    pub(crate) num_inputs: i32,
    pub(crate) num_outputs: i32,
    pub(crate) flags: i32,
    /// The host's: Pina Studio keeps the plugin's `Instance` here, for the host callback.
    pub(crate) resvd1: isize,
    pub(crate) resvd2: isize,
    pub(crate) initial_delay: i32,
    pub(crate) real_qualities: i32,
    pub(crate) off_qualities: i32,
    pub(crate) io_ratio: f32,
    pub(crate) object: *mut c_void,
    pub(crate) user: *mut c_void,
    pub(crate) unique_id: i32,
    pub(crate) version: i32,
    pub(crate) process_replacing: Option<ProcessFn>,
    pub(crate) process_double_replacing: Option<ProcessDoubleFn>,
    pub(crate) future: [u8; 56],
}

/// What Pina Studio asks of a plugin, through its dispatcher.
pub(crate) mod eff {
    pub const OPEN: i32 = 0;
    pub const CLOSE: i32 = 1;
    pub const SET_PROGRAM: i32 = 2;
    pub const GET_PROGRAM: i32 = 3;
    pub const GET_PARAM_LABEL: i32 = 6;
    pub const GET_PARAM_DISPLAY: i32 = 7;
    pub const GET_PARAM_NAME: i32 = 8;
    pub const SET_SAMPLE_RATE: i32 = 10;
    pub const SET_BLOCK_SIZE: i32 = 11;
    pub const MAINS_CHANGED: i32 = 12;
    pub const EDIT_GET_RECT: i32 = 13;
    pub const EDIT_OPEN: i32 = 14;
    pub const EDIT_CLOSE: i32 = 15;
    pub const EDIT_IDLE: i32 = 19;
    pub const GET_CHUNK: i32 = 23;
    pub const SET_CHUNK: i32 = 24;
    pub const PROCESS_EVENTS: i32 = 25;
    pub const CAN_BE_AUTOMATED: i32 = 26;
    pub const STRING_TO_PARAMETER: i32 = 27;
    pub const GET_PROGRAM_NAME_INDEXED: i32 = 29;
    pub const GET_PLUG_CATEGORY: i32 = 35;
    pub const GET_EFFECT_NAME: i32 = 45;
    pub const GET_VENDOR_STRING: i32 = 47;
    pub const VENDOR_SPECIFIC: i32 = 50;
    pub const GET_PARAMETER_PROPERTIES: i32 = 56;
    pub const BEGIN_SET_PROGRAM: i32 = 67;
    pub const END_SET_PROGRAM: i32 = 68;
    pub const SHELL_GET_NEXT_PLUGIN: i32 = 70;
    pub const START_PROCESS: i32 = 71;
    pub const STOP_PROCESS: i32 = 72;
    pub const SET_PROCESS_PRECISION: i32 = 77;
}

/// What a plugin asks of Pina Studio, through the host callback. The SDK skips 5: the opcodes after
/// `audioMasterPinConnected` (4) start at 6.
pub(crate) mod master {
    pub const AUTOMATE: i32 = 0;
    pub const VERSION: i32 = 1;
    pub const CURRENT_ID: i32 = 2;
    pub const IDLE: i32 = 3;
    pub const GET_TIME: i32 = 7;
    pub const PROCESS_EVENTS: i32 = 8;
    pub const IO_CHANGED: i32 = 13;
    pub const SIZE_WINDOW: i32 = 15;
    pub const GET_SAMPLE_RATE: i32 = 16;
    pub const GET_BLOCK_SIZE: i32 = 17;
    pub const GET_CURRENT_PROCESS_LEVEL: i32 = 23;
    pub const GET_VENDOR_STRING: i32 = 32;
    pub const GET_PRODUCT_STRING: i32 = 33;
    pub const GET_VENDOR_VERSION: i32 = 34;
    pub const CAN_DO: i32 = 37;
    pub const UPDATE_DISPLAY: i32 = 42;
    pub const BEGIN_EDIT: i32 = 43;
    pub const END_EDIT: i32 = 44;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VstTimeInfo {
    pub(crate) sample_pos: f64,
    pub(crate) sample_rate: f64,
    pub(crate) nano_seconds: f64,
    pub(crate) ppq_pos: f64,
    pub(crate) tempo: f64,
    pub(crate) bar_start_pos: f64,
    pub(crate) cycle_start_pos: f64,
    pub(crate) cycle_end_pos: f64,
    pub(crate) time_sig_numerator: i32,
    pub(crate) time_sig_denominator: i32,
    pub(crate) smpte_offset: i32,
    pub(crate) smpte_frame_rate: i32,
    pub(crate) samples_to_next_clock: i32,
    pub(crate) flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VstMidiEvent {
    pub(crate) kind: i32,
    pub(crate) byte_size: i32,
    pub(crate) delta_frames: i32,
    pub(crate) flags: i32,
    pub(crate) note_length: i32,
    pub(crate) note_offset: i32,
    pub(crate) midi_data: [u8; 4],
    pub(crate) detune: i8,
    pub(crate) note_off_velocity: u8,
    pub(crate) reserved1: u8,
    pub(crate) reserved2: u8,
}

/// `VstEvents`, with room for `EVENTS`.
#[repr(C)]
pub(crate) struct VstEvents {
    pub(crate) num_events: i32,
    pub(crate) reserved: isize,
    pub(crate) events: [*mut VstMidiEvent; EVENTS],
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct ERect {
    pub(crate) top: i16,
    pub(crate) left: i16,
    pub(crate) bottom: i16,
    pub(crate) right: i16,
}

#[repr(C)]
pub(crate) struct ParameterProperties {
    pub(crate) step_float: f32,
    pub(crate) small_step_float: f32,
    pub(crate) large_step_float: f32,
    pub(crate) label: [u8; 64],
    pub(crate) flags: i32,
    pub(crate) min_integer: i32,
    pub(crate) max_integer: i32,
    pub(crate) step_integer: i32,
    pub(crate) large_step_integer: i32,
    pub(crate) short_label: [u8; 8],
    pub(crate) display_index: i16,
    pub(crate) category: i16,
    pub(crate) num_parameters_in_category: i16,
    pub(crate) reserved: i16,
    pub(crate) category_label: [u8; 24],
    pub(crate) future: [u8; 16],
}
