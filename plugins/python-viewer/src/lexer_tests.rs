use super::*;
use crate::types::TokenClass::{
    Comment, Function, Keyword, Literal, Number, Operator, Punctuation, TypeName,
};

pub(crate) const STRING: TokenClass = TokenClass::String;

/// The tokens of each line of `text`, as the text they cover, lexed in order from the start.
pub(crate) fn lines(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(line.as_bytes(), state, &mut tokens);
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    let end = start + token.len as usize;
                    (
                        line.get(start..end).expect("on character boundaries"),
                        token.class,
                    )
                })
                .collect()
        })
        .collect()
}

pub(crate) fn one(line: &str) -> Vec<(&str, TokenClass)> {
    lines(line).remove(0)
}

#[test]
fn every_hard_keyword_is_a_keyword_wherever_it_stands() {
    for keyword in [
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield",
    ] {
        assert_eq!(one(keyword), vec![(keyword, Keyword)], "{keyword}");
    }
    for literal in [
        "True",
        "False",
        "None",
        "Ellipsis",
        "NotImplemented",
        "__debug__",
    ] {
        assert_eq!(one(literal), vec![(literal, Literal)], "{literal}");
    }
    // Case matters.
    assert!(one("If TRUE none").is_empty());
    assert_eq!(
        one("from . import x"),
        vec![("from", Keyword), (".", Punctuation), ("import", Keyword)]
    );
}

#[test]
fn definitions_name_their_function_and_their_class() {
    assert_eq!(
        one("def f(x): return None"),
        vec![
            ("def", Keyword),
            ("f", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
            ("return", Keyword),
            ("None", Literal),
        ]
    );
    assert_eq!(
        one("class Point(Base, metaclass=ABCMeta):"),
        vec![
            ("class", Keyword),
            ("Point", TypeName),
            ("(", Punctuation),
            (",", Punctuation),
            ("=", Operator),
            (")", Punctuation),
            (":", Punctuation),
        ]
    );
    assert_eq!(
        one("async def main() -> int:"),
        vec![
            ("async", Keyword),
            ("def", Keyword),
            ("main", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("->", Operator),
            ("int", TypeName),
            (":", Punctuation),
        ]
    );
    // A name that spells a built-in is the name defined.
    assert_eq!(one("def print():")[1], ("print", Function));
}

#[test]
fn built_in_names_but_not_attributes_of_the_same_name() {
    assert_eq!(
        one("print(len(x), int(\"3\"), ValueError)"),
        vec![
            ("print", Function),
            ("(", Punctuation),
            ("len", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (",", Punctuation),
            ("int", TypeName),
            ("(", Punctuation),
            ("\"3\"", STRING),
            (")", Punctuation),
            (",", Punctuation),
            ("ValueError", TypeName),
            (")", Punctuation),
        ]
    );
    assert_eq!(
        one("os.path.print"),
        vec![(".", Punctuation), (".", Punctuation)]
    );
}

#[test]
fn decorators_where_a_statement_starts_and_matrix_products_elsewhere() {
    assert_eq!(one("@property"), vec![("@property", Function)]);
    assert_eq!(
        one("    @app.route(\"/\")"),
        vec![
            ("@app.route", Function),
            ("(", Punctuation),
            ("\"/\"", STRING),
            (")", Punctuation),
        ]
    );
    assert_eq!(one("x = a @ b"), vec![("=", Operator), ("@", Operator)]);
    assert_eq!(one("m @= n"), vec![("@=", Operator)]);
    assert_eq!(one("@"), vec![("@", Operator)]);
}

#[test]
fn soft_keywords_head_their_blocks_and_stay_names_elsewhere() {
    let text = "match command.split():\n    case [action]:\n        pass\n    case _:\n        type = action";
    assert_eq!(
        lines(text),
        vec![
            vec![
                ("match", Keyword),
                (".", Punctuation),
                ("(", Punctuation),
                (")", Punctuation),
                (":", Punctuation),
            ],
            vec![
                ("case", Keyword),
                ("[", Punctuation),
                ("]", Punctuation),
                (":", Punctuation),
            ],
            vec![("pass", Keyword)],
            vec![("case", Keyword), (":", Punctuation)],
            vec![("type", TypeName), ("=", Operator)],
        ]
    );
    assert_eq!(
        one("type Point = tuple[float, float]"),
        vec![
            ("type", Keyword),
            ("Point", TypeName),
            ("=", Operator),
            ("tuple", TypeName),
            ("[", Punctuation),
            ("float", TypeName),
            (",", Punctuation),
            ("float", TypeName),
            ("]", Punctuation),
        ]
    );
    assert_eq!(
        one("match = re.match(p, s)"),
        vec![
            ("=", Operator),
            (".", Punctuation),
            ("(", Punctuation),
            (",", Punctuation),
            (")", Punctuation),
        ]
    );
    // After `;` a statement starts too.
    assert_eq!(
        one("x = 1; type T = int"),
        vec![
            ("=", Operator),
            ("1", Number),
            (";", Punctuation),
            ("type", Keyword),
            ("T", TypeName),
            ("=", Operator),
            ("int", TypeName),
        ]
    );
}

#[test]
fn inside_brackets_and_after_a_backslash_no_statement_starts() {
    assert_eq!(
        lines("x = (\n    match y:\n)"),
        vec![
            vec![("=", Operator), ("(", Punctuation)],
            vec![(":", Punctuation)],
            vec![(")", Punctuation)],
        ]
    );
    assert_eq!(
        lines("y = 1 + \\\n    @decorator\n@decorator"),
        vec![
            vec![("=", Operator), ("1", Number), ("+", Operator)],
            vec![("@", Operator)],
            vec![("@decorator", Function)],
        ]
    );
    // Brackets of every kind count, and the statement after them starts again.
    assert_eq!(
        lines("d = {\n  'k': [1,\n  2]}\nmatch x:")[3][0],
        ("match", Keyword)
    );
}

#[test]
fn comments_numbers_and_operators() {
    assert_eq!(
        one("x = \"#no\"  # yes"),
        vec![("=", Operator), ("\"#no\"", STRING), ("# yes", Comment)]
    );
    assert_eq!(
        one("n = 1_000 + 0xFF * 1.5e-3j - .5"),
        vec![
            ("=", Operator),
            ("1_000", Number),
            ("+", Operator),
            ("0xFF", Number),
            ("*", Operator),
            ("1.5e-3j", Number),
            ("-", Operator),
            (".5", Number),
        ]
    );
    assert_eq!(
        one("a //= b ** -c"),
        vec![("//=", Operator), ("**", Operator), ("-", Operator)]
    );
    assert_eq!(
        one("if (n := len(a)) > 10:"),
        vec![
            ("if", Keyword),
            ("(", Punctuation),
            (":=", Operator),
            ("len", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (")", Punctuation),
            (">", Operator),
            ("10", Number),
            (":", Punctuation),
        ]
    );
    assert_eq!(
        one("x[1:2, ...]"),
        vec![
            ("[", Punctuation),
            ("1", Number),
            (":", Punctuation),
            ("2", Number),
            (",", Punctuation),
            ("...", Punctuation),
            ("]", Punctuation),
        ]
    );
    // A name with digits is a name; a stray character is skipped.
    assert!(one("x1 = $").iter().all(|token| token.1 == Operator));
}

#[test]
fn open_records_strings_a_line_or_the_file_leaves_open() {
    let mut opens = Opens::default();
    let mut state = LineState::default();
    for line in [
        "a = 'open",
        "ok = \"x\" + 'y' + \"\"\"z\"\"\"",
        "s = 'goes \\",
        "on'",
        "b = rb\"\"\"doc",
        "still",
    ] {
        state = lex_line(line.as_bytes(), state, &mut opens);
        opens.line += 1;
    }
    assert_eq!(
        opens.into_unclosed(),
        vec![
            QuoteOpen {
                line: 0,
                col: 4,
                width: 1,
            },
            QuoteOpen {
                line: 4,
                col: 4,
                width: 5,
            },
        ]
    );
    assert!(state.frames.top().is_some_and(Frame::is_text));
}

#[test]
fn a_long_line_keeps_at_most_its_token_cap() {
    let line = "x, ".repeat(MAX_TOKENS_PER_LINE + 10);
    let mut tokens: Vec<Token> = Vec::new();
    lex_line(line.as_bytes(), LineState::default(), &mut tokens);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
}
