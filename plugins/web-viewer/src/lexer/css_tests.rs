use crate::test_support::{all, assert_plain, assert_tokens, highlighted};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

const CSS: &str = concat!(
    "/* header\n",
    "   comment */\n",
    "@media screen and (max-width: 600px) {\n",
    "  .nav > a:hover, #main::before { color: #fff !important; width: calc(100% - 2em); }\n",
    "}\n",
    "a[href^=\"http\"] { background: url(img/a.png) no-repeat; font-family: \"Open Sans\", x; }\n",
);

#[test]
fn css_has_its_classes() {
    let lines = highlighted("a.css", CSS.as_bytes());
    assert_eq!(lines[0], vec![(Comment, "/* header".to_owned())]);
    assert_eq!(lines[1], vec![(Comment, "   comment */".to_owned())]);
    assert_tokens(
        &lines[2],
        &[
            (Keyword, "@media"),
            (Literal, "screen"),
            (Literal, "and"),
            (Key, "max-width"),
            (Number, "600px"),
            (Punctuation, "{"),
        ],
    );
    assert_tokens(
        &lines[3],
        &[
            (TypeName, ".nav"),
            (Operator, ">"),
            (TypeName, "a"),
            (TypeName, ":hover"),
            (TypeName, "#main"),
            (TypeName, "::before"),
            (Key, "color"),
            (Number, "#fff"),
            (Literal, "!important"),
            (Key, "width"),
            (Function, "calc"),
            (Number, "100%"),
            (Operator, "-"),
            (Number, "2em"),
        ],
    );
    assert_tokens(
        &lines[5],
        &[
            (TypeName, "a"),
            (Key, "href"),
            (Operator, "^="),
            (String, "\"http\""),
            (Key, "background"),
            (Function, "url"),
            (String, "img/a.png"),
            (Literal, "no-repeat"),
            (Key, "font-family"),
            (String, "\"Open Sans\""),
        ],
    );
}

#[test]
fn minified_css_tells_selectors_from_declarations() {
    let tokens = all("a.min.css", ".a{color:red;margin:0}.b>i{top:-1px}");
    assert_tokens(
        &tokens,
        &[
            (TypeName, ".a"),
            (Key, "color"),
            (Key, "margin"),
            (Number, "0"),
            (TypeName, ".b"),
            (TypeName, "i"),
            (Key, "top"),
            (Number, "-1px"),
        ],
    );
    assert_plain(&tokens, "red");
}

#[test]
fn a_declaration_value_goes_over_lines() {
    let lines = highlighted(
        "a.css",
        b"a {\n  grid-template-areas:\n    \"head\"\n    \"main\";\n  top: 0;\n}\n",
    );
    assert_eq!(lines[2], vec![(String, "\"head\"".to_owned())]);
    assert_tokens(&lines[4], &[(Key, "top"), (Number, "0")]);
}

#[test]
fn scss_nests_and_has_variables_and_line_comments() {
    let text = concat!(
        "$main: #333;\n",
        "// line comment\n",
        ".card {\n",
        "  &:hover { color: $main; }\n",
        "  @include shadow(2px);\n",
        "  .title { margin: 0 }\n",
        "  #{$sel} { top: 0; }\n",
        "}\n",
    );
    let lines = highlighted("a.scss", text.as_bytes());
    assert_tokens(&lines[0], &[(Key, "$main"), (Number, "#333")]);
    assert_eq!(lines[1], vec![(Comment, "// line comment".to_owned())]);
    assert_tokens(&lines[2], &[(TypeName, ".card")]);
    assert_tokens(
        &lines[3],
        &[
            (TypeName, "&"),
            (TypeName, ":hover"),
            (Key, "color"),
            (Key, "$main"),
        ],
    );
    assert_tokens(
        &lines[4],
        &[(Keyword, "@include"), (Function, "shadow"), (Number, "2px")],
    );
    assert_tokens(&lines[5], &[(TypeName, ".title"), (Key, "margin")]);
    assert_tokens(
        &lines[6],
        &[(Punctuation, "#{"), (Key, "$sel"), (Key, "top")],
    );
}

#[test]
fn less_has_at_variables_and_mixins() {
    let text = "@color: red;\n.mixin() { color: @color; }\n@media print { .a { .mixin(); } }\n";
    let lines = highlighted("a.less", text.as_bytes());
    assert_tokens(&lines[0], &[(Key, "@color")]);
    assert_tokens(
        &lines[1],
        &[(TypeName, ".mixin"), (Key, "color"), (Key, "@color")],
    );
    assert_tokens(
        &lines[2],
        &[
            (Keyword, "@media"),
            (Literal, "print"),
            (TypeName, ".a"),
            (Function, ".mixin"),
        ],
    );
}

#[test]
fn a_string_keeps_its_escapes() {
    let tokens = all("a.css", "a::after { content: \"a\\\"b\"; }");
    assert_tokens(&tokens, &[(String, "\"a\\\"b\""), (Key, "content")]);
}

#[test]
fn plain_css_has_no_line_comments() {
    // `//` is no comment in CSS: the declaration is read as usual.
    let tokens = all("a.css", "a { top: 0; // x\n}");
    assert!(
        !tokens.iter().any(|(class, _)| *class == Comment),
        "{tokens:?}"
    );
}
