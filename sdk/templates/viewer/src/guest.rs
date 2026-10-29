//! The component: `src/lib.rs` translated into the vocabulary of `wit/rcommander.wit`. Built
//! only for `wasm32`; it holds no logic of its own, so it needs no test of its own.
#![allow(
    unsafe_code,
    reason = "the canonical-ABI glue wit-bindgen generates is the only unsafe code here"
)]

wit_bindgen::generate!({
    path: "wit",
    world: "viewer-plugin",
});

use std::cell::RefCell;

use exports::rcommander::plugin::viewer::{self as wit, Guest};
use rcommander::plugin::{settings, sink, source, types as wit_types};

use crate::Viewer;
use crate::types::{Document, Failure, FailureKind, Output, Sink, Source};

struct Component;

thread_local! {
    /// The document `open` read: a store holds one, for the calls after `open`.
    static DOCUMENT: RefCell<Option<Viewer<Handle>>> = const { RefCell::new(None) };
}

/// The file behind a `source` handle, valid for the life of the store.
struct Handle(wit_types::Source);

impl Source for Handle {
    fn name(&self) -> String {
        source::info(self.0).name
    }

    fn size(&self) -> u64 {
        source::info(self.0).size
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        source::read_at(self.0, offset, length).map_err(from_wit)
    }

    fn should_stop(&mut self) -> bool {
        source::should_stop()
    }
}

/// The sink of one `text` call.
struct Out(wit_types::Sink);

impl Sink for Out {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        sink::write_at(self.0, offset, bytes).map_err(from_wit)
    }
}

impl Guest for Component {
    fn probe(src: wit_types::Source) -> bool {
        crate::probe(&mut Handle(src))
    }

    // The same document for the Lister and Quick View, and no colours of its own: the viewer
    // renders the Markdown in rCommander's theme. The setting is read once, when the document
    // opens: what the person chose, else the manifest's default.
    fn open(
        src: wit_types::Source,
        _purpose: wit::Purpose,
        _theme: wit::Theme,
    ) -> Result<wit::Document, wit::Failure> {
        let chosen = crate::table::chosen_separator(settings::setting("separator").as_deref());
        let (viewer, document) = Viewer::open(Handle(src), chosen).map_err(to_wit)?;
        DOCUMENT.with(|slot| *slot.borrow_mut() = Some(viewer));
        Ok(document_to_wit(document))
    }

    fn text(out: wit_types::Sink) -> Result<String, wit::Failure> {
        DOCUMENT.with(|slot| match slot.borrow_mut().as_mut() {
            Some(viewer) => viewer.text(&mut Out(out)).map_err(to_wit),
            None => Err(not_supported()),
        })
    }

    // The outputs this viewer never names. The host calls none of them for a document that does
    // not offer it; the answers are there because the world exports them.

    fn tokens(_first_line: u32, _count: u32) -> Result<Vec<Vec<wit::Token>>, wit::Failure> {
        Err(not_supported())
    }

    fn image(_out: wit_types::Sink) -> Result<String, wit::Failure> {
        Err(not_supported())
    }

    fn page(
        _index: u32,
        _width: u32,
        _height: u32,
        _out: wit_types::Sink,
    ) -> Result<(), wit::Failure> {
        Err(not_supported())
    }

    fn children(_node: Option<u64>) -> Result<Vec<wit::TreeNode>, wit::Failure> {
        Err(not_supported())
    }

    fn search(
        _needle: String,
        _from_page: u32,
        _backwards: bool,
    ) -> Result<Option<u32>, wit::Failure> {
        Err(not_supported())
    }

    fn thumbnail(
        _src: wit_types::Source,
        _size: u32,
        _out: wit_types::Sink,
    ) -> Result<String, wit::Failure> {
        Err(not_supported())
    }
}

fn document_to_wit(document: Document) -> wit::Document {
    wit::Document {
        outputs: document
            .outputs
            .into_iter()
            .map(|output| match output {
                Output::Text => wit::Output::Text,
                Output::Markdown => wit::Output::Markdown,
            })
            .collect(),
        title: document.title,
        pages: None,
    }
}

fn to_wit(failure: Failure) -> wit::Failure {
    wit::Failure {
        kind: match failure.kind {
            FailureKind::BadData => wit_types::FailureKind::BadData,
            FailureKind::Cancelled => wit_types::FailureKind::Cancelled,
            FailureKind::Io => wit_types::FailureKind::Io,
        },
        code: failure.code,
        params: Vec::new(),
    }
}

/// A failure of the host's streams. Its code is the host's, which a plugin cannot answer with,
/// so only the class goes on.
fn from_wit(failure: wit_types::Failure) -> Failure {
    Failure::of(match failure.kind {
        wit_types::FailureKind::Cancelled => FailureKind::Cancelled,
        wit_types::FailureKind::BadData => FailureKind::BadData,
        _ => FailureKind::Io,
    })
}

fn not_supported() -> wit::Failure {
    wit::Failure {
        kind: wit_types::FailureKind::NotSupported,
        code: None,
        params: Vec::new(),
    }
}

export!(Component);
