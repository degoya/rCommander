//! The nesting budget: usvg and resvg each recurse once per level of element nesting (roxmltree
//! parses iteratively since 0.21.2), so a document of 100 000 nested `<g>` - a few hundred
//! kilobytes - overflows the stack, which in the component is a trap. The nesting is told first
//! by a scan of the bytes that keeps no state but a counter, and a document deeper than the
//! budget is never parsed. What the parser made of it is counted once more ([`tree_within`],
//! RC-080-69): the scan and the parser can never disagree into a deeper tree.

use roxmltree::{Document, NodeId};

/// The deepest element nesting a document may have, the root counted. Far beyond what an editor
/// writes (a few dozen levels), and low enough that the recursions together stay inside
/// the component's stack.
pub const MAX_DEPTH: usize = 256;

/// Whether no element of `text` is nested deeper than `limit`. `<name` opens an element unless
/// its tag ends in `/>`, `</` closes one; comments, CDATA sections, processing instructions,
/// the DOCTYPE with its internal subset and quoted attribute values are skipped, so a `<` inside
/// them counts nothing. Malformed markup is counted as it looks; the parser refuses it later.
#[must_use]
pub fn depth_within(text: &[u8], limit: usize) -> bool {
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(start) = find(text, at, b"<") {
        let rest = &text[start..];
        at = if rest.starts_with(b"<!--") {
            after(text, start + 4, b"-->")
        } else if rest.starts_with(b"<![CDATA[") {
            after(text, start + 9, b"]]>")
        } else if rest.starts_with(b"<?") {
            after(text, start + 2, b"?>")
        } else if rest.starts_with(b"<!") {
            declaration_end(text, start + 2)
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            tag_end(text, start + 2).0
        } else {
            let (end, empty) = tag_end(text, start + 1);
            if !empty {
                depth += 1;
                if depth > limit {
                    return false;
                }
            }
            end
        };
    }
    true
}

/// Whether no element of the parsed `document` is nested deeper than `limit`, the root counted
/// and, as in [`depth_within`], an element without content (`<rect/>`) not: one pass in document
/// order over the path of open elements, never longer than `limit` itself.
#[must_use]
pub fn tree_within(document: &Document, limit: usize) -> bool {
    let mut open: Vec<NodeId> = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.is_element() && node.has_children())
    {
        let parent = node.parent().map(|parent| parent.id());
        while open.last().is_some_and(|last| Some(*last) != parent) {
            open.pop();
        }
        open.push(node.id());
        if open.len() > limit {
            return false;
        }
    }
    true
}

/// The offset of the first `pattern` at or after `from`.
fn find(text: &[u8], from: usize, pattern: &[u8]) -> Option<usize> {
    text.get(from..)?
        .windows(pattern.len())
        .position(|window| window == pattern)
        .map(|offset| from + offset)
}

/// The offset after the first `pattern` at or after `from`; the end of `text` without one.
pub(crate) fn after(text: &[u8], from: usize, pattern: &[u8]) -> usize {
    find(text, from, pattern).map_or(text.len(), |at| at + pattern.len())
}

/// The offset after the `>` that ends a tag whose name starts at `from`, quoted values skipped,
/// and whether the tag is empty (`/>`).
fn tag_end(text: &[u8], from: usize) -> (usize, bool) {
    let mut quote = None;
    let mut last = 0u8;
    for (offset, &byte) in text.iter().enumerate().skip(from) {
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return (offset + 1, last == b'/');
        }
        if !byte.is_ascii_whitespace() {
            last = byte;
        }
    }
    (text.len(), false)
}

/// The offset after the `>` that ends a `<!...` declaration starting at `from`: brackets of an
/// internal subset, quoted literals and comments inside it skipped.
pub(crate) fn declaration_end(text: &[u8], from: usize) -> usize {
    let mut quote = None;
    let mut brackets = 0usize;
    let mut at = from;
    while let Some(&byte) = text.get(at) {
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
        } else if text[at..].starts_with(b"<!--") {
            at = after(text, at + 4, b"-->");
            continue;
        } else {
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'>' if brackets == 0 => return at + 1,
                _ => {}
            }
        }
        at += 1;
    }
    text.len()
}

#[cfg(test)]
#[path = "depth_tests.rs"]
mod tests;
