//! Commands that write music the agent doesn't write note by note: the chord track, bars copied,
//! and the sounds of transitions.

use super::*;

/// The writing commands.
pub(super) fn run(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetChords {
            chords,
            start,
            bars,
        } => compose::set_chords(p, chords.as_deref(), start, bars),
        Command::CopyBars {
            from,
            bars,
            to,
            tracks,
        } => arrange::copy_bars(p, from, bars, to, tracks),
        Command::Transition(t) => arrange::transition(p, dir, t),
        _ => unreachable!("not one of the writing commands"),
    }
}
