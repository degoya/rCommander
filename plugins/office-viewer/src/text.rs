//! The `text` output of each kind of document: a Word body as its paragraphs and tables, a
//! workbook sheet by sheet - the sheet's name on a line of its own, then its rows - and a
//! presentation slide by slide under a `Slide n` line. Sheets and slides are separated by an
//! empty line.

use crate::flow::Flow;
use crate::out::Lines;
use crate::package::Package;
use crate::sheet::{self, SharedStrings};
use crate::slides;
use crate::types::{Failure, Source};
use crate::xml::Ev;

/// A sheet or a slide: its name (a sheet's) and its part, `None` when its relationship is
/// missing or points outside the package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) name: String,
    pub(crate) part: Option<String>,
}

/// The body of `main`, a Word document.
pub(crate) fn word<S: Source>(
    package: &mut Package<S>,
    main: &str,
    out: &mut dyn Lines,
) -> Result<(), Failure> {
    let Some(mut xml) = package.xml(main)? else {
        return Ok(());
    };
    let mut flow = Flow::default();
    loop {
        let ev = xml.next()?;
        if matches!(ev, Ev::Eof) || out.full() {
            break;
        }
        flow.event(&ev, out)?;
    }
    flow.finish(out)
}

/// The sheets of a workbook, each with at most `max_rows` rows below its header.
pub(crate) fn sheets<S: Source>(
    package: &mut Package<S>,
    sections: &[Section],
    shared: Option<&str>,
    max_rows: u64,
    out: &mut dyn Lines,
) -> Result<(), Failure> {
    let strings = match shared {
        Some(part) => match package.xml(part)? {
            Some(mut xml) => sheet::shared_strings(&mut xml)?,
            None => SharedStrings::default(),
        },
        None => SharedStrings::default(),
    };
    for (index, section) in sections.iter().enumerate() {
        if out.full() {
            break;
        }
        if index > 0 {
            out.push("\n")?;
        }
        out.push(&format!("{}\n", section.name))?;
        let Some(part) = section.part.as_deref() else {
            continue;
        };
        if let Some(mut xml) = package.xml(part)? {
            sheet::write(&mut xml, &strings, max_rows, out)?;
        }
    }
    Ok(())
}

/// The slides of a presentation, in its order.
pub(crate) fn slides<S: Source>(
    package: &mut Package<S>,
    sections: &[Section],
    out: &mut dyn Lines,
) -> Result<(), Failure> {
    for (index, section) in sections.iter().enumerate() {
        if out.full() {
            break;
        }
        if index > 0 {
            out.push("\n")?;
        }
        out.push(&format!("Slide {}\n", index + 1))?;
        let Some(part) = section.part.as_deref() else {
            continue;
        };
        if let Some(mut xml) = package.xml(part)? {
            slides::slide(&mut xml, out)?;
        }
    }
    Ok(())
}
