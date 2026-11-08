//! Open Packaging relationships: which part a `_rels/*.rels` part points to, by id or by type.
//! A relationship with `TargetMode="External"` (a link, a linked picture, an attached template)
//! is dropped as it is read; a target is resolved against the part's folder and dropped when it
//! names a scheme or climbs out of the package, so only parts inside it are ever opened.

use std::io::Read;

use crate::types::Failure;
use crate::xml::{Ev, Xml, attr, local};

/// One relationship, its target a part name inside the package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rel {
    pub(crate) id: String,
    /// The last segment of `Type`: `officeDocument`, `worksheet`, `slide`, `core-properties`.
    pub(crate) kind: String,
    pub(crate) target: String,
}

/// The relationships part of `part`: `word/_rels/document.xml.rels` for `word/document.xml`,
/// `_rels/.rels` for the package (`""`).
pub(crate) fn rels_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Reads the relationships of `part` from its relationships part `xml`.
pub(crate) fn read<R: Read>(xml: &mut Xml<R>, part: &str) -> Result<Vec<Rel>, Failure> {
    let folder = part.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut rels = Vec::new();
    loop {
        match xml.next()? {
            Ev::Start(start) | Ev::Empty(start) if local(&start) == b"Relationship" => {
                let external = attr(&start, b"TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                if external {
                    continue;
                }
                let (Some(id), Some(kind), Some(target)) = (
                    attr(&start, b"Id"),
                    attr(&start, b"Type"),
                    attr(&start, b"Target"),
                ) else {
                    continue;
                };
                if let Some(target) = resolve(folder, &target) {
                    let kind = kind
                        .rsplit_once('/')
                        .map_or(kind.as_str(), |(_, last)| last);
                    rels.push(Rel {
                        id,
                        kind: kind.to_owned(),
                        target,
                    });
                }
            }
            Ev::Eof => return Ok(rels),
            _ => {}
        }
    }
}

/// The target of the relationship `id`.
pub(crate) fn by_id<'r>(rels: &'r [Rel], id: &str) -> Option<&'r str> {
    rels.iter()
        .find(|rel| rel.id == id)
        .map(|rel| rel.target.as_str())
}

/// The target of the first relationship of `kind`.
pub(crate) fn by_kind<'r>(rels: &'r [Rel], kind: &str) -> Option<&'r str> {
    rels.iter()
        .find(|rel| rel.kind == kind)
        .map(|rel| rel.target.as_str())
}

/// The part name `target` names from `folder`; `None` for a URI with a scheme, a backslash, or
/// a path above the package's root.
pub(crate) fn resolve(folder: &str, target: &str) -> Option<String> {
    let target = percent_decoded(target)?;
    if target.contains(':') || target.contains('\\') {
        return None;
    }
    let target = target.split(['#', '?']).next().unwrap_or_default();
    let (mut segments, rest): (Vec<&str>, &str) = match target.strip_prefix('/') {
        Some(absolute) => (Vec::new(), absolute),
        None => (
            folder
                .split('/')
                .filter(|segment| !segment.is_empty())
                .collect(),
            target,
        ),
    };
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// `text` with its `%XX` escapes decoded; `None` for a broken escape or bytes that are not
/// UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    if !text.contains('%') {
        return Some(text.to_owned());
    }
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(byte);
            at += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
#[path = "rels_tests.rs"]
mod tests;
