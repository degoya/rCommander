use super::*;
use crate::depth::MAX_DEPTH;
use crate::test_support::{TestSource, VecSink, gzip, gzip_zeros, nested};
use crate::types::{FailureKind, TokenClass};

/// A small picture with every XML token class the source shows: a declaration, a comment,
/// elements, attributes and an entity.
const ICON: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- an icon -->
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50">
  <title>Ticks &amp; crosses</title>
  <rect x="0" y="0" width="50" height="50" fill="#ff0000"/>
</svg>
"##;

fn open(bytes: &[u8]) -> Viewer {
    Viewer::open(TestSource::new(bytes.to_vec()), Settings::default())
        .expect("the document opens")
        .0
}

fn text_of(viewer: &mut Viewer) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

/// What the XML viewer itself answers for `bytes`: its text and the tokens of every line.
fn xml_viewer(bytes: &[u8]) -> (String, Vec<Vec<Token>>) {
    let (mut viewer, _) = XmlViewer::open(TestSource::new(bytes.to_vec()), Format::AsIs)
        .expect("the XML viewer opens it");
    let mut sink = VecSink::default();
    viewer.text(&mut sink).expect("text");
    let lines = u32::try_from(viewer.line_count()).expect("a few lines");
    let tokens = viewer.tokens(0, lines).expect("tokens");
    (String::from_utf8(sink.0).expect("UTF-8"), tokens)
}

#[test]
fn a_document_offers_the_picture_then_the_source() {
    let (_, document) = Viewer::open(
        TestSource::new(ICON.as_bytes().to_vec()),
        Settings::default(),
    )
    .expect("opens");
    assert_eq!(document.outputs, vec![Output::Image, Output::Highlighted]);
    assert_eq!(document.title, None);
    assert_eq!(document.pages, None);
}

#[test]
fn the_image_is_a_raw_raster_of_the_named_size() {
    let mut viewer = open(ICON.as_bytes());
    let mut sink = VecSink::default();
    assert_eq!(viewer.image(&mut sink).expect("image"), "rgba:1024x512");
    assert_eq!(sink.0.len(), 1024 * 512 * 4);
    // The left half is the red square, the right half transparent.
    assert_eq!(&sink.0[..4], &[255, 0, 0, 255]);
    let last = sink.0.len() - 4;
    assert_eq!(&sink.0[last..], &[0, 0, 0, 0]);
}

#[test]
fn the_settings_of_open_apply_to_the_image() {
    let settings = Settings::from_values(Some("512"), Some("white"));
    let (mut viewer, _) =
        Viewer::open(TestSource::new(ICON.as_bytes().to_vec()), settings).expect("opens");
    let mut sink = VecSink::default();
    assert_eq!(viewer.image(&mut sink).expect("image"), "rgba:512x256");
    let last = sink.0.len() - 4;
    assert_eq!(&sink.0[last..], &[255, 255, 255, 255]);
}

#[test]
fn the_source_is_coloured_as_the_xml_viewer_colours_it() {
    let mut viewer = open(ICON.as_bytes());
    let (text, tokens) = xml_viewer(ICON.as_bytes());
    assert_eq!(text_of(&mut viewer), text);
    assert_eq!(viewer.tokens(0, 100).expect("tokens"), tokens);
    let classes: Vec<TokenClass> = tokens.iter().flatten().map(|token| token.class).collect();
    for class in [
        TokenClass::Keyword,
        TokenClass::Comment,
        TokenClass::TypeName,
        TokenClass::Key,
        TokenClass::String,
        TokenClass::Literal,
        TokenClass::Punctuation,
    ] {
        assert!(classes.contains(&class), "{class:?}");
    }
    // A window in the middle is the same lines.
    assert_eq!(viewer.tokens(2, 2).expect("tokens"), tokens[2..4].to_vec());
}

#[test]
fn invalid_utf8_keeps_the_colours_on_their_characters() {
    let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\">\n<text>\xff\xfe</text><rect x=\"1\"/>\n</svg>\n";
    let mut viewer = open(bytes);
    let text = text_of(&mut viewer);
    let line = text.lines().nth(1).expect("the second line");
    assert_eq!(line, "<text>\u{fffd}\u{fffd}</text><rect x=\"1\"/>");
    let tokens = viewer.tokens(1, 1).expect("tokens").remove(0);
    for token in &tokens {
        let span = token.start as usize..(token.start + token.len) as usize;
        assert!(
            line.get(span).is_some(),
            "{token:?} on character boundaries"
        );
    }
    let rect = line.find("rect").expect("rect") as u32;
    assert!(
        tokens.iter().any(|token| token.start == rect
            && token.len == 4
            && token.class == TokenClass::TypeName),
        "{tokens:?}"
    );
}

#[test]
fn svgz_is_inflated_for_the_picture_and_the_source() {
    let plain = open(ICON.as_bytes());
    let mut packed = open(&gzip(ICON.as_bytes()));
    assert_eq!(packed.svg(), plain.svg());
    assert_eq!(text_of(&mut packed), ICON);
    let mut sink = VecSink::default();
    assert_eq!(packed.image(&mut sink).expect("image"), "rgba:1024x512");
}

#[test]
fn svgz_over_64_mib_inflated_does_not_open() {
    let bomb = gzip_zeros(MAX_SVG_BYTES + 1);
    let failure = Viewer::open(TestSource::new(bomb), Settings::default())
        .err()
        .expect("refused");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code::TOO_LARGE));
}

#[test]
fn a_document_nested_too_deep_is_refused_not_overflowed() {
    let deep = nested(100_000);
    let mut viewer = open(deep.as_bytes());
    let failure = viewer.image(&mut VecSink::default()).expect_err("too deep");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code::TOO_COMPLEX));
    // The source still shows.
    assert_eq!(text_of(&mut viewer), deep);
}

#[test]
fn a_document_at_the_depth_budget_is_drawn() {
    let mut viewer = open(nested(MAX_DEPTH).as_bytes());
    let mut sink = VecSink::default();
    assert_eq!(viewer.image(&mut sink).expect("image"), "rgba:1024x1024");
    assert_eq!(&sink.0[..4], &[255, 0, 0, 255]);
}

#[test]
fn a_broken_picture_still_shows_its_source() {
    let broken = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect";
    let mut viewer = open(broken);
    let failure = viewer
        .image(&mut VecSink::default())
        .expect_err("no picture");
    assert_eq!(failure.code.as_deref(), Some(code::NOT_SVG));
    assert_eq!(text_of(&mut viewer).as_bytes(), broken);
}

#[test]
fn probe_takes_svg_and_svgz_and_declines_other_xml() {
    let probe_of = |bytes: &[u8]| probe(&mut TestSource::new(bytes.to_vec()));
    assert!(probe_of(ICON.as_bytes()));
    assert!(probe_of(&gzip(ICON.as_bytes())));
    let mut late = format!("<!-- {} -->", "x".repeat(10_000)).into_bytes();
    late.extend_from_slice(b"<svg/>");
    assert!(probe_of(&late));
    assert!(!probe_of(b"<?xml version=\"1.0\"?><project/>"));
    assert!(!probe_of(b""));
}
