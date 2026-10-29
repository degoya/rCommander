use super::*;
use crate::lexer::Lexer;
use crate::reader::SliceBytes;

fn layout(text: &str) -> String {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut state = LayoutState::default();
    let mut out = String::new();
    while let Some(lexeme) = lexer.next().expect("a slice cannot fail") {
        for piece in state.feed(&lexeme).iter() {
            match piece {
                Piece::Break(depth) => {
                    out.push('\n');
                    out.push_str(&" ".repeat(indentation(depth).len()));
                }
                Piece::Space => out.push(' '),
                Piece::Lexeme => {
                    out.push_str(&text[lexeme.start as usize..lexeme.end as usize]);
                }
            }
        }
    }
    out
}

#[test]
fn members_and_elements_get_a_line_each() {
    assert_eq!(
        layout(r#"{"a":[1,2,{}],"b":{"c":[ ]},"d":null}"#),
        "{\n  \"a\": [\n    1,\n    2,\n    {}\n  ],\n  \"b\": {\n    \"c\": []\n  },\n  \"d\": null\n}"
    );
    assert_eq!(layout("[]"), "[]");
    assert_eq!(layout("  42 "), "42");
}

#[test]
fn invalid_json_is_laid_out_as_well_as_it_can_be() {
    // Two values in a row: a line each, not run together.
    assert_eq!(layout("[1 2]"), "[\n  1\n  2\n]");
    // A close too many keeps depth at zero.
    assert_eq!(layout("[]]"), "[]\n]");
    assert_eq!(layout("[,]"), "[\n  ,\n  \n]");
}

#[test]
fn indentation_stops_at_its_deepest_level() {
    let deep = "[".repeat(100);
    let text = layout(&deep);
    let widest = text.lines().map(str::len).max().unwrap_or_default();
    assert_eq!(widest, 2 * MAX_INDENT_LEVELS as usize + 1);
}
