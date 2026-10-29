use rc_plugin_api::viewer::{Output, TokenClass};

use super::{
    ImageFormat, MAX_PIXELS, MAX_TITLE_CHARS, MAX_TOKENS_PER_LINE, document, encoding, image,
    image_format, raster_bytes, tokens,
};
use crate::bindings::exports::rcommander::plugin::viewer as wit;
use crate::failure::codes;

fn raw(outputs: Vec<wit::Output>, title: Option<&str>, pages: Option<u32>) -> wit::Document {
    wit::Document {
        outputs,
        title: title.map(str::to_owned),
        pages,
    }
}

fn what(result: Result<impl std::fmt::Debug, rc_plugin_types::Failure>) -> String {
    let failure = result.expect_err("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_BAD_OUTPUT));
    failure.params.get("what").cloned().unwrap_or_default()
}

#[test]
fn outputs_are_not_empty_unique_and_in_the_manifest() {
    let offered = [Output::Highlighted, Output::Tree];
    let ok = document(
        raw(
            vec![wit::Output::Highlighted, wit::Output::Tree],
            None,
            None,
        ),
        &offered,
    )
    .expect("valid");
    assert_eq!(ok.outputs, offered);
    assert_eq!(
        what(document(raw(vec![], None, None), &offered)),
        "outputs_empty"
    );
    assert_eq!(
        what(document(
            raw(vec![wit::Output::Tree, wit::Output::Tree], None, None),
            &offered
        )),
        "outputs_duplicate"
    );
    assert_eq!(
        what(document(
            raw(vec![wit::Output::Image], None, None),
            &offered
        )),
        "output_not_in_manifest"
    );
}

#[test]
fn a_paged_document_names_its_page_count() {
    let offered = [Output::Pages];
    assert_eq!(
        what(document(
            raw(vec![wit::Output::Pages], None, None),
            &offered
        )),
        "page_count"
    );
    assert_eq!(
        what(document(
            raw(vec![wit::Output::Pages], None, Some(0)),
            &offered
        )),
        "page_count"
    );
    let paged = document(raw(vec![wit::Output::Pages], None, Some(12)), &offered).expect("ok");
    assert_eq!(paged.pages, Some(12));
    let text = document(
        raw(vec![wit::Output::Text], None, Some(12)),
        &[Output::Text],
    );
    assert_eq!(text.expect("ok").pages, None, "no pages, no count");
}

#[test]
fn a_title_is_spelled_out_and_capped() {
    let hostile = format!("evil\u{202e}txt.exe\u{7}{}", "x".repeat(1000));
    let shown = document(
        raw(vec![wit::Output::Text], Some(&hostile), None),
        &[Output::Text],
    )
    .expect("valid")
    .title
    .expect("a title");
    assert!(shown.starts_with("evil\\u{202e}txt.exe\\u{7}"), "{shown}");
    assert!(!shown.contains('\u{202e}') && !shown.contains('\u{7}'));
    assert!(shown.chars().count() <= MAX_TITLE_CHARS + 1 + 20);
}

#[test]
fn an_encoding_is_a_plain_label() {
    assert_eq!(encoding("UTF-8").expect("ok"), "utf-8");
    assert_eq!(encoding("windows-1252").expect("ok"), "windows-1252");
    let long = "a".repeat(33);
    for bad in ["", "utf 8", "../x", long.as_str()] {
        assert_eq!(what(encoding(bad)), "encoding", "{bad}");
    }
}

#[test]
fn image_formats_and_raster_sizes() {
    assert_eq!(image_format("png"), Some(ImageFormat::Png));
    assert_eq!(
        image_format("rgba:2x3"),
        Some(ImageFormat::Rgba {
            width: 2,
            height: 3
        })
    );
    for bad in [
        "PNG",
        "svg",
        "rgba:2",
        "rgba:x3",
        "rgba:+2x3",
        "rgba:2x3x4",
        "rgba:99999999999x1",
    ] {
        assert_eq!(image_format(bad), None, "{bad}");
    }
    assert_eq!(raster_bytes(2, 3), Some(24));
    assert_eq!(raster_bytes(0, 3), None);
    assert_eq!(
        raster_bytes(100_000, 100_000),
        None,
        "a side beyond the limit"
    );
    let side = u32::try_from(MAX_PIXELS.isqrt()).expect("fits");
    assert_eq!(raster_bytes(side + 1, side + 1), None, "too many pixels");
    assert_eq!(raster_bytes(u32::MAX, u32::MAX), None);

    assert_eq!(what(image("rgba:100000x100000", 0)), "image_size");
    assert_eq!(what(image("rgba:2x2", 3)), "raster_length");
    assert_eq!(what(image("png", 0)), "image_empty");
    assert_eq!(what(image("tiff", 10)), "image_format");
    assert!(image("rgba:2x2", 16).is_ok());
}

fn span(start: u32, len: u32, class: wit::TokenClass) -> wit::Token {
    wit::Token { start, len, class }
}

#[test]
fn a_line_breaking_a_token_rule_is_shown_uncoloured() {
    let lines = ["{\"a\": 1}", "caf\u{e9} \"x\"", "plain", "fine", "spare"];
    let answered = vec![
        // Overlapping spans.
        vec![
            span(0, 3, wit::TokenClass::Punctuation),
            span(2, 3, wit::TokenClass::String),
        ],
        // A span splitting the two bytes of the e with an acute accent.
        vec![span(0, 4, wit::TokenClass::Keyword)],
        // A span beyond its line.
        vec![span(0, 100, wit::TokenClass::Comment)],
        vec![
            span(0, 2, wit::TokenClass::Key),
            span(2, 2, wit::TokenClass::String),
        ],
    ];
    let shown = tokens(&lines, answered);
    assert_eq!(shown.len(), lines.len());
    assert_eq!(shown[0], None);
    assert_eq!(shown[1], None);
    assert_eq!(shown[2], None);
    let fine = shown[3].as_ref().expect("valid spans");
    assert_eq!(fine.len(), 2);
    assert_eq!(fine[1].class, TokenClass::String);
    assert_eq!(shown[4], None, "no answer for the line");
}

#[test]
fn spans_must_be_sorted_non_empty_and_bounded() {
    let unsorted = vec![vec![
        span(2, 1, wit::TokenClass::Number),
        span(0, 1, wit::TokenClass::Number),
    ]];
    assert!(tokens(&["0123"], unsorted)[0].is_none());
    let empty = vec![vec![span(0, 0, wit::TokenClass::Number)]];
    assert!(tokens(&["0123"], empty)[0].is_none());
    let line = "x".repeat(MAX_TOKENS_PER_LINE + 1);
    let many = vec![
        (0..=MAX_TOKENS_PER_LINE as u32)
            .map(|at| span(at, 1, wit::TokenClass::Literal))
            .collect(),
    ];
    assert!(tokens(&[line.as_str()], many)[0].is_none());
    let extra = vec![vec![], vec![span(0, 1, wit::TokenClass::Literal)]];
    let shown = tokens(&["a"], extra);
    assert_eq!(shown.len(), 1, "lists beyond the lines are dropped");
    assert!(shown[0].as_ref().is_some_and(Vec::is_empty));
}
