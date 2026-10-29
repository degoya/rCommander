use super::retype;
use crate::lexer::state::{Attr, CssDialect, Dialect, Elem};
use crate::test_support::{all, assert_plain, assert_tokens, highlighted, line};
use crate::types::TokenClass::{
    Comment, Key, Keyword, Literal, Operator, Punctuation, String, TypeName,
};

#[test]
fn tags_attributes_values_and_entities() {
    let tokens = line(
        "a.html",
        "<div class=\"a &amp; b\" id=x>Hi &lt; there &#x26; &#38;</div>",
    );
    assert_tokens(
        &tokens,
        &[
            (Punctuation, "<"),
            (TypeName, "div"),
            (Key, "class"),
            (Operator, "="),
            (String, "\"a "),
            (Literal, "&amp;"),
            (String, " b\""),
            (Key, "id"),
            (String, "x"),
            (Punctuation, ">"),
            (Literal, "&lt;"),
            (Literal, "&#x26;"),
            (Literal, "&#38;"),
            (Punctuation, "</"),
        ],
    );
    // Text stays uncoloured.
    assert_plain(&tokens, "Hi ");
}

#[test]
fn a_self_closing_tag_and_a_declaration() {
    let tokens = all("a.html", "<!DOCTYPE html>\n<br/><img src='a.png' />\n");
    assert_tokens(
        &tokens,
        &[
            (Keyword, "<!DOCTYPE html>"),
            (TypeName, "br"),
            (Punctuation, "/>"),
            (TypeName, "img"),
            (Key, "src"),
            (String, "'a.png'"),
        ],
    );
}

#[test]
fn a_comment_goes_over_lines() {
    let lines = highlighted("a.html", b"<!-- one\ntwo -->after<b>\n");
    assert_eq!(lines[0], vec![(Comment, "<!-- one".to_owned())]);
    assert_eq!(
        lines[1],
        vec![
            (Comment, "two -->".to_owned()),
            (Punctuation, "<".to_owned()),
            (TypeName, "b".to_owned()),
            (Punctuation, ">".to_owned()),
        ]
    );
}

#[test]
fn a_tag_and_a_value_go_over_lines() {
    let lines = highlighted("a.html", b"<a\n  title=\"one\ntwo\"\n  href=x>link</a>\n");
    assert_eq!(
        lines[1],
        vec![
            (Key, "title".to_owned()),
            (Operator, "=".to_owned()),
            (String, "\"one".to_owned())
        ]
    );
    assert_eq!(lines[2], vec![(String, "two\"".to_owned())]);
    assert_tokens(
        &lines[3],
        &[(Key, "href"), (String, "x"), (Punctuation, ">")],
    );
}

#[test]
fn a_lone_ampersand_and_angle_are_text() {
    let tokens = line("a.html", "a & b < c");
    assert!(tokens.is_empty(), "{tokens:?}");
}

#[test]
fn lang_and_type_choose_the_dialect() {
    let script = Elem::Script(Dialect::Js);
    assert_eq!(retype(script, Attr::Lang, b"ts"), Elem::Script(Dialect::Ts));
    assert_eq!(
        retype(script, Attr::Lang, b"tsx"),
        Elem::Script(Dialect::Tsx)
    );
    assert_eq!(
        retype(script, Attr::Type, b"text/typescript"),
        Elem::Script(Dialect::Ts)
    );
    assert_eq!(
        retype(script, Attr::Type, b"module"),
        Elem::Script(Dialect::Js)
    );
    assert_eq!(
        retype(script, Attr::Type, b"application/ld+json"),
        Elem::Script(Dialect::Js)
    );
    assert_eq!(
        retype(script, Attr::Type, b"text/x-template"),
        Elem::RawScript
    );
    let style = Elem::Style(CssDialect::Css);
    assert_eq!(
        retype(style, Attr::Lang, b"scss"),
        Elem::Style(CssDialect::Scss)
    );
    assert_eq!(
        retype(style, Attr::Lang, b"less"),
        Elem::Style(CssDialect::Less)
    );
    assert_eq!(retype(Elem::Other, Attr::Lang, b"ts"), Elem::Other);
}

#[test]
fn a_template_script_is_not_coloured() {
    let lines = highlighted(
        "a.html",
        b"<script type=\"text/x-template\">\n<div>{{ a }}</div> var x = 1;\n</script>\n",
    );
    assert!(lines[1].is_empty(), "{:?}", lines[1]);
    assert_tokens(&lines[2], &[(Punctuation, "</"), (TypeName, "script")]);
}
