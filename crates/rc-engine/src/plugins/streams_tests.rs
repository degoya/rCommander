//! The line starts of a plugin's text, counted while it is written.

use super::LineFeed;
use crate::viewer::TextEncoding;

#[test]
fn text_written_in_order_is_counted_on_the_way() {
    let mut feed = LineFeed::default();
    feed.wrote(0, b"{\n  \"a\": 1,");
    feed.wrote(11, b"\n  \"b\": 2\n}");
    let index = feed
        .finish(22, TextEncoding::Utf8)
        .expect("counted in order");
    assert!(index.is_complete());
    assert_eq!(index.line_count(), 4);
}

#[test]
fn a_write_out_of_order_or_a_wider_encoding_is_counted_afterwards() {
    let mut feed = LineFeed::default();
    feed.wrote(4, b"late");
    feed.wrote(0, b"head");
    assert!(feed.finish(8, TextEncoding::Utf8).is_none());

    let mut wide = LineFeed::default();
    wide.wrote(0, b"a\0\n\0");
    assert!(wide.finish(4, TextEncoding::Utf16Le).is_none());

    let mut short = LineFeed::default();
    short.wrote(0, b"abc");
    assert!(
        short.finish(5, TextEncoding::Utf8).is_none(),
        "the host counted more bytes than the feed saw"
    );
}
