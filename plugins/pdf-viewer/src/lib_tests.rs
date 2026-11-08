//! The PDF viewer end to end on small documents: the text of two pages, the facts in the tree,
//! an encrypted document, a damaged one, one too large, and moving on.

use super::{MAX_PDF_BYTES, Viewer, code, probe};
use crate::test_support::{ENCRYPTED, TestSource, VecSink, pdf};
use crate::types::{Failure, FailureKind, Source};

fn two_pages() -> Vec<u8> {
    pdf(
        &[&["Hello world", "Second line"], &["Page two text"]],
        "/Title (Report) /Author (Ada) /CreationDate (D:20240501123456Z)",
        "",
    )
}

fn open(bytes: Vec<u8>) -> Result<Viewer<TestSource>, Failure> {
    Viewer::open(TestSource::new(bytes)).map(|(viewer, _)| viewer)
}

fn failure(bytes: Vec<u8>) -> Failure {
    match open(bytes) {
        Ok(_) => panic!("the document opened"),
        Err(failure) => failure,
    }
}

#[test]
fn probe_wants_the_pdf_header() {
    assert!(probe(&mut TestSource::new(two_pages())));
    assert!(probe(&mut TestSource::new(b"\r\n%PDF-1.7\n".to_vec())));
    assert!(!probe(&mut TestSource::new(
        b"PK\x03\x04 not a pdf".to_vec()
    )));
    assert!(!probe(&mut TestSource::new(Vec::new())));
}

#[test]
fn the_text_comes_page_by_page() {
    let (mut viewer, document) = Viewer::open(TestSource::new(two_pages())).expect("open");
    assert_eq!(document.title.as_deref(), Some("Report"));
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    let text = String::from_utf8(sink.0).expect("UTF-8");
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} in {text:?}"))
    };
    assert!(at("Page 1") < at("Hello world"));
    assert!(at("Hello world") < at("Second line"));
    assert!(at("Second line") < at("Page 2"));
    assert!(at("Page 2") < at("Page two text"));
    assert!(text.contains("Hello world\nSecond line\n"), "{text:?}");
}

#[test]
fn the_tree_holds_the_documents_facts() {
    let mut viewer = open(two_pages()).expect("open");
    let facts: Vec<(&str, String)> = viewer
        .facts()
        .iter()
        .map(|(label, value)| (*label, value.clone()))
        .collect();
    for wanted in [
        ("title", "Report"),
        ("author", "Ada"),
        ("created", "2024-05-01 12:34:56"),
        ("pages", "2"),
        ("version", "1.4"),
        ("encrypted", "no"),
        ("page size", "595 x 842 pt (210 x 297 mm)"),
    ] {
        assert!(
            facts
                .iter()
                .any(|(label, value)| (*label, value.as_str()) == wanted),
            "{wanted:?} in {facts:?}"
        );
    }
    let nodes = viewer.children(None).expect("top level");
    assert_eq!(nodes.len(), facts.len());
    assert!(nodes.iter().all(|node| !node.has_children));
    assert_eq!(nodes[0].id, 1);
    assert!(viewer.children(Some(1)).expect("a leaf").is_empty());
}

#[test]
fn a_document_with_a_user_password_asks_for_it() {
    let bytes = pdf(&[&["secret"]], "/Title (x)", ENCRYPTED);
    let failure = failure(bytes);
    assert_eq!(failure.kind, FailureKind::WrongPassword);
    assert_eq!(failure.code.as_deref(), Some(code::PASSWORD_REQUIRED));
}

#[test]
fn a_damaged_file_is_bad_data() {
    let failure = failure(b"not a pdf at all, just text".to_vec());
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code::DAMAGED));
}

/// A file larger than the limit, never read.
struct Huge;

impl Source for Huge {
    fn size(&self) -> u64 {
        MAX_PDF_BYTES + 1
    }

    fn read_at(&mut self, _offset: u64, _length: u32) -> Result<Vec<u8>, Failure> {
        panic!("a file over the limit is not read");
    }
}

#[test]
fn a_file_over_the_limit_is_not_read() {
    let Err(failure) = Viewer::open(Huge) else {
        panic!("opened");
    };
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code::TOO_LARGE));
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(two_pages());
    src.stop_after = Some(0);
    let Err(failure) = Viewer::open(src) else {
        panic!("opened");
    };
    assert_eq!(failure.kind, FailureKind::Cancelled);
}
