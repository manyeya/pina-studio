//! Settings saved and restored, and `.fxp` preset files.

use super::*;

/// What a plugin's settings are kept as: its own chunk, or its parameters' values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Settings<'a> {
    Chunk(std::borrow::Cow<'a, [u8]>),
    Params(Vec<f32>),
}

/// A plugin's state as one blob: its program, then its chunk or its parameters' values.
pub(crate) fn pack(program: i32, settings: &Settings) -> Vec<u8> {
    let mut out = STATE_MAGIC.to_vec();
    out.extend(program.to_le_bytes());
    match settings {
        Settings::Chunk(c) => {
            out.push(b'C');
            out.extend(c.iter());
        }
        Settings::Params(values) => {
            out.push(b'P');
            out.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        }
    }
    out
}

pub(crate) fn unpack(blob: &[u8]) -> Option<(i32, Settings<'_>)> {
    let rest = blob.strip_prefix(STATE_MAGIC)?;
    let (program, rest) = rest.split_first_chunk::<4>()?;
    let settings = match rest.split_first()? {
        (b'C', chunk) => Settings::Chunk(chunk.into()),
        (b'P', values) if values.len() % 4 == 0 => Settings::Params(
            values
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&b| f32::from_le_bytes(b))
                .collect(),
        ),
        _ => return None,
    };
    Some((i32::from_le_bytes(*program), settings))
}

/// An `.fxp` preset: the plugin it's for, its name and settings.
#[derive(Debug, PartialEq)]
pub(crate) struct Fxp<'a> {
    pub(crate) id: i32,
    pub(crate) name: String,
    pub(crate) settings: Settings<'a>,
}

pub(crate) fn read_fxp(bytes: &[u8]) -> Option<Fxp<'_>> {
    let be = |at: usize| Some(i32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if bytes.get(..4)? != b"CcnK" {
        return None;
    }
    let count = be(24)?.max(0) as usize;
    let settings = match bytes.get(8..12)? {
        b"FxCk" if count <= (bytes.len().saturating_sub(FXP_HEADER)) / 4 => Settings::Params(
            (0..count)
                .map(|i| be(FXP_HEADER + 4 * i).map(|b| f32::from_bits(b as u32)))
                .collect::<Option<_>>()?,
        ),
        b"FPCh" => {
            let size = be(FXP_HEADER)?.max(0) as usize;
            let at = FXP_HEADER + 4;
            Settings::Chunk(bytes.get(at..at.checked_add(size)?)?.into())
        }
        _ => return None,
    };
    Some(Fxp {
        id: be(16)?,
        name: text(bytes.get(28..FXP_HEADER)?),
        settings,
    })
}

/// The plugin an `.fxp` file is for, from its header.
pub(crate) fn fxp_id(file: &Path) -> Option<i32> {
    use std::io::Read;
    let mut head = [0u8; 20];
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    (&head[..4] == b"CcnK").then(|| i32::from_be_bytes(head[16..20].try_into().unwrap()))
}

/// An `.fxp` file: `settings` for plugin `id` (at `version`, with `params` parameters), called
/// `name`.
pub(crate) fn write_fxp(
    id: i32,
    version: i32,
    params: u32,
    name: &str,
    settings: &Settings,
) -> Vec<u8> {
    let mut body = Vec::new();
    let magic = match settings {
        Settings::Params(_) => b"FxCk",
        Settings::Chunk(_) => b"FPCh",
    };
    body.extend(magic);
    body.extend(1i32.to_be_bytes());
    body.extend(id.to_be_bytes());
    body.extend(version.to_be_bytes());
    let count = match settings {
        Settings::Params(values) => values.len() as i32,
        Settings::Chunk(_) => params as i32,
    };
    body.extend(count.to_be_bytes());
    let mut prg_name = [0u8; 28];
    for (dst, src) in prg_name[..27].iter_mut().zip(name.bytes()) {
        *dst = src;
    }
    body.extend(prg_name);
    match settings {
        Settings::Params(values) => body.extend(values.iter().flat_map(|v| v.to_be_bytes())),
        Settings::Chunk(c) => {
            body.extend((c.len() as i32).to_be_bytes());
            body.extend(c.iter());
        }
    }
    let mut out = b"CcnK".to_vec();
    out.extend((body.len() as i32).to_be_bytes());
    out.extend(body);
    out
}
