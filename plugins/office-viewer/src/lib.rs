//! rCommander's bundled Office viewer (RC-100-99): the text of a Word document, workbook or
//! presentation (`text`, OOXML: `docx`, `docm`, `xlsx`, `xlsm`, `pptx`, `pptm`), or its
//! properties with its sheets or slides as a tree (`tree`), the text first.
//!
//! Built on what the host promises a viewer (`docs/plugins.md`, "Viewer (WLX)"):
//!
//! - **The file is a handle, read in windows.** The package is a zip read through `read-at` by
//!   a reader of the plugin's own (`package`): its end record, its central directory and the
//!   XML parts a view needs, stored or Deflate - never a picture, a video or an embedded
//!   object.
//! - **Every part through its relationships.** The main part is what `_rels/.rels` names, the
//!   sheets and slides what the workbook's and the presentation's relationships name, in the
//!   order `workbook.xml` and `presentation.xml` list them (`rels`). A relationship with
//!   `TargetMode="External"` is dropped and a target outside the package never opened.
//! - **No macros.** No part named `*.bin` is ever opened, `vbaProject.bin` among them
//!   (`package`).
//! - **Bounded work** ([`Limits`]): at most 10 000 entries, counted on the end record before the
//!   central directory is read; each part at most 64 MiB inflated, counted on the bytes actually
//!   inflated, and at most 500 times its packed size once past 1 MiB; XML at most 256 elements
//!   deep, no entity expanded beyond the five predefined ones; the text at most 256 MiB. Each
//!   breach is `bad-data` with its own code ([`code`]).
//! - **Moving on cancels.** Every window asks `should-stop` first.
//!
//! The setting `sheet_rows` ([`settings`]) is read once, when a document opens. The layout is
//! the other bundled viewers': the logic here, tested natively with `cargo test`; the WIT
//! translation in `src/guest.rs`, only on `wasm32`.

mod central;
mod entries;
mod flow;
#[cfg(target_arch = "wasm32")]
mod guest;
mod limits;
mod out;
mod package;
mod props;
mod rels;
pub mod settings;
mod sheet;
mod slides;
mod source_io;
mod text;
mod tree;
pub mod types;
mod xml;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod test_support;

pub use limits::{Limits, code};
pub use tree::Kind;

use out::TextOut;
use package::Package;
use rels::Rel;
use settings::Settings;
use source_io::damaged;
use text::Section;
use tree::Tree;
use types::{Document, Failure, FailureKind, Output, Sink, Source, TreeNode};
use xml::{Ev, attr, local, prefixed_attr};

/// The part every OOXML package has: what makes a zip a package.
const CONTENT_TYPES: &str = "[Content_Types].xml";

/// Whether the file is an Office package: a zip whose central directory has
/// `[Content_Types].xml`. A zip with more entries than [`Limits::max_entries`] is taken too, so
/// opening it tells the person why it is not shown.
pub fn probe<S: Source>(src: &mut S) -> bool {
    match entries::find(&mut *src) {
        Ok(Some(end)) if end.declared > Limits::DEFAULT.max_entries => true,
        Ok(Some(_)) => Package::open(&mut *src, Limits::DEFAULT)
            .is_ok_and(|package| package.contains(CONTENT_TYPES)),
        _ => false,
    }
}

/// One open document.
pub struct Viewer<S: Source> {
    package: Package<S>,
    kind: Kind,
    /// The main part: `word/document.xml`, `xl/workbook.xml`, `ppt/presentation.xml`.
    main: String,
    settings: Settings,
    limits: Limits,
    properties: Vec<(&'static str, String)>,
    /// The sheets of a workbook or the slides of a presentation, in order.
    sections: Vec<Section>,
    /// A workbook's shared strings part.
    shared: Option<String>,
    tree: Option<Tree>,
}

impl<S: Source> Viewer<S> {
    /// `open` within [`Limits::DEFAULT`]: offers `text`, then `tree`.
    pub fn open(src: S, settings: Settings) -> Result<(Self, Document), Failure> {
        Self::open_within(src, settings, Limits::DEFAULT)
    }

    /// `open` within `limits`: reads the package's relationships, its properties and the list
    /// of its sheets or slides; the text waits for `text`.
    pub fn open_within(
        src: S,
        settings: Settings,
        limits: Limits,
    ) -> Result<(Self, Document), Failure> {
        let mut package = Package::open(src, limits)?;
        if !package.contains(CONTENT_TYPES) {
            return Err(not_office());
        }
        let root = relationships(&mut package, "")?;
        let main = rels::by_kind(&root, "officeDocument")
            .ok_or_else(not_office)?
            .to_owned();
        let kind = kind_of(&mut package, &main)?;
        let mut properties = Vec::new();
        for (relationship, wanted) in [
            ("core-properties", props::CORE),
            ("extended-properties", props::APP),
        ] {
            if let Some(part) = rels::by_kind(&root, relationship)
                && let Some(mut xml) = package.xml(part)?
            {
                properties.extend(props::read(&mut xml, wanted)?);
            }
        }
        let (sections, shared) = match kind {
            Kind::Word => (Vec::new(), None),
            Kind::Excel => workbook(&mut package, &main)?,
            Kind::PowerPoint => (presentation(&mut package, &main)?, None),
        };
        let title = properties
            .iter()
            .find(|(label, _)| *label == "title")
            .map(|(_, value)| value.clone());
        let viewer = Self {
            package,
            kind,
            main,
            settings,
            limits,
            properties,
            sections,
            shared,
            tree: None,
        };
        Ok((
            viewer,
            Document {
                outputs: vec![Output::Text, Output::Tree],
                title,
                pages: None,
            },
        ))
    }

    /// What the document is.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// `text`: writes the text into `out`, as UTF-8, and names its encoding.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let mut lines = TextOut::new(out, self.limits.max_text_bytes);
        match self.kind {
            Kind::Word => text::word(&mut self.package, &self.main, &mut lines)?,
            Kind::Excel => text::sheets(
                &mut self.package,
                &self.sections,
                self.shared.as_deref(),
                self.settings.sheet_rows.rows(),
                &mut lines,
            )?,
            Kind::PowerPoint => text::slides(&mut self.package, &self.sections, &mut lines)?,
        }
        lines.finish()?;
        Ok("utf-8".to_owned())
    }

    /// `children`: the top level for `None`, else the children of the node with that id.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        let tree = match self.tree.take() {
            Some(tree) => tree,
            None => tree::build(
                &mut self.package,
                self.kind,
                &self.properties,
                &self.sections,
            )?,
        };
        let nodes = tree.children(node);
        self.tree = Some(tree);
        nodes
    }
}

/// The relationships of `part`; none when it has no relationships part.
fn relationships<S: Source>(package: &mut Package<S>, part: &str) -> Result<Vec<Rel>, Failure> {
    match package.xml(&rels::rels_name(part))? {
        Some(mut xml) => rels::read(&mut xml, part),
        None => Ok(Vec::new()),
    }
}

/// What the main part is, by its root element.
fn kind_of<S: Source>(package: &mut Package<S>, main: &str) -> Result<Kind, Failure> {
    let mut xml = package.xml(main)?.ok_or_else(not_office)?;
    loop {
        match xml.next()? {
            Ev::Start(start) | Ev::Empty(start) => {
                return match local(&start) {
                    b"document" => Ok(Kind::Word),
                    b"workbook" => Ok(Kind::Excel),
                    b"presentation" => Ok(Kind::PowerPoint),
                    _ => Err(not_office()),
                };
            }
            Ev::Eof => return Err(damaged()),
            _ => {}
        }
    }
}

/// The sheets of a workbook in its order, and its shared strings part.
fn workbook<S: Source>(
    package: &mut Package<S>,
    main: &str,
) -> Result<(Vec<Section>, Option<String>), Failure> {
    let mut sheets: Vec<(String, Option<String>)> = Vec::new();
    if let Some(mut xml) = package.xml(main)? {
        loop {
            match xml.next()? {
                Ev::Start(start) | Ev::Empty(start) if local(&start) == b"sheet" => {
                    let name = attr(&start, b"name").unwrap_or_default();
                    sheets.push((name, prefixed_attr(&start, b"id")));
                }
                Ev::Eof => break,
                _ => {}
            }
        }
    }
    let rels = relationships(package, main)?;
    let sections = sheets
        .into_iter()
        .map(|(name, id)| Section {
            name,
            part: id
                .as_deref()
                .and_then(|id| rels::by_id(&rels, id))
                .map(str::to_owned),
        })
        .collect();
    Ok((
        sections,
        rels::by_kind(&rels, "sharedStrings").map(str::to_owned),
    ))
}

/// The slides of a presentation in its order.
fn presentation<S: Source>(package: &mut Package<S>, main: &str) -> Result<Vec<Section>, Failure> {
    let ids = match package.xml(main)? {
        Some(mut xml) => slides::slide_ids(&mut xml)?,
        None => Vec::new(),
    };
    let rels = relationships(package, main)?;
    Ok(ids
        .iter()
        .map(|id| Section {
            name: String::new(),
            part: rels::by_id(&rels, id).map(str::to_owned),
        })
        .collect())
}

fn not_office() -> Failure {
    Failure::coded(FailureKind::BadData, code::NOT_OFFICE)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
