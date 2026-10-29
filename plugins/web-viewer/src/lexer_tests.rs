//! The languages inside each other: what opens and what closes a frame.

use crate::test_support::{assert_plain, assert_tokens, highlighted, line};
use crate::types::TokenClass::{
    Comment, Function, Invalid, Key, Keyword, Literal, Number, Operator, Punctuation, String,
    TypeName,
};

fn owned(
    tokens: &[(crate::types::TokenClass, &str)],
) -> Vec<(crate::types::TokenClass, std::string::String)> {
    tokens
        .iter()
        .map(|(class, text)| (*class, (*text).to_owned()))
        .collect()
}

#[test]
fn a_script_in_html_is_javascript_until_its_end_tag() {
    let text = concat!(
        "<script>\n",
        "const re = /a<b/g; // </b>\n",
        "if (a < b) x = \"</p>\";\n",
        "</script>\n",
        "<p>after</p>\n",
    );
    let lines = highlighted("a.html", text.as_bytes());
    assert_tokens(
        &lines[1],
        &[(Keyword, "const"), (String, "/a<b/g"), (Comment, "// </b>")],
    );
    assert_tokens(
        &lines[2],
        &[(Keyword, "if"), (Operator, "<"), (String, "\"</p>\"")],
    );
    assert_eq!(
        lines[3],
        owned(&[
            (Punctuation, "</"),
            (TypeName, "script"),
            (Punctuation, ">")
        ])
    );
    assert_tokens(&lines[4], &[(TypeName, "p")]);
}

#[test]
fn an_end_tag_in_a_string_ends_the_script_as_in_a_browser() {
    let tokens = line("a.html", "<script>var s = \"</script>\";</script>");
    assert_eq!(
        tokens,
        owned(&[
            (Punctuation, "<"),
            (TypeName, "script"),
            (Punctuation, ">"),
            (Keyword, "var"),
            (Operator, "="),
            (String, "\""),
            (Punctuation, "</"),
            (TypeName, "script"),
            (Punctuation, ">"),
            (Punctuation, "</"),
            (TypeName, "script"),
            (Punctuation, ">"),
        ])
    );
}

#[test]
fn an_end_tag_in_a_comment_or_a_template_ends_the_script_too() {
    let lines = highlighted(
        "a.html",
        b"<script>/* x\n</SCRIPT><b>\n<script>`a\n</script >\n",
    );
    assert_tokens(
        &lines[1],
        &[(Punctuation, "</"), (TypeName, "SCRIPT"), (TypeName, "b")],
    );
    assert_tokens(&lines[3], &[(Punctuation, "</"), (TypeName, "script")]);
}

#[test]
fn a_style_in_html_is_css_until_its_end_tag() {
    let tokens = line("a.html", "<style>a { color: red; } /* </b> */</style><i>");
    assert_tokens(
        &tokens,
        &[
            (TypeName, "style"),
            (TypeName, "a"),
            (Key, "color"),
            (Comment, "/* </b> */"),
            (TypeName, "i"),
        ],
    );
    assert_plain(&tokens, "red");
}

#[test]
fn style_and_event_attributes_hold_css_and_javascript() {
    let tokens = line(
        "a.html",
        "<p style=\"color: red; margin: 0 auto\" onclick=\"go(1, 'x')\" title=\"t\">",
    );
    assert_tokens(
        &tokens,
        &[
            (Key, "style"),
            (String, "\""),
            (Key, "color"),
            (Key, "margin"),
            (Number, "0"),
            (Literal, "auto"),
            (Key, "onclick"),
            (Function, "go"),
            (String, "'x'"),
            (Key, "title"),
            (String, "\"t\""),
            (Punctuation, ">"),
        ],
    );
    // The attribute's quote closes the JavaScript in it wherever it stands.
    let quoted = line("a.html", "<a onclick=\"f(\" class=c>");
    assert_tokens(&quoted, &[(Function, "f"), (Key, "class"), (String, "c")]);
}

#[test]
fn a_vue_file_has_typescript_and_scss_by_lang() {
    let text = concat!(
        "<template><div :class=\"x\">{{ a }}</div></template>\n",
        "<script setup lang=\"ts\">\n",
        "const n: number = 1;\n",
        "</script>\n",
        "<style lang=\"scss\" scoped>\n",
        "$c: red;\n",
        ".a { &:hover { color: $c; } }\n",
        "</style>\n",
    );
    let lines = highlighted("App.vue", text.as_bytes());
    assert_tokens(&lines[0], &[(TypeName, "template"), (Key, ":class")]);
    assert_tokens(&lines[2], &[(Key, "n"), (TypeName, "number")]);
    assert_tokens(&lines[5], &[(Key, "$c")]);
    assert_tokens(
        &lines[6],
        &[(TypeName, "&"), (TypeName, ":hover"), (Key, "$c")],
    );
}

#[test]
fn php_with_html_between_over_lines() {
    let text = concat!(
        "<?php if ($a): ?>\n",
        "<b class=\"<?= $c ?>\">x</b>\n",
        "<?php endif; // done ?>\n",
        "<script>var x = <?= json_encode($a) ?>; var s = \"<?php echo $s ?>\";</script>\n",
        "<p>tail</p>\n",
    );
    let lines = highlighted("a.php", text.as_bytes());
    assert_tokens(
        &lines[0],
        &[
            (Punctuation, "<?php"),
            (Keyword, "if"),
            (Key, "$a"),
            (Punctuation, "?>"),
        ],
    );
    assert_tokens(
        &lines[1],
        &[
            (Key, "class"),
            (Punctuation, "<?="),
            (Key, "$c"),
            (Punctuation, "?>"),
            (TypeName, "b"),
        ],
    );
    assert_tokens(
        &lines[2],
        &[
            (Keyword, "endif"),
            (Comment, "// done "),
            (Punctuation, "?>"),
        ],
    );
    assert_tokens(
        &lines[3],
        &[
            (Keyword, "var"),
            (Punctuation, "<?="),
            (Function, "json_encode"),
            (Keyword, "echo"),
            (Key, "$s"),
            (TypeName, "script"),
        ],
    );
    assert_tokens(&lines[4], &[(TypeName, "p")]);
}

#[test]
fn a_template_string_inside_a_template_string() {
    let tokens = line("a.js", "const s = `a ${`b ${c} d`} e`;");
    assert_eq!(
        tokens,
        owned(&[
            (Keyword, "const"),
            (Operator, "="),
            (String, "`"),
            (String, "a "),
            (Punctuation, "${"),
            (String, "`"),
            (String, "b "),
            (Punctuation, "${"),
            (Punctuation, "}"),
            (String, " d"),
            (String, "`"),
            (Punctuation, "}"),
            (String, " e"),
            (String, "`"),
            (Punctuation, ";"),
        ])
    );
}

#[test]
fn the_stack_stops_at_64_and_the_rest_of_the_line_is_invalid() {
    // A JavaScript file is one frame; each "`" and each "${" opens one more. The 32nd "${" would
    // be the 65th frame.
    let text = format!("{}\nx\n", "`${".repeat(40));
    let lines = highlighted("a.js", text.as_bytes());
    let rest = format!("${{{}", "`${".repeat(8));
    assert_eq!(lines[0].last(), Some(&(Invalid, rest)));
    assert_eq!(
        lines[0]
            .iter()
            .filter(|(class, _)| *class == Invalid)
            .count(),
        1
    );
    // The next line starts inside the 64th frame, a template string.
    assert_eq!(lines[1], owned(&[(String, "x")]));
}
