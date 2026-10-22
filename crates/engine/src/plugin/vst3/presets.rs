//! `.vstpreset` files: read and written.

use super::*;

/// A class id as `.vstpreset` files write it: its 16 bytes in hex.
pub(crate) fn hex(cid: &TUID) -> String {
    cid.iter().map(|&b| format!("{:02X}", b as u8)).collect()
}

/// The class id a `.vstpreset` file is for, from its header.
pub(crate) fn preset_class(file: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = [0u8; 40];
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    (&head[..4] == b"VST3").then(|| String::from_utf8_lossy(&head[8..40]).to_uppercase())
}

/// A chunk of a `.vstpreset` file: "Comp" (the component's settings) or "Cont" (the
/// controller's), found through the chunk list its header points at.
pub(crate) fn preset_chunk<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Option<&'a [u8]> {
    let le32 = |at: usize| Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le64 = |at: usize| Some(i64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?));
    if bytes.get(..4)? != b"VST3" {
        return None;
    }
    let list = le64(40)? as usize;
    if bytes.get(list..list + 4)? != b"List" {
        return None;
    }
    (0..le32(list + 4)?.max(0) as usize).find_map(|i| {
        let at = list + 8 + i * 20;
        (bytes.get(at..at + 4)? == id).then_some(())?;
        let (offset, size) = (le64(at + 4)? as usize, le64(at + 12)? as usize);
        bytes.get(offset..offset + size)
    })
}

/// A `.vstpreset` file holding a component's and controller's settings.
pub(crate) fn write_preset(class: &str, ours: &[u8], theirs: &[u8]) -> Vec<u8> {
    let mut out = b"VST3".to_vec();
    out.extend(1i32.to_le_bytes());
    out.extend(class.as_bytes());
    let list_at = 48 + ours.len() + theirs.len();
    out.extend((list_at as i64).to_le_bytes());
    out.extend(ours);
    out.extend(theirs);
    out.extend(b"List");
    out.extend(2i32.to_le_bytes());
    for (id, offset, size) in [
        (b"Comp", 48, ours.len()),
        (b"Cont", 48 + ours.len(), theirs.len()),
    ] {
        out.extend(id);
        out.extend((offset as i64).to_le_bytes());
        out.extend((size as i64).to_le_bytes());
    }
    out
}
