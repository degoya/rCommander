//! One sample per language with what sets it apart, lexed line by line from the file's start.

use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    self, Comment, Function, Invalid, Keyword, Literal, Number, Operator, Punctuation, TypeName,
};

const STRING: TokenClass = TokenClass::String;

#[test]
fn c_directives_splices_comments_and_digit_separators() {
    let lines = lexed(
        Lang::C,
        concat!(
            "#include <stdio.h>\n",
            "#define MAX(a, b) \\\n",
            "    ((a) > (b) ? (a) : (b))\n",
            "#error don't do this\n",
            "/* block */ int main(void) { // done \\\n",
            "   still comment\n",
            "    char c = 'x'; long n = 1'000;\n",
            "    return NULL; // #define STR(x) #x\n",
            "}\n",
            "#define STR(x) #x",
        ),
    );
    assert_eq!(lines[0], vec![("#include", Keyword), ("<stdio.h>", STRING)]);
    assert_eq!(lines[1][0], ("#define", Keyword));
    assert_eq!(class_of(&lines[1..2], "MAX"), Some(Function));
    // The directive goes on: code, no directive of its own.
    assert!(texts_of(&lines[2..3], Keyword).is_empty());
    assert_eq!(class_of(&lines[2..3], "?"), Some(Operator));
    // `#error`'s text is no code: its quote opens nothing.
    assert_eq!(lines[3], vec![("#error", Keyword)]);
    assert_eq!(lines[4][0], ("/* block */", Comment));
    assert_eq!(class_of(&lines[4..5], "int"), Some(TypeName));
    assert_eq!(class_of(&lines[4..5], "main"), Some(Function));
    assert_eq!(lines[4].last(), Some(&("// done \\", Comment)));
    assert_eq!(lines[5], vec![("   still comment", Comment)]);
    assert_eq!(class_of(&lines[6..7], "'x'"), Some(STRING));
    assert_eq!(class_of(&lines[6..7], "1'000"), Some(Number));
    assert_eq!(class_of(&lines[7..8], "NULL"), Some(Literal));
    assert_eq!(
        lines[9],
        vec![
            ("#define", Keyword),
            ("STR", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("#", Operator)
        ]
    );
}

#[test]
fn cpp_templates_raw_strings_and_prefixes() {
    let lines = lexed(
        Lang::Cpp,
        concat!(
            "#pragma once\n",
            "namespace app {\n",
            "template <typename T> class Box final {\n",
            "    auto s = R\"json({\"a\": \")\"})json\";\n",
            "    auto w = L\"wide\"; char8_t c = u8'x';\n",
            "    int n = 0x7FFF'FFFF;\n",
            "};\n",
            "}  // namespace app",
        ),
    );
    assert_eq!(lines[0], vec![("#pragma", Keyword)]);
    assert_eq!(class_of(&lines[1..2], "namespace"), Some(Keyword));
    assert_eq!(
        texts_of(&lines[2..3], Keyword),
        vec!["template", "typename", "class", "final"]
    );
    assert_eq!(class_of(&lines[2..3], "Box"), Some(TypeName));
    assert_eq!(
        texts_of(&lines[3..4], STRING),
        vec!["R\"json({\"a\": \")\"})json\""]
    );
    assert_eq!(texts_of(&lines[4..5], STRING), vec!["L\"wide\"", "u8'x'"]);
    assert_eq!(class_of(&lines[4..5], "char8_t"), Some(TypeName));
    assert_eq!(class_of(&lines[5..6], "0x7FFF'FFFF"), Some(Number));
    assert_eq!(lines[7].last(), Some(&("// namespace app", Comment)));
}

#[test]
fn csharp_regions_records_verbatim_and_raw_strings() {
    let lines = lexed(
        Lang::CSharp,
        concat!(
            "#region Setup\n",
            "using System;\n",
            "[Obsolete] public record Point(int X, int Y);\n",
            "var path = @\"C:\\temp\\\";\n",
            "var msg = $\"{Count:D3} items\";\n",
            "var json = \"\"\"\n",
            "    {\"name\": \"x\"}\n",
            "    \"\"\";\n",
            "var @class = 1;\n",
            "#endregion",
        ),
    );
    assert_eq!(lines[0], vec![("#region", Keyword)]);
    assert_eq!(class_of(&lines[1..2], "using"), Some(Keyword));
    assert_eq!(class_of(&lines[2..3], "record"), Some(Keyword));
    assert_eq!(class_of(&lines[2..3], "Point"), Some(TypeName));
    assert_eq!(class_of(&lines[2..3], "int"), Some(TypeName));
    // A backslash in a verbatim string is text: the string closes at its quote.
    assert_eq!(texts_of(&lines[3..4], STRING), vec!["@\"C:\\temp\\\""]);
    assert_eq!(
        texts_of(&lines[4..5], STRING),
        vec!["$\"", "D3", " items\""]
    );
    assert_eq!(lines[5].last(), Some(&("\"\"\"", STRING)));
    assert_eq!(lines[6], vec![("    {\"name\": \"x\"}", STRING)]);
    assert_eq!(lines[7][0], ("    \"\"\"", STRING));
    assert_eq!(texts_of(&lines[8..9], Keyword), vec!["var"]);
    assert_eq!(lines[9], vec![("#endregion", Keyword)]);
}

#[test]
fn java_annotations_declarations_and_text_blocks() {
    let lines = lexed(
        Lang::Java,
        concat!(
            "package app;\n",
            "@Override @SuppressWarnings(\"unchecked\")\n",
            "public final class Main extends Base implements Runnable {\n",
            "    String text = \"\"\"\n",
            "        Hello, \"world\"\n",
            "        \"\"\";\n",
            "    char quote = '\"'; int[] xs = {1_000, 0b1010};\n",
            "    @interface Marker {}\n",
            "}",
        ),
    );
    assert_eq!(
        lines[1],
        vec![
            ("@Override", Function),
            ("@SuppressWarnings", Function),
            ("(", Punctuation),
            ("\"unchecked\"", STRING),
            (")", Punctuation)
        ]
    );
    assert_eq!(
        texts_of(&lines[2..3], Keyword),
        vec!["public", "final", "class", "extends", "implements"]
    );
    assert_eq!(class_of(&lines[2..3], "Main"), Some(TypeName));
    assert_eq!(lines[4], vec![("        Hello, \"world\"", STRING)]);
    assert_eq!(lines[5][0], ("        \"\"\"", STRING));
    assert_eq!(texts_of(&lines[6..7], STRING), vec!["'\"'"]);
    assert_eq!(texts_of(&lines[6..7], Number), vec!["1_000", "0b1010"]);
    assert_eq!(lines[7][0], ("@interface", Keyword));
    assert_eq!(class_of(&lines[7..8], "Marker"), Some(TypeName));
}

#[test]
fn kotlin_scripts_nested_comments_and_templates() {
    let lines = lexed(
        Lang::Kotlin,
        concat!(
            "#!/usr/bin/env kotlin\n",
            "@file:JvmName(\"Main\")\n",
            "/* outer /* inner */ still outer */\n",
            "data class User(val name: String, val age: Int)\n",
            "fun greet(u: User) = \"Hi ${u.name}, you are $age\"\n",
            "val raw = \"\"\"\n",
            "    path: C:\\no\\escape $home\n",
            "\"\"\"",
        ),
    );
    assert_eq!(lines[0], vec![("#!/usr/bin/env kotlin", Comment)]);
    assert_eq!(lines[1][0], ("@file", Function));
    assert_eq!(class_of(&lines[1..2], "JvmName"), Some(Function));
    assert_eq!(
        lines[2],
        vec![("/* outer /* inner */ still outer */", Comment)]
    );
    assert_eq!(
        texts_of(&lines[3..4], Keyword),
        vec!["data", "class", "val", "val"]
    );
    assert_eq!(
        texts_of(&lines[3..4], TypeName),
        vec!["User", "String", "Int"]
    );
    assert_eq!(class_of(&lines[4..5], "greet"), Some(Function));
    assert_eq!(
        texts_of(&lines[4..5], STRING),
        vec!["\"Hi ", ", you are ", "\""]
    );
    assert_eq!(
        lines[6],
        vec![("    path: C:\\no\\escape ", STRING), ("$", Punctuation)]
    );
    assert_eq!(lines[7], vec![("\"\"\"", STRING)]);
}

#[test]
fn go_declarations_raw_strings_and_runes() {
    let lines = lexed(
        Lang::Go,
        concat!(
            "package main\n",
            "\n",
            "import \"fmt\"\n",
            "\n",
            "type Point struct{ X, Y int }\n",
            "\n",
            "func main() {\n",
            "\ts := `C:\\raw\n",
            "still raw`\n",
            "\tr := '\\n'\n",
            "\tfmt.Println(s, r, nil, len(s))\n",
            "}",
        ),
    );
    assert_eq!(lines[0], vec![("package", Keyword)]);
    assert_eq!(lines[2], vec![("import", Keyword), ("\"fmt\"", STRING)]);
    assert_eq!(
        lines[4][..3],
        [("type", Keyword), ("Point", TypeName), ("struct", Keyword)]
    );
    assert_eq!(class_of(&lines[4..5], "int"), Some(TypeName));
    assert_eq!(class_of(&lines[6..7], "main"), Some(Function));
    assert_eq!(lines[7].last(), Some(&("`C:\\raw", STRING)));
    assert_eq!(lines[8], vec![("still raw`", STRING)]);
    assert_eq!(texts_of(&lines[9..10], STRING), vec!["'\\n'"]);
    assert_eq!(texts_of(&lines[10..11], Function), vec!["Println", "len"]);
    assert_eq!(class_of(&lines[10..11], "nil"), Some(Literal));
}

#[test]
fn rust_attributes_lifetimes_macros_raw_strings_and_nested_comments() {
    let lines = lexed(
        Lang::Rust,
        concat!(
            "#![allow(dead_code)]\n",
            "/// Docs.\n",
            "#[derive(Debug, Clone)]\n",
            "struct Holder<'a> { name: &'a str, tag: char }\n",
            "/* outer /* nested */ still */\n",
            "impl<'a> Holder<'a> {\n",
            "    fn new(name: &'a str) -> Self {\n",
            "        let c = 'x'; let q = '\\''; let r = r#\"raw \"q\"\"#;\n",
            "        println!(\"{name} {}\", c != q);\n",
            "        'outer: loop { break 'outer; }\n",
            "    }\n",
            "}",
        ),
    );
    assert_eq!(lines[0][..2], [("#![", Punctuation), ("allow", Function)]);
    assert_eq!(lines[1], vec![("/// Docs.", Comment)]);
    assert_eq!(lines[2][..2], [("#[", Punctuation), ("derive", Function)]);
    assert_eq!(
        texts_of(&lines[3..4], TypeName),
        vec!["Holder", "'a", "'a", "str", "char"]
    );
    assert_eq!(lines[4], vec![("/* outer /* nested */ still */", Comment)]);
    assert_eq!(class_of(&lines[6..7], "new"), Some(Function));
    assert_eq!(class_of(&lines[6..7], "Self"), Some(Keyword));
    assert_eq!(
        texts_of(&lines[7..8], STRING),
        vec!["'x'", "'\\''", "r#\"raw \"q\"\"#"]
    );
    assert_eq!(lines[8][0], ("println!", Function));
    assert_eq!(class_of(&lines[8..9], "!="), Some(Operator));
    assert_eq!(texts_of(&lines[9..10], TypeName), vec!["'outer", "'outer"]);
    assert!(texts_of(&lines, Invalid).is_empty());
}

/// The acceptance's case: a lifetime opens no character literal that would colour the rest of
/// the line as a string.
#[test]
fn a_rust_lifetime_is_no_character_to_the_line_end() {
    let lines = lexed(
        Lang::Rust,
        "fn first<'a>(x: &'a str, y: &'a str) -> &'a str { x }",
    );
    assert!(texts_of(&lines, STRING).is_empty(), "{lines:?}");
    assert_eq!(
        texts_of(&lines, TypeName),
        vec!["'a", "'a", "str", "'a", "str", "'a", "str"]
    );
    assert_eq!(class_of(&lines, "first"), Some(Function));
    assert_eq!(class_of(&lines, "->"), Some(Operator));
    // Characters beside lifetimes, one beyond ASCII among them.
    let lines = lexed(Lang::Rust, "let u = '\u{fc}'; let s: &'static str = \"a\";");
    assert_eq!(texts_of(&lines, STRING), vec!["'\u{fc}'", "\"a\""]);
    assert_eq!(texts_of(&lines, TypeName), vec!["'static", "str"]);
}

#[test]
fn a_quote_without_its_closer_is_invalid_alone() {
    let lines = lexed(Lang::Java, "char c = 'x; int y;");
    assert_eq!(texts_of(&lines, Invalid), vec!["'"]);
    assert_eq!(class_of(&lines, "int"), Some(TypeName));
}

#[test]
fn a_block_comment_goes_on_over_lines_and_nests_only_where_the_language_nests() {
    let rust = lexed(Lang::Rust, "/* a /* b */\nstill */ x");
    assert_eq!(rust[1], vec![("still */", Comment)]);
    let c = lexed(Lang::C, "/* a /* b */\nint x;");
    assert_eq!(c[0], vec![("/* a /* b */", Comment)]);
    assert_eq!(c[1][0], ("int", TypeName));
}

#[test]
fn the_state_after_a_line_is_the_same_from_any_start() {
    // Lexing a line again from the state before it gives the same tokens and state.
    use crate::lexer::{LineState, lex_line};
    use crate::types::Token;
    let text = "auto s = R\"x(\nmid )\" \n)x\"; /* c\n*/ int y;";
    let mut state = LineState::default();
    for line in text.split('\n') {
        let mut first: Vec<Token> = Vec::new();
        let mut again: Vec<Token> = Vec::new();
        let next = lex_line(Lang::Cpp, line.as_bytes(), state, &mut first);
        assert_eq!(
            lex_line(Lang::Cpp, line.as_bytes(), state, &mut again),
            next
        );
        assert_eq!(lex_line(Lang::Cpp, line.as_bytes(), state, &mut ()), next);
        assert_eq!(first, again);
        state = next;
    }
    assert_eq!(state, LineState::default());
}
