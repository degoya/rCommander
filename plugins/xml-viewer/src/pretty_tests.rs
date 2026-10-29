use super::*;
use crate::lexer::Lexer;
use crate::reader::SliceBytes;

/// `text` laid out.
fn pretty(text: &str) -> String {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut layout = LayoutState::default();
    let mut out = String::new();
    while let Some(lexeme) = lexer.next().expect("a slice reads") {
        for piece in layout.feed(&lexeme).iter() {
            match piece {
                Piece::Break(depth) => {
                    out.push('\n');
                    out.push_str(std::str::from_utf8(indentation(depth)).expect("spaces"));
                }
                Piece::RawBreak => out.push('\n'),
                Piece::Space => out.push(' '),
                Piece::Lexeme => out.push_str(&text[lexeme.start as usize..lexeme.end as usize]),
            }
        }
    }
    out
}

#[test]
fn an_element_a_line_two_spaces_a_level() {
    assert_eq!(
        pretty("<a><b>x</b><c/></a>"),
        "<a>\n  <b>x</b>\n  <c/>\n</a>"
    );
}

#[test]
fn the_prolog_and_a_tag_s_attributes_get_lines_and_single_spaces() {
    assert_eq!(
        pretty("<?xml version=\"1.0\"?><!--c--><root   a = \"1\"\n   b='2'><e></e></root>"),
        "<?xml version=\"1.0\"?>\n<!--c-->\n<root a=\"1\" b='2'>\n  <e></e>\n</root>"
    );
}

#[test]
fn text_stays_inline_and_mixed_content_gets_lines() {
    assert_eq!(
        pretty("<p>\n  Hello\n  <b>world</b>  again &amp; more\n</p>"),
        "<p>Hello\n  <b>world</b>\n  again &amp; more\n</p>"
    );
    assert_eq!(pretty("<t>one\n   two</t>"), "<t>one two</t>");
    assert_eq!(pretty("<s><![CDATA[x]]></s>"), "<s><![CDATA[x]]></s>");
}

#[test]
fn constructs_over_lines_keep_their_lines() {
    assert_eq!(
        pretty("<a><!-- one\n     two --><v x=\"1\n  2\"/></a>"),
        "<a>\n  <!-- one\n     two -->\n  <v x=\"1\n  2\"/>\n</a>"
    );
}

#[test]
fn depth_never_goes_below_zero() {
    assert_eq!(pretty("</x></y><z/>"), "</x>\n</y>\n<z/>");
}

#[test]
fn indentation_is_capped() {
    assert_eq!(indentation(0), b"");
    assert_eq!(indentation(2), b"    ");
    assert_eq!(indentation(1000).len(), 2 * MAX_INDENT_LEVELS as usize);
}
