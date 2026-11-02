use crate::lexer::tests::{STRING, lines, one};
use crate::types::TokenClass::{Function, Number, Operator, Punctuation};

#[test]
fn every_prefix_in_any_case_belongs_to_its_string() {
    assert_eq!(
        one(r#"x = r"a\"b" + b'c' + f"d" + u"e" + rb"f" + BR'g' + Rf"h" + t"i" + Tr'j'"#),
        vec![
            ("=", Operator),
            (r#"r"a\"b""#, STRING),
            ("+", Operator),
            ("b'c'", STRING),
            ("+", Operator),
            ("f\"d\"", STRING),
            ("+", Operator),
            ("u\"e\"", STRING),
            ("+", Operator),
            ("rb\"f\"", STRING),
            ("+", Operator),
            ("BR'g'", STRING),
            ("+", Operator),
            ("Rf\"h\"", STRING),
            ("+", Operator),
            ("t\"i\"", STRING),
            ("+", Operator),
            ("Tr'j'", STRING),
        ]
    );
    // A name that is no prefix stays a name before its string.
    assert_eq!(one("print'x'"), vec![("print", Function), ("'x'", STRING)]);
}

#[test]
fn escapes_keep_a_quote_in_its_string() {
    assert_eq!(
        one(r#"s = "a\\" + 'it\'s' + "say \"hi\"""#),
        vec![
            ("=", Operator),
            (r#""a\\""#, STRING),
            ("+", Operator),
            (r"'it\'s'", STRING),
            ("+", Operator),
            (r#""say \"hi\"""#, STRING),
        ]
    );
}

#[test]
fn triple_quotes_span_lines_and_hold_single_quotes() {
    let text = "doc = \"\"\"First \"quoted\" and \"\"two\"\"\nsecond ''' line\n\"\"\"; x = 1";
    assert_eq!(
        lines(text),
        vec![
            vec![
                ("=", Operator),
                ("\"\"\"First \"quoted\" and \"\"two\"\"", STRING),
            ],
            vec![("second ''' line", STRING)],
            vec![
                ("\"\"\"", STRING),
                (";", Punctuation),
                ("=", Operator),
                ("1", Number),
            ],
        ]
    );
    assert_eq!(
        lines("'''\nx\n'''"),
        vec![
            vec![("'''", STRING)],
            vec![("x", STRING)],
            vec![("'''", STRING)]
        ]
    );
    assert_eq!(
        one("e = \"\"\"\"\"\" + \"\" + ''"),
        vec![
            ("=", Operator),
            ("\"\"\"\"\"\"", STRING),
            ("+", Operator),
            ("\"\"", STRING),
            ("+", Operator),
            ("''", STRING),
        ]
    );
}

#[test]
fn f_string_fields_are_code_between_text() {
    assert_eq!(
        one("f\"a{x}b\""),
        vec![
            ("f\"a", STRING),
            ("{", Punctuation),
            ("}", Punctuation),
            ("b\"", STRING),
        ]
    );
    assert_eq!(
        one("print(f\"{len(items)!r:>10} items\")"),
        vec![
            ("print", Function),
            ("(", Punctuation),
            ("f\"", STRING),
            ("{", Punctuation),
            ("len", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("!r", Operator),
            (":", Punctuation),
            (">10", STRING),
            ("}", Punctuation),
            (" items\"", STRING),
            (")", Punctuation),
        ]
    );
    // `{{` and `}}` are braces of the text; a bytes or plain string has no fields.
    assert_eq!(one("f\"{{x}} {{\""), vec![("f\"{{x}} {{\"", STRING)]);
    assert_eq!(one("b\"{x}\""), vec![("b\"{x}\"", STRING)]);
    assert_eq!(one("'{x}'"), vec![("'{x}'", STRING)]);
    // `!=` in a field is the operator.
    assert_eq!(
        one("f'{a != b}'"),
        vec![
            ("f'", STRING),
            ("{", Punctuation),
            ("!=", Operator),
            ("}", Punctuation),
            ("'", STRING),
        ]
    );
}

#[test]
fn fields_hold_strings_in_the_same_quote_and_other_f_strings() {
    assert_eq!(
        one("f\"{x[\"a\"]}\""),
        vec![
            ("f\"", STRING),
            ("{", Punctuation),
            ("[", Punctuation),
            ("\"a\"", STRING),
            ("]", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
    assert_eq!(
        one("f\"{f\"{1}\"}\""),
        vec![
            ("f\"", STRING),
            ("{", Punctuation),
            ("f\"", STRING),
            ("{", Punctuation),
            ("1", Number),
            ("}", Punctuation),
            ("\"", STRING),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
    // A dict in a field: its `:` and `}` are the dict's.
    assert_eq!(
        one("f\"{ {'a': 1}['a'] }\""),
        vec![
            ("f\"", STRING),
            ("{", Punctuation),
            ("{", Punctuation),
            ("'a'", STRING),
            (":", Punctuation),
            ("1", Number),
            ("}", Punctuation),
            ("[", Punctuation),
            ("'a'", STRING),
            ("]", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
}

#[test]
fn format_specs_are_text_with_fields_of_their_own() {
    assert_eq!(
        one("f\"{v:{w}.{p}f}\""),
        vec![
            ("f\"", STRING),
            ("{", Punctuation),
            (":", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            (".", STRING),
            ("{", Punctuation),
            ("}", Punctuation),
            ("f", STRING),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
}

#[test]
fn named_characters_are_no_field_but_in_a_raw_string() {
    assert_eq!(
        one(r#"f"\N{BULLET} {x}""#),
        vec![
            (r#"f"\N{BULLET} "#, STRING),
            ("{", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
    assert_eq!(
        one(r#"rf"\N{x}""#),
        vec![
            (r#"rf"\N"#, STRING),
            ("{", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
        ]
    );
}

#[test]
fn fields_go_on_over_lines() {
    let text = "msg = f\"\"\"Total: {\n    sum(values)  # all of them\n}\"\"\"";
    assert_eq!(
        lines(text),
        vec![
            vec![
                ("=", Operator),
                ("f\"\"\"Total: ", STRING),
                ("{", Punctuation)
            ],
            vec![
                ("sum", Function),
                ("(", Punctuation),
                (")", Punctuation),
                ("# all of them", crate::types::TokenClass::Comment),
            ],
            vec![("}", Punctuation), ("\"\"\"", STRING)],
        ]
    );
    // As Python 3.12 reads them, also in a one-quote f-string.
    assert_eq!(
        lines("f\"{\n    x\n}\""),
        vec![
            vec![("f\"", STRING), ("{", Punctuation)],
            vec![],
            vec![("}", Punctuation), ("\"", STRING)],
        ]
    );
}

#[test]
fn a_one_quote_string_ends_with_its_line_unless_a_backslash_continues_it() {
    assert_eq!(
        lines("x = 'abc\ny = 1"),
        vec![
            vec![("=", Operator), ("'abc", STRING)],
            vec![("=", Operator), ("1", Number)],
        ]
    );
    assert_eq!(
        lines("s = 'abc\\\ndef' + x"),
        vec![
            vec![("=", Operator), ("'abc\\", STRING)],
            vec![("def'", STRING), ("+", Operator)],
        ]
    );
    assert_eq!(
        lines("x = '''abc\nmore"),
        vec![
            vec![("=", Operator), ("'''abc", STRING)],
            vec![("more", STRING)]
        ]
    );
}

#[test]
fn strings_nested_past_the_kept_frames_are_read_on_their_line() {
    let line = format!("{}f\"{{1}}\"{}", "f\"{".repeat(4), "}\"".repeat(4));
    let mut wanted = Vec::new();
    for _ in 0..4 {
        wanted.extend([("f\"", STRING), ("{", Punctuation)]);
    }
    wanted.push(("f\"{1}\"", STRING));
    for _ in 0..4 {
        wanted.extend([("}", Punctuation), ("\"", STRING)]);
    }
    let source = format!("{line}\nx = 1");
    let got = lines(&source);
    assert_eq!(got[0], wanted);
    assert_eq!(got[1], vec![("=", Operator), ("1", Number)]);
}
