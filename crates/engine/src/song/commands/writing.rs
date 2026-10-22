//! Commands that write music: chords, bass, melody and drums, arranging, copying bars, transitions.

use super::*;

/// The writing commands.
pub(super) fn run(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetChords {
            chords,
            start,
            bars,
        } => return compose::set_chords(p, chords.as_deref(), start, bars),
        Command::WriteChords(w) => return compose::write_chords(p, w),
        Command::WriteBass(w) => return compose::write_bass(p, w),
        Command::WriteMelody(w) => return compose::write_melody(p, w),
        Command::WriteDrums(w) => return compose::write_drums(p, w),
        Command::Arrange(a) => return arrange::arrange(p, a),
        Command::CopyBars {
            from,
            bars,
            to,
            tracks,
        } => return arrange::copy_bars(p, from, bars, to, tracks),
        Command::Transition(t) => return arrange::transition(p, dir, t),
        _ => unreachable!("not one of the writing commands"),
    }
}
