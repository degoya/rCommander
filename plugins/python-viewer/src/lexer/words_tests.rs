use super::*;

fn number(text: &str) -> &str {
    &text[..number_end(text.as_bytes(), 0)]
}

#[test]
fn numbers_in_every_form_end_where_python_ends_them() {
    for whole in [
        "0",
        "42",
        "1_000_000",
        "0x_FF",
        "0XdeadBEEF",
        "0o17",
        "0O7_7",
        "0b1010",
        "0B_1_0",
        "1.5",
        ".5",
        "1.",
        "1_0.0_1",
        "6.02e23",
        "6.02E+23",
        "1e-3",
        "1_0e1_0",
        "2j",
        "1.5J",
        "1e3j",
        ".5j",
    ] {
        assert_eq!(number(whole), whole, "{whole}");
    }
    for (text, wanted) in [
        ("1if x else 2", "1"),
        ("1else", "1"),
        ("0x1F)", "0x1F"),
        ("0o78", "0o7"),
        ("0b102", "0b10"),
        ("3.real", "3."),
        ("1e", "1"),
        ("1e+", "1"),
        ("10,", "10"),
        ("2jx", "2j"),
    ] {
        assert_eq!(number(text), wanted, "{text}");
    }
}

#[test]
fn string_prefixes_in_any_case() {
    for prefix in [
        "r", "u", "b", "f", "t", "R", "U", "B", "F", "T", "br", "rb", "Br", "rB", "BR", "fr", "rf",
        "Rf", "fR", "tr", "rt", "TR",
    ] {
        assert!(is_string_prefix(prefix.as_bytes()), "{prefix}");
    }
    for word in ["", "x", "bf", "fb", "ub", "ur", "rr", "brr", "print", "bu"] {
        assert!(!is_string_prefix(word.as_bytes()), "{word}");
    }
}

#[test]
fn operators_take_their_longest_form() {
    for (text, width) in [
        ("**=", 3),
        ("//= 2", 3),
        (">>=", 3),
        ("**2", 2),
        ("->", 2),
        (":=", 2),
        ("!=", 2),
        ("@=", 2),
        ("==", 2),
        ("+1", 1),
        ("~x", 1),
        ("@m", 1),
        ("=", 1),
    ] {
        assert_eq!(operator_width(text.as_bytes()), Some(width), "{text}");
    }
    for text in ["", "x", "(", ",", "$", "?"] {
        assert_eq!(operator_width(text.as_bytes()), None, "{text}");
    }
}

fn soft(line: &str) -> bool {
    let end = name_end(line.as_bytes(), 0);
    is_soft_keyword(line.as_bytes(), &line.as_bytes()[..end], end)
}

#[test]
fn soft_keywords_only_where_they_head_a_statement() {
    for line in [
        "match command.split():",
        "match (x, y):",
        "match x:  # the subject",
        "case [x, y, *rest]:",
        "case {\"key\": value}:",
        "case Point(x=0) if y > 0:",
        "case -1 | 2:",
        "case 'a:b':",
        "case _:",
        "type Pair = tuple[int, int]",
        "type Pair[T] = tuple[T, T]",
        "type  Tree[T: (int, str)]  =  list[T]",
    ] {
        assert!(soft(line), "{line}");
    }
    for line in [
        "match = re.match(pattern, text)",
        "match.group(1)",
        "match(x)",
        "match: int = 1",
        "match",
        "case, other = 1, 2",
        "case = {'a': 1}",
        "type(x)",
        "type = 'a'",
        "type X == 1",
        "type X",
        "type[int]",
        "type Pair[T = 1",
        "print(x):",
    ] {
        assert!(!soft(line), "{line}");
    }
}

#[test]
fn built_in_names_have_their_class_and_others_none() {
    for word in [
        "print",
        "len",
        "isinstance",
        "super",
        "enumerate",
        "__import__",
    ] {
        assert_eq!(
            builtin_class(word.as_bytes()),
            Some(TokenClass::Function),
            "{word}"
        );
    }
    for word in [
        "int",
        "str",
        "dict",
        "type",
        "ValueError",
        "Exception",
        "UserWarning",
    ] {
        assert_eq!(
            builtin_class(word.as_bytes()),
            Some(TokenClass::TypeName),
            "{word}"
        );
    }
    for word in [
        "self", "printer", "Print", "MyError", "_print", "__init__", "",
    ] {
        assert_eq!(builtin_class(word.as_bytes()), None, "{word}");
    }
    assert!(is_keyword(b"lambda") && is_keyword(b"nonlocal") && !is_keyword(b"match"));
    assert!(is_literal(b"None") && is_literal(b"__debug__") && !is_literal(b"none"));
}
