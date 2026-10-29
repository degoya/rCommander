//! The component (RC-100-11, built with RC-100-12): `src/lib.rs` translated into the viewer world
//! of `crates/rc-plugin-api/wit/rcommander.wit`. Built only for `wasm32`; every type of
//! [`crate::types`] maps to its WIT twin one to one, so nothing here decides anything and it
//! needs no test of its own - the conformance run of `scripts/build-plugins.sh` asks the built
//! component.
//!
//! The layout is rDownloader's `sdk/templates/*/src/guest.rs` at commit 26c8592a6.
#![allow(
    unsafe_code,
    reason = "the canonical-ABI glue wit-bindgen generates is the only unsafe code here"
)]

wit_bindgen::generate!({
    path: "../../crates/rc-plugin-api/wit",
    world: "viewer-plugin",
});

use std::cell::RefCell;

use exports::rcommander::plugin::viewer::{self as wit, Guest};
use rcommander::plugin::{settings, sink, source, types as wit_types};

use crate::Viewer;
use crate::types::{
    Document, Failure, FailureKind, Format, Output, Sink, Source, Token, TokenClass, TreeNode,
};

struct Component;

thread_local! {
    /// The document `open` read: a store holds one, for the calls after `open`.
    static DOCUMENT: RefCell<Option<Viewer<Handle>>> = const { RefCell::new(None) };
}

/// The file behind a `source` handle, valid for the life of the store; its size asked once.
struct Handle {
    src: wit_types::Source,
    size: u64,
}

impl Handle {
    fn new(src: wit_types::Source) -> Self {
        Self {
            src,
            size: source::info(src).size,
        }
    }
}

impl Source for Handle {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        source::read_at(self.src, offset, length).map_err(from_wit)
    }

    fn should_stop(&mut self) -> bool {
        source::should_stop()
    }

    fn progress(&mut self, done: u64, total: Option<u64>) {
        source::progress(done, total);
    }
}

/// The sink of one `text` call.
struct Out(wit_types::Sink);

impl Sink for Out {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        sink::write_at(self.0, offset, bytes).map_err(from_wit)
    }
}

/// Runs `call` on the open document; `not-supported` before `open`.
fn with_document<T>(
    call: impl FnOnce(&mut Viewer<Handle>) -> Result<T, Failure>,
) -> Result<T, wit::Failure> {
    DOCUMENT.with(|slot| match slot.borrow_mut().as_mut() {
        Some(viewer) => call(viewer).map_err(to_wit),
        None => Err(to_wit(Failure::of(FailureKind::NotSupported))),
    })
}

impl Guest for Component {
    fn probe(src: wit_types::Source) -> bool {
        crate::probe(&mut Handle::new(src))
    }

    // The same document for the Lister and Quick View, and no colours of its own: a token names
    // a class, the theme colours it. The setting `format` is read once, when the document opens
    // (contract 0.2.0, RC-100-14): `pretty` indents, anything else is the file's own text.
    fn open(
        src: wit_types::Source,
        _purpose: wit::Purpose,
        _theme: wit::Theme,
    ) -> Result<wit::Document, wit::Failure> {
        let format = Format::from_setting(settings::setting("format").as_deref());
        let (viewer, document) = Viewer::open(Handle::new(src), format).map_err(to_wit)?;
        DOCUMENT.with(|slot| *slot.borrow_mut() = Some(viewer));
        Ok(document_to_wit(document))
    }

    fn text(out: wit_types::Sink) -> Result<String, wit::Failure> {
        with_document(|viewer| viewer.text(&mut Out(out)))
    }

    fn tokens(first_line: u32, count: u32) -> Result<Vec<Vec<wit::Token>>, wit::Failure> {
        with_document(|viewer| viewer.tokens(first_line, count)).map(|lines| {
            lines
                .into_iter()
                .map(|line| line.into_iter().map(token_to_wit).collect())
                .collect()
        })
    }

    fn children(node: Option<u64>) -> Result<Vec<wit::TreeNode>, wit::Failure> {
        with_document(|viewer| viewer.children(node))
            .map(|nodes| nodes.into_iter().map(node_to_wit).collect())
    }

    // The outputs this viewer never names. The host calls none of them for a document that does
    // not offer it; the answers are there because the world exports them.

    fn image(_out: wit_types::Sink) -> Result<String, wit::Failure> {
        Err(to_wit(Failure::of(FailureKind::NotSupported)))
    }

    fn page(
        _index: u32,
        _width: u32,
        _height: u32,
        _out: wit_types::Sink,
    ) -> Result<(), wit::Failure> {
        Err(to_wit(Failure::of(FailureKind::NotSupported)))
    }

    fn search(
        _needle: String,
        _from_page: u32,
        _backwards: bool,
    ) -> Result<Option<u32>, wit::Failure> {
        Err(to_wit(Failure::of(FailureKind::NotSupported)))
    }

    fn thumbnail(
        _src: wit_types::Source,
        _size: u32,
        _out: wit_types::Sink,
    ) -> Result<String, wit::Failure> {
        Err(to_wit(Failure::of(FailureKind::NotSupported)))
    }
}

fn document_to_wit(document: Document) -> wit::Document {
    wit::Document {
        outputs: document.outputs.into_iter().map(output_to_wit).collect(),
        title: document.title,
        pages: document.pages,
    }
}

fn output_to_wit(output: Output) -> wit::Output {
    match output {
        Output::Text => wit::Output::Text,
        Output::Highlighted => wit::Output::Highlighted,
        Output::Markdown => wit::Output::Markdown,
        Output::Image => wit::Output::Image,
        Output::Tree => wit::Output::Tree,
        Output::Pages => wit::Output::Pages,
    }
}

fn node_to_wit(node: TreeNode) -> wit::TreeNode {
    wit::TreeNode {
        id: node.id,
        label: node.label,
        value: node.value,
        has_children: node.has_children,
    }
}

fn token_to_wit(token: Token) -> wit::Token {
    wit::Token {
        start: token.start,
        len: token.len,
        class: match token.class {
            TokenClass::Keyword => wit::TokenClass::Keyword,
            TokenClass::Key => wit::TokenClass::Key,
            TokenClass::String => wit::TokenClass::String,
            TokenClass::Number => wit::TokenClass::Number,
            TokenClass::Literal => wit::TokenClass::Literal,
            TokenClass::Comment => wit::TokenClass::Comment,
            TokenClass::Punctuation => wit::TokenClass::Punctuation,
            TokenClass::Operator => wit::TokenClass::Operator,
            TokenClass::TypeName => wit::TokenClass::TypeName,
            TokenClass::Function => wit::TokenClass::Function,
            TokenClass::Invalid => wit::TokenClass::Invalid,
        },
    }
}

fn to_wit(failure: Failure) -> wit::Failure {
    wit::Failure {
        kind: match failure.kind {
            FailureKind::NotFound => wit_types::FailureKind::NotFound,
            FailureKind::Exists => wit_types::FailureKind::Exists,
            FailureKind::AccessDenied => wit_types::FailureKind::AccessDenied,
            FailureKind::NotSupported => wit_types::FailureKind::NotSupported,
            FailureKind::BadData => wit_types::FailureKind::BadData,
            FailureKind::WrongPassword => wit_types::FailureKind::WrongPassword,
            FailureKind::Cancelled => wit_types::FailureKind::Cancelled,
            FailureKind::Io => wit_types::FailureKind::Io,
            FailureKind::Other => wit_types::FailureKind::Other,
        },
        code: failure.code,
        params: failure.params,
    }
}

/// A failure of the host's streams, as the logic sees one: its class and code go on (the host
/// keeps only a code in the plugin's own namespace when it comes back).
fn from_wit(failure: wit_types::Failure) -> Failure {
    Failure {
        kind: match failure.kind {
            wit_types::FailureKind::NotFound => FailureKind::NotFound,
            wit_types::FailureKind::Exists => FailureKind::Exists,
            wit_types::FailureKind::AccessDenied => FailureKind::AccessDenied,
            wit_types::FailureKind::NotSupported => FailureKind::NotSupported,
            wit_types::FailureKind::BadData => FailureKind::BadData,
            wit_types::FailureKind::WrongPassword => FailureKind::WrongPassword,
            wit_types::FailureKind::Cancelled => FailureKind::Cancelled,
            wit_types::FailureKind::Io => FailureKind::Io,
            wit_types::FailureKind::Other => FailureKind::Other,
        },
        code: failure.code,
        params: failure.params,
    }
}

export!(Component);
