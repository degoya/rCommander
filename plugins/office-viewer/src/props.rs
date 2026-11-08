//! The document properties of `docProps/core.xml` (Dublin Core: title, subject, creator, ...)
//! and `docProps/app.xml` (the application and its counts), each a child of the root element.

use std::io::Read;

use crate::types::Failure;
use crate::xml::{Ev, Xml, local};

/// The longest value kept, in bytes; a longer one is cut on a character boundary.
const MAX_VALUE_BYTES: usize = 1024;

/// The core properties shown, by local name, with their labels in the tree.
pub(crate) const CORE: &[(&[u8], &str)] = &[
    (b"title", "title"),
    (b"subject", "subject"),
    (b"creator", "creator"),
    (b"lastModifiedBy", "last modified by"),
    (b"created", "created"),
    (b"modified", "modified"),
];

/// The extended properties shown. A presentation's slide count is the value of its `slides`
/// node, so `Slides` is not repeated here.
pub(crate) const APP: &[(&[u8], &str)] = &[
    (b"Application", "application"),
    (b"Pages", "pages"),
    (b"Words", "words"),
];

/// The properties of `wanted` the part holds, in the order of `wanted`, each with its label.
pub(crate) fn read<R: Read>(
    xml: &mut Xml<R>,
    wanted: &[(&[u8], &'static str)],
) -> Result<Vec<(&'static str, String)>, Failure> {
    let mut values: Vec<Option<String>> = vec![None; wanted.len()];
    let mut depth = 0usize;
    let mut current: Option<usize> = None;
    let mut value = String::new();
    loop {
        match xml.next()? {
            Ev::Start(start) => {
                depth += 1;
                if depth == 2 {
                    current = wanted.iter().position(|(name, _)| *name == local(&start));
                    value.clear();
                }
            }
            Ev::End(_) => {
                if depth == 2
                    && let Some(index) = current.take()
                {
                    let text = value.trim();
                    if !text.is_empty()
                        && let Some(slot) = values.get_mut(index)
                    {
                        *slot = Some(text.to_owned());
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Ev::Text(text) => {
                if current.is_some() && value.len() < MAX_VALUE_BYTES {
                    push_capped(&mut value, &text);
                }
            }
            Ev::Eof => break,
            Ev::Empty(_) | Ev::Other => {}
        }
    }
    Ok(wanted
        .iter()
        .zip(values)
        .filter_map(|((_, label), value)| value.map(|value| (*label, value)))
        .collect())
}

/// Appends `text` to `value` up to [`MAX_VALUE_BYTES`].
fn push_capped(value: &mut String, text: &str) {
    let mut end = MAX_VALUE_BYTES.saturating_sub(value.len()).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    value.push_str(text.get(..end).unwrap_or_default());
}

#[cfg(test)]
#[path = "props_tests.rs"]
mod tests;
