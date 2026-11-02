//! A progression laid out in time: each chord and the ticks it plays.

use super::*;

/// A chord and when it plays, in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
    pub chord: Chord,
}

/// A progression written out: bars split by "|", each bar's chords sharing it equally; chords as
/// symbols or numerals. Repeated to fill `bars` bars from `start`.
pub fn lay_out(
    text: &str,
    key: &Key,
    start: u64,
    bars: u64,
    bar: u64,
) -> Result<Vec<Span>, String> {
    let groups: Vec<Vec<Chord>> = text
        .split('|')
        .map(|g| {
            g.split_whitespace()
                .map(|c| read_chord(c, key).ok_or(format!("{c:?} isn't a chord Pina Studio reads")))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let groups: Vec<Vec<Chord>> = groups.into_iter().filter(|g| !g.is_empty()).collect();
    if groups.is_empty() {
        return Err("no chords".into());
    }
    let mut out: Vec<Span> = Vec::new();
    for b in 0..bars {
        let g = &groups[b as usize % groups.len()];
        let each = bar / g.len() as u64;
        for (i, c) in g.iter().enumerate() {
            let s = start + b * bar + i as u64 * each;
            let e = if i + 1 == g.len() {
                start + (b + 1) * bar
            } else {
                s + each
            };
            match out.last_mut() {
                // The same chord again carries on.
                Some(last) if last.chord == *c && last.end == s => last.end = e,
                _ => out.push(Span {
                    start: s,
                    end: e,
                    chord: *c,
                }),
            }
        }
    }
    Ok(out)
}
