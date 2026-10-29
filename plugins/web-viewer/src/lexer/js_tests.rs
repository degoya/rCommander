use crate::test_support::{all, assert_plain, assert_tokens, highlighted, line};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

const JS: &str = concat!(
    "// comment\n",
    "/* multi\n",
    "line */\n",
    "function greet(name) {\n",
    "  const obj = { key: 'v\\'s', other: \"q\\\"t\" };\n",
    "  return obj.key.toUpperCase() + 0x1F + 1.5e3 + 10n;\n",
    "}\n",
    "let n = null, u = undefined, t = true, z = NaN;\n",
    "class Foo extends Bar { static #count = 0; }\n",
    "const re = new RegExp(\"x\") ?? cond ? left : right;\n",
    "label: for (;;) { break label; }\n",
    "@decorator class Baz {}\n",
);

#[test]
fn javascript_has_its_classes() {
    let lines = highlighted("a.js", JS.as_bytes());
    assert_eq!(lines[0], vec![(Comment, "// comment".to_owned())]);
    assert_eq!(lines[1], vec![(Comment, "/* multi".to_owned())]);
    assert_eq!(lines[2], vec![(Comment, "line */".to_owned())]);
    let tokens: Vec<_> = lines.into_iter().flatten().collect();
    assert_tokens(
        &tokens,
        &[
            (Keyword, "function"),
            (Function, "greet"),
            (Keyword, "const"),
            (Key, "key"),
            (String, "'v\\'s'"),
            (Key, "other"),
            (String, "\"q\\\"t\""),
            (Keyword, "return"),
            (Function, "toUpperCase"),
            (Number, "0x1F"),
            (Number, "1.5e3"),
            (Number, "10n"),
            (Literal, "null"),
            (Literal, "undefined"),
            (Literal, "true"),
            (Literal, "NaN"),
            (Keyword, "class"),
            (TypeName, "Foo"),
            (Keyword, "extends"),
            (TypeName, "Bar"),
            (Keyword, "new"),
            (TypeName, "RegExp"),
            (Operator, "??"),
            (Key, "label"),
            (Function, "@decorator"),
            (TypeName, "Baz"),
            (Punctuation, ";"),
        ],
    );
    // A condition's branches are no keys; a property is a plain name.
    assert_plain(&tokens, "left");
    assert_plain(&tokens, "name");
    assert_plain(&tokens, "#count");
}

#[test]
fn a_condition_s_colon_is_an_operator_and_makes_no_key() {
    assert_eq!(
        line("a.js", "x = cond ? left : right;"),
        vec![
            (Operator, "=".to_owned()),
            (Operator, "?".to_owned()),
            (Operator, ":".to_owned()),
            (Punctuation, ";".to_owned()),
        ]
    );
}

#[test]
fn a_division_after_a_value() {
    assert_eq!(
        line("a.js", "a / b / c"),
        vec![(Operator, "/".to_owned()), (Operator, "/".to_owned())]
    );
    assert_tokens(
        &line("a.js", "(a + b)/2/c"),
        &[(Punctuation, ")"), (Operator, "/"), (Number, "2")],
    );
    assert!(
        !line("a.js", "(a + b)/2/c")
            .iter()
            .any(|(class, _)| *class == String)
    );
    assert_tokens(&line("a.js", "x[0] /= 2"), &[(Operator, "/=")]);
}

#[test]
fn a_regular_expression_where_no_value_comes_before() {
    assert_eq!(
        line("a.js", "x = /re/g;"),
        vec![
            (Operator, "=".to_owned()),
            (String, "/re/g".to_owned()),
            (Punctuation, ";".to_owned()),
        ]
    );
    assert_tokens(
        &line("a.js", "return /re/.test(s);"),
        &[(Keyword, "return"), (String, "/re/"), (Function, "test")],
    );
    // A `/` in a class or after a `\` does not end it.
    assert_tokens(
        &line("a.js", "f(/[/]\\//i, 1)"),
        &[(String, "/[/]\\//i"), (Number, "1")],
    );
}

#[test]
fn the_regular_expression_rule_holds_across_lines() {
    let lines = highlighted("a.js", b"x =\n  /re/.source;\ny = a\n  / 2;\n");
    assert_tokens(&lines[1], &[(String, "/re/")]);
    assert_tokens(&lines[3], &[(Operator, "/"), (Number, "2")]);
}

#[test]
fn a_string_with_a_line_continuation_goes_on() {
    let lines = highlighted("a.js", b"s = \"one \\\ntwo\"; x = 1;\n");
    assert_eq!(lines[1][0], (String, "two\"".to_owned()));
    assert_tokens(&lines[1], &[(Number, "1")]);
}

#[test]
fn an_unclosed_string_ends_with_its_line() {
    let lines = highlighted("a.js", b"s = 'open\nx = 1;\n");
    assert_eq!(lines[0][1], (String, "'open".to_owned()));
    assert_tokens(&lines[1], &[(Operator, "="), (Number, "1")]);
}

#[test]
fn a_template_string_goes_over_lines_with_code_in_it() {
    let lines = highlighted("a.js", b"t = `x ${\n  a + 1\n} y\nz`;\n");
    assert_tokens(
        &lines[0],
        &[(String, "`"), (String, "x "), (Punctuation, "${")],
    );
    assert_tokens(&lines[1], &[(Operator, "+"), (Number, "1")]);
    assert_tokens(&lines[2], &[(Punctuation, "}"), (String, " y")]);
    assert_eq!(
        lines[3],
        vec![
            (String, "z".to_owned()),
            (String, "`".to_owned()),
            (Punctuation, ";".to_owned()),
        ]
    );
}

#[test]
fn a_shebang_is_a_comment() {
    let lines = highlighted("a.mjs", b"#!/usr/bin/env node\nimport x from 'y';\n");
    assert_eq!(lines[0], vec![(Comment, "#!/usr/bin/env node".to_owned())]);
    assert_tokens(
        &lines[1],
        &[(Keyword, "import"), (Keyword, "from"), (String, "'y'")],
    );
}

#[test]
fn a_keyword_after_a_dot_is_a_property() {
    let tokens = all("a.js", "a.default.get(x); obj.new = 1;");
    assert_tokens(&tokens, &[(Function, "get")]);
    assert_plain(&tokens, "default");
    assert_plain(&tokens, "new");
}
