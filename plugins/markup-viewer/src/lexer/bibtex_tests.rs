use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

/// A BibTeX file with what sets it apart: a comment, an entry with its key, braced and quoted
/// values (braces nested, one over two lines), a number, an abbreviation, `@string`, `@comment`,
/// text between entries, and an entry in parentheses joining values with `#`.
const SAMPLE: &str = r#"% references
@article{knuth1984,
  author  = {Donald E. Knuth},
  title   = "Literate {P}rogramming",
  year    = 1984,
  month   = jan,
  abstract = {A long
    abstract {nested}.},
}
@string{acm = "ACM"}
@comment{ignored}
Free text between entries.
@book(key2, publisher = acm # " Press")"#;

#[test]
fn a_bibtex_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Bibtex, SAMPLE);
    assert_eq!(lines[0], vec![("% references", Comment)]);
    assert_eq!(
        lines[1],
        vec![
            ("@article", Keyword),
            ("{", Punctuation),
            ("knuth1984", TypeName),
            (",", Punctuation)
        ]
    );
    assert_eq!(
        lines[2],
        vec![
            ("author", Key),
            ("=", Operator),
            ("{Donald E. Knuth}", String),
            (",", Punctuation)
        ]
    );
    assert_eq!(
        class_of(&lines[3..4], "\"Literate {P}rogramming\""),
        Some(String)
    );
    assert_eq!(class_of(&lines[4..5], "1984"), Some(Number));
    assert_eq!(class_of(&lines[5..6], "jan"), Some(Literal));
    assert_eq!(texts_of(&lines[6..7], String), vec!["{A long"]);
    assert_eq!(
        lines[7],
        vec![("    abstract {nested}.}", String), (",", Punctuation)]
    );
    assert_eq!(lines[8], vec![("}", Punctuation)]);
    assert_eq!(
        lines[9],
        vec![
            ("@string", Keyword),
            ("{", Punctuation),
            ("acm", Key),
            ("=", Operator),
            ("\"ACM\"", String),
            ("}", Punctuation)
        ]
    );
    assert_eq!(
        lines[10],
        vec![("@comment", Keyword), ("{ignored}", Comment)]
    );
    assert_eq!(lines[11], vec![("Free text between entries.", Comment)]);
    assert_eq!(
        lines[12],
        vec![
            ("@book", Keyword),
            ("(", Punctuation),
            ("key2", TypeName),
            (",", Punctuation),
            ("publisher", Key),
            ("=", Operator),
            ("acm", Literal),
            ("#", Operator),
            ("\" Press\"", String),
            (")", Punctuation)
        ]
    );
}

#[test]
fn a_key_may_stand_on_the_next_line_and_text_before_an_entry_is_a_comment() {
    let lines = lexed(Lang::Bibtex, "see @misc{\n  later,\n  note = {x}}");
    assert_eq!(
        lines[0],
        vec![("see ", Comment), ("@misc", Keyword), ("{", Punctuation)]
    );
    assert_eq!(lines[1], vec![("later", TypeName), (",", Punctuation)]);
    assert_eq!(lines[2].last(), Some(&("}", Punctuation)));
}
