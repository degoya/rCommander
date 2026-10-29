//! The `CF_HDROP` clipboard format as bytes (RC-080-01): a `DROPFILES` header, then each path as
//! NUL-terminated UTF-16, the list closed by one more NUL. Ctrl+C and Ctrl+X write it with a
//! "Preferred DropEffect" (copy or move), Ctrl+V reads what the Explorer wrote; the Windows
//! clipboard module (`windows/clipboard.rs`) only moves the bytes in and out of global memory.
//! Pure, so both directions are tested on every system.
//!
//! What the clipboard holds is another program's: [`decode`] reads it bounded - a header that
//! points outside the block, an ANSI list, an unterminated or overlong path or too many paths
//! give nothing.

#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "used by the Windows clipboard and the tests only")
)]

/// `sizeof(DROPFILES)`: `pFiles`, `pt.x`, `pt.y`, `fNC` and `fWide`, four bytes each.
pub const HEADER_LEN: usize = 20;
/// The most paths read from one list; a longer one is hostile or a mistake.
pub const MAX_PATHS: usize = 10_000;
/// The longest path read, in UTF-16 units: the longest path Windows has (`\\?\` paths).
pub const MAX_PATH_UNITS: usize = 32_767;

/// `DROPEFFECT_COPY` and `DROPEFFECT_MOVE`, the values of the "Preferred DropEffect" format.
pub const EFFECT_COPY: u32 = 1;
pub const EFFECT_MOVE: u32 = 2;

/// The `CF_HDROP` block for `paths`, each given as UTF-16 without a NUL. A path that is empty or
/// holds a NUL cannot be written into the list and is left out.
pub fn encode(paths: &[Vec<u16>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + 2);
    // pFiles: the list starts right after the header; the point and fNC stay zero.
    bytes.extend_from_slice(&(HEADER_LEN as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 12]);
    // fWide: the paths are UTF-16.
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    for path in paths
        .iter()
        .filter(|path| !path.is_empty() && !path.contains(&0))
    {
        for unit in path.iter().chain(Some(&0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}

/// The paths of a `CF_HDROP` block, as UTF-16 without their NULs; `None` for a block that is no
/// complete wide list within the limits.
pub fn decode(bytes: &[u8]) -> Option<Vec<Vec<u16>>> {
    let word = |at: usize| {
        let field = bytes.get(at..at + 4)?;
        Some(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
    };
    let start = usize::try_from(word(0)?).ok()?;
    // An ANSI list (fWide 0) would need the writer's code page; the Explorer writes UTF-16.
    if word(16)? == 0 || start < HEADER_LEN || start > bytes.len() {
        return None;
    }
    let units: Vec<u16> = bytes[start..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let mut paths = Vec::new();
    let mut pieces = units.split(|&unit| unit == 0);
    // The last piece has no NUL after it: an unterminated path, or nothing after the list's end.
    let unterminated = pieces.next_back();
    for path in pieces {
        if path.is_empty() {
            return Some(paths);
        }
        if paths.len() == MAX_PATHS || path.len() > MAX_PATH_UNITS {
            return None;
        }
        paths.push(path.to_vec());
    }
    // Every path ended with its NUL, only the list's own NUL is missing.
    unterminated.filter(|rest| rest.is_empty()).map(|_| paths)
}

/// The "Preferred DropEffect" block: move for Ctrl+X, copy for Ctrl+C.
pub fn effect_bytes(cut: bool) -> [u8; 4] {
    let effect = if cut { EFFECT_MOVE } else { EFFECT_COPY };
    effect.to_le_bytes()
}

/// Whether a "Preferred DropEffect" block asks for a move (the Explorer's Ctrl+X); anything
/// else, an unreadable block included, is a copy.
pub fn is_move(bytes: &[u8]) -> bool {
    let Some(&[a, b, c, d]) = bytes.get(..4) else {
        return false;
    };
    let effect = u32::from_le_bytes([a, b, c, d]);
    effect & EFFECT_MOVE != 0 && effect & EFFECT_COPY == 0
}

#[cfg(test)]
#[path = "dropfiles_tests.rs"]
mod tests;
