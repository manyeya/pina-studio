//! The lyrics, as far as they can be read without understanding them: each line's syllables and
//! the sound its last word ends on (so its rhyme scheme), the lines said again (its hooks), who's
//! speaking to whom, and the words it keeps coming back to. What it means is for a reader: the
//! agent, given all this.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Line {
    /// When it's sung, seconds, if the lyrics are timed.
    pub time: Option<f32>,
    pub text: String,
    /// The section it's in (an index into the study's sections).
    pub section: Option<usize>,
    pub syllables: usize,
    /// Its letter in its section's rhyme scheme.
    pub rhyme: char,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// Lines sung three times or more.
    pub hooks: Vec<String>,
    /// Each section's rhyme scheme, by its index: "AABB", "ABAB".
    pub schemes: Vec<(usize, String)>,
    /// Who it's sung as, and to: "first person, to you".
    pub voice: String,
    /// The words it keeps coming back to, most first, with how often.
    pub words: Vec<(String, usize)>,
    /// Where the lyrics came from.
    pub source: String,
}

/// A word's syllables, by its vowel groups (a silent final e not counted).
pub fn syllables(word: &str) -> usize {
    let w: Vec<char> = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if w.is_empty() {
        return 0;
    }
    let vowel = |c: char| "aeiouy".contains(c);
    let mut groups = 0;
    let mut prev = false;
    for &c in &w {
        let v = vowel(c);
        if v && !prev {
            groups += 1;
        }
        prev = v;
    }
    let n = w.len();
    if n > 2
        && w[n - 1] == 'e'
        && !vowel(w[n - 2])
        && !(w[n - 2] == 'l' && n > 3 && !vowel(w[n - 3]))
        && groups > 1
    {
        groups -= 1;
    }
    groups.max(1)
}

/// The sound a word ends on, for rhyming: from its last vowel sound to its end, spelt simply
/// ("night", "light" → "ite"; "fire", "desire" → "ire"; "away", "today" → "ay").
pub fn rhyme_key(word: &str) -> String {
    let w: String = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if w.is_empty() {
        return String::new();
    }
    // Spellings of one sound, as one.
    let mut s = w
        .replace("igh", "i")
        .replace("ie", "i")
        .replace("ea", "e")
        .replace("ee", "e")
        .replace("ey", "e")
        .replace("ay", "a")
        .replace("ai", "a")
        .replace("ow", "o")
        .replace("oa", "o")
        .replace("ph", "f")
        .replace("ck", "k");
    // A final silent e lengthens the vowel before it: "time" is "im" said long.
    if s.len() > 2 && s.ends_with('e') {
        s.pop();
        s.push('*');
    }
    if s.ends_with('y') && s.len() > 2 {
        s.pop();
        s.push('e');
    }
    let chars: Vec<char> = s.chars().collect();
    let vowel = |c: char| "aeiouy".contains(c);
    // The last vowel group, and what follows it.
    let mut i = chars.len();
    while i > 0 && !vowel(chars[i - 1]) {
        i -= 1;
    }
    while i > 1 && vowel(chars[i - 2]) {
        i -= 1;
    }
    chars[i.saturating_sub(1)..].iter().collect()
}

/// Rhyme letters for lines (each by its last word): the same sound, the same letter.
pub fn scheme(lines: &[&str]) -> String {
    let mut keys: Vec<String> = Vec::new();
    let mut out = String::new();
    for line in lines {
        let last = line.split_whitespace().last().unwrap_or("");
        let k = rhyme_key(last);
        let at = keys
            .iter()
            .position(|x| !k.is_empty() && (*x == k || near(x, &k)));
        let i = match at {
            Some(i) => i,
            None => {
                keys.push(k);
                keys.len() - 1
            }
        };
        out.push((b'A' + (i % 26) as u8) as char);
    }
    out
}

/// Near rhymes: the same vowel, ending on consonants that sound alike.
fn near(a: &str, b: &str) -> bool {
    let (av, bv): (String, String) = (
        a.chars().take_while(|c| "aeiouy*".contains(*c)).collect(),
        b.chars().take_while(|c| "aeiouy*".contains(*c)).collect(),
    );
    let alike = |x: &str, y: &str| {
        let family = |c: char| match c {
            'm' | 'n' => 'n',
            'd' | 't' => 't',
            'b' | 'p' => 'p',
            'g' | 'k' | 'c' => 'k',
            's' | 'z' => 's',
            c => c,
        };
        let sound = |s: &str| {
            s.chars()
                .filter(|c| *c != '*')
                .map(family)
                .collect::<String>()
        };
        sound(x) == sound(y)
    };
    !av.is_empty() && av == bv && alike(&a[av.len()..], &b[bv.len()..])
}

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "at", "for", "with", "is", "it",
    "its", "it's", "i", "im", "i'm", "you", "your", "you're", "me", "my", "we", "our", "us",
    "they", "them", "he", "she", "his", "her", "this", "that", "be", "am", "are", "was", "were",
    "been", "do", "dont", "don't", "so", "if", "just", "all", "up", "down", "out", "no", "not",
    "oh", "yeah", "ooh", "la", "na", "like", "got", "get", "gonna", "wanna", "can", "cant",
    "can't", "will", "what", "when", "where", "how", "who", "now", "then", "there", "here", "too",
    "baby", "know", "let", "go", "say", "yes", "from", "into", "by", "as", "ya", "yo", "uh", "ah",
    "hey", "ill", "i'll", "ive", "i've", "id", "i'd",
];

/// The lyrics read: `lines` (each its time, if known, and words), `sections` as (start, end)
/// seconds.
pub fn read(lines: &[(Option<f32>, String)], sections: &[(f32, f32)], source: &str) -> Lyrics {
    let clean: Vec<(Option<f32>, String)> = lines
        .iter()
        .map(|(t, s)| (*t, s.trim().to_string()))
        .filter(|(_, s)| !s.is_empty())
        .collect();
    let section_of =
        |t: Option<f32>| t.and_then(|t| sections.iter().position(|&(a, b)| t >= a - 0.5 && t < b));
    let mut out = Lyrics {
        lines: clean
            .iter()
            .map(|(t, s)| Line {
                time: *t,
                text: s.clone(),
                section: section_of(*t),
                syllables: s.split_whitespace().map(syllables).sum(),
                rhyme: 'A',
            })
            .collect(),
        source: source.into(),
        ..Default::default()
    };
    // Rhyme schemes, section by section (or in fours, untimed).
    let groups: Vec<(Option<usize>, Vec<usize>)> = {
        let mut g: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        for (i, l) in out.lines.iter().enumerate() {
            let key = l.section.or(Some(usize::MAX - i / 4));
            match g.last_mut() {
                Some((k, v)) if *k == key => v.push(i),
                _ => g.push((key, vec![i])),
            }
        }
        g
    };
    for (section, idx) in &groups {
        let texts: Vec<&str> = idx.iter().map(|&i| out.lines[i].text.as_str()).collect();
        let s = scheme(&texts);
        for (k, &i) in idx.iter().enumerate() {
            out.lines[i].rhyme = s.chars().nth(k).unwrap_or('A');
        }
        if let Some(sec) = section.filter(|s| *s < sections.len()) {
            out.schemes.push((sec, s));
        }
    }
    // Hooks: lines sung three times or more.
    let norm = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .collect::<String>()
    };
    let mut counts: Vec<(String, String, usize)> = Vec::new();
    for l in &out.lines {
        let n = norm(&l.text);
        match counts.iter_mut().find(|(k, _, _)| *k == n) {
            Some(e) => e.2 += 1,
            None => counts.push((n, l.text.clone(), 1)),
        }
    }
    counts.sort_by_key(|e| std::cmp::Reverse(e.2));
    out.hooks = counts
        .iter()
        .filter(|e| e.2 >= 3)
        .map(|e| e.1.clone())
        .collect();
    // Voice: who sings, to whom.
    let all: Vec<String> = out
        .lines
        .iter()
        .flat_map(|l| {
            l.text
                .split_whitespace()
                .map(|w| {
                    w.to_lowercase()
                        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let has = |set: &[&str]| all.iter().filter(|w| set.contains(&w.as_str())).count();
    let (i, you, we) = (
        has(&["i", "me", "my", "mine", "i'm", "im", "i've", "i'll"]),
        has(&["you", "your", "yours", "you're", "ya"]),
        has(&["we", "us", "our", "we're"]),
    );
    out.voice = match (i > 0, you > 0, we > i) {
        (_, _, true) => "first person plural: \"we\"".into(),
        (true, true, _) => "first person, to \"you\"".into(),
        (true, false, _) => "first person".into(),
        (false, true, _) => "second person: \"you\"".into(),
        _ => "third person".into(),
    };
    // Its words: the content words, most often first.
    let mut freq: Vec<(String, usize)> = Vec::new();
    for w in &all {
        let w = w.trim_matches('\'');
        if w.len() < 3 || STOP.contains(&w) {
            continue;
        }
        match freq.iter_mut().find(|(x, _)| x == w) {
            Some(e) => e.1 += 1,
            None => freq.push((w.to_string(), 1)),
        }
    }
    freq.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    freq.truncate(16);
    out.words = freq;
    out
}

/// Synced lyrics ("[01:02.30] a line") as timed lines; plain lyrics as untimed ones.
pub fn parse(synced: Option<&str>, plain: Option<&str>) -> Vec<(Option<f32>, String)> {
    if let Some(s) = synced.filter(|s| !s.trim().is_empty()) {
        let mut out = Vec::new();
        for line in s.lines() {
            let mut rest = line.trim();
            let mut times = Vec::new();
            while let Some(r) = rest.strip_prefix('[') {
                let Some(end) = r.find(']') else { break };
                let stamp = &r[..end];
                if let Some((m, sec)) = stamp.split_once(':')
                    && let (Ok(m), Ok(sec)) = (m.parse::<f32>(), sec.parse::<f32>())
                {
                    times.push(m * 60.0 + sec);
                }
                rest = r[end + 1..].trim();
            }
            for t in times {
                out.push((Some(t), rest.to_string()));
            }
        }
        out.sort_by(|a, b| a.0.unwrap_or(0.0).total_cmp(&b.0.unwrap_or(0.0)));
        return out.into_iter().filter(|(_, s)| !s.is_empty()).collect();
    }
    // Plain: a stray time stamp at a line's start is dropped, a "[Chorus]" heading skipped.
    plain
        .unwrap_or("")
        .lines()
        .map(|l| {
            let mut rest = l.trim();
            while let Some(r) = rest.strip_prefix('[')
                && let Some(end) = r.find(']')
                && r[..end]
                    .split_once(':')
                    .is_some_and(|(m, s)| m.parse::<f32>().is_ok() && s.parse::<f32>().is_ok())
            {
                rest = r[end + 1..].trim();
            }
            rest
        })
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .map(|l| (None, l.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
