//! PresentationML: the slides in the order `presentation.xml` lists them (`sldIdLst`, each
//! `sldId` naming its part by `r:id`), never the order of the part names, and a slide's text
//! frames and tables as [`crate::flow`] reads them, with the slide's title - the text of the shape
//! whose placeholder is `title` or `ctrTitle`.

use std::io::Read;

use crate::flow::Flow;
use crate::out::Lines;
use crate::types::Failure;
use crate::xml::{Ev, Xml, attr, local, local_end, prefixed_attr};

/// The longest title kept, in characters.
const MAX_TITLE_CHARS: usize = 256;

/// The relationship ids of the slides, in the presentation's order.
pub(crate) fn slide_ids<R: Read>(xml: &mut Xml<R>) -> Result<Vec<String>, Failure> {
    let mut ids = Vec::new();
    loop {
        match xml.next()? {
            Ev::Start(start) | Ev::Empty(start) if local(&start) == b"sldId" => {
                if let Some(id) = prefixed_attr(&start, b"id") {
                    ids.push(id);
                }
            }
            Ev::Eof => return Ok(ids),
            _ => {}
        }
    }
}

/// Writes the text of one slide into `out` and returns its title.
pub(crate) fn slide<R: Read>(
    xml: &mut Xml<R>,
    out: &mut dyn Lines,
) -> Result<Option<String>, Failure> {
    let mut flow = Flow::default();
    let mut shapes = 0usize;
    let mut is_title = false;
    let mut in_text = false;
    let mut text = String::new();
    let mut title: Option<String> = None;
    loop {
        let ev = xml.next()?;
        match &ev {
            Ev::Start(start) => match local(start) {
                b"sp" => {
                    shapes += 1;
                    if shapes == 1 {
                        is_title = false;
                        text.clear();
                    }
                }
                b"ph" => is_title |= shapes > 0 && is_title_placeholder(start),
                b"t" => in_text = true,
                _ => {}
            },
            Ev::Empty(start) => {
                if local(start) == b"ph" {
                    is_title |= shapes > 0 && is_title_placeholder(start);
                }
            }
            Ev::End(end) => match local_end(end) {
                b"t" => in_text = false,
                b"sp" => {
                    shapes = shapes.saturating_sub(1);
                    let found = text.trim();
                    if shapes == 0 && is_title && title.is_none() && !found.is_empty() {
                        title = Some(found.chars().take(MAX_TITLE_CHARS).collect());
                    }
                }
                _ => {}
            },
            Ev::Text(piece) => {
                if in_text && is_title && title.is_none() {
                    text.push_str(piece);
                }
            }
            Ev::Eof => break,
            Ev::Other => {}
        }
        if !out.full() {
            flow.event(&ev, out)?;
        }
    }
    flow.finish(out)?;
    Ok(title)
}

/// Whether a `ph` names the slide's title.
fn is_title_placeholder(start: &quick_xml::events::BytesStart<'_>) -> bool {
    matches!(attr(start, b"type").as_deref(), Some("title" | "ctrTitle"))
}

#[cfg(test)]
#[path = "slides_tests.rs"]
mod tests;
