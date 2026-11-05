use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{self, Keyword, Punctuation, TypeName};

const STRING: TokenClass = TokenClass::String;

#[test]
fn cpp_raw_strings_close_at_their_delimiter_also_lines_later() {
    let lines = lexed(
        Lang::Cpp,
        "auto s = R\"x(a)\" b)x\";\nauto t = R\"sql(\nSELECT \")\" FROM t\n)sql\";",
    );
    assert_eq!(texts_of(&lines[..1], STRING), vec!["R\"x(a)\" b)x\""]);
    assert_eq!(lines[1].last(), Some(&("R\"sql(", STRING)));
    assert_eq!(lines[2], vec![("SELECT \")\" FROM t", STRING)]);
    assert_eq!(lines[3], vec![(")sql\"", STRING), (";", Punctuation)]);
}

#[test]
fn csharp_verbatim_interpolated_and_raw_strings() {
    let lines = lexed(
        Lang::CSharp,
        concat!(
            "var a = @\"C:\\dir\\\"\"quoted\"\"\";\n",
            "var b = $\"{name,10:N2} {{x}} {(ok ? \"y\" : \"n\")}\";\n",
            "var c = $$\"\"\"{\"k\": {{v}}}\"\"\";\n",
            "var d = @$\"line\n",
            "{x}\";",
        ),
    );
    assert_eq!(
        texts_of(&lines[..1], STRING),
        vec!["@\"C:\\dir\\\"\"quoted\"\"\""]
    );
    assert_eq!(
        texts_of(&lines[1..2], STRING),
        vec!["$\"", "N2", " {{x}} ", "\"y\"", "\"n\"", "\""]
    );
    assert_eq!(
        texts_of(&lines[1..2], Punctuation),
        vec!["{", ",", ":", "}", "{", "(", ":", ")", "}", ";"]
    );
    assert_eq!(
        texts_of(&lines[2..3], STRING),
        vec!["$$\"\"\"{\"k\": ", "}\"\"\""]
    );
    assert_eq!(texts_of(&lines[2..3], Punctuation), vec!["{{", "}}", ";"]);
    assert_eq!(texts_of(&lines[3..4], STRING), vec!["@$\"line"]);
    assert_eq!(
        lines[4],
        vec![
            ("{", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
            (";", Punctuation)
        ]
    );
}

#[test]
fn java_text_blocks_and_characters() {
    let lines = lexed(
        Lang::Java,
        "String s = \"\"\"\n    He said \"hi\" \\\"\"\"\n    \"\"\";\nchar c = '\\'';",
    );
    assert_eq!(lines[0][0], ("String", TypeName));
    assert_eq!(lines[0].last(), Some(&("\"\"\"", STRING)));
    assert_eq!(lines[1], vec![("    He said \"hi\" \\\"\"\"", STRING)]);
    assert_eq!(lines[2], vec![("    \"\"\"", STRING), (";", Punctuation)]);
    assert_eq!(class_of(&lines[3..], "'\\''"), Some(STRING));
}

#[test]
fn kotlin_templates_in_strings_and_raw_strings() {
    let lines = lexed(
        Lang::Kotlin,
        "val s = \"Hi $name, ${user.age + 1} \\$x\"\nval r = \"\"\"raw $x ${\"in\"}\"\"\"",
    );
    assert_eq!(
        texts_of(&lines[..1], STRING),
        vec!["\"Hi ", ", ", " \\$x\""]
    );
    assert_eq!(
        texts_of(&lines[..1], Punctuation),
        vec!["$", "${", ".", "}"]
    );
    assert_eq!(
        texts_of(&lines[1..], STRING),
        vec!["\"\"\"raw ", " ", "\"in\"", "\"\"\""]
    );
}

#[test]
fn go_raw_strings_go_on_over_lines_and_runes_are_characters() {
    let lines = lexed(Lang::Go, "s := `raw \\n\nline`\nr := 'x'");
    assert_eq!(lines[0].last(), Some(&("`raw \\n", STRING)));
    assert_eq!(lines[1], vec![("line`", STRING)]);
    assert_eq!(class_of(&lines[2..], "'x'"), Some(STRING));
    assert_eq!(class_of(&lines[..1], ":="), Some(TokenClass::Operator));
}

#[test]
fn rust_raw_byte_and_c_strings_and_raw_names() {
    let lines = lexed(
        Lang::Rust,
        concat!(
            "let a = r#\"say \"hi\"\"#;\n",
            "let b = br##\"x\"#\"##;\n",
            "let c = b\"bytes\\n\";\n",
            "let d = \"multi\n",
            "line\";\n",
            "let r#type = 1;\n",
            "let e = c\"cstr\";",
        ),
    );
    let strings: Vec<_> = lines
        .iter()
        .map(|line| texts_of(std::slice::from_ref(line), STRING))
        .collect();
    assert_eq!(strings[0], vec!["r#\"say \"hi\"\"#"]);
    assert_eq!(strings[1], vec!["br##\"x\"#\"##"]);
    assert_eq!(strings[2], vec!["b\"bytes\\n\""]);
    assert_eq!(strings[3], vec!["\"multi"]);
    assert_eq!(strings[4], vec!["line\""]);
    assert!(strings[5].is_empty());
    assert_eq!(texts_of(&lines[5..6], Keyword), vec!["let"]);
    assert_eq!(strings[6], vec!["c\"cstr\""]);
}

#[test]
fn a_one_line_string_left_open_ends_with_its_line() {
    // Java: the next line is code again.
    let lines = lexed(Lang::Java, "s = \"open\nint x;");
    assert_eq!(lines[1][0], ("int", TypeName));
    // C: a backslash at the line's end carries the string on.
    let lines = lexed(Lang::C, "s = \"go \\\non\";\nint x;");
    assert_eq!(lines[1], vec![("on\"", STRING), (";", Punctuation)]);
    assert_eq!(lines[2][0], ("int", TypeName));
}
