use super::*;
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation as P, TypeName,
};

const STR: TokenClass = TokenClass::String;

/// The tokens of every line of `text`, each as its text and class, the state carried on.
fn lex(text: &str, dialect: Dialect) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(line.as_bytes(), state, dialect, &mut tokens);
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    (&line[start..start + token.len as usize], token.class)
                })
                .collect()
        })
        .collect()
}

fn standard(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    lex(text, Dialect::Standard)
}

#[test]
fn postgresql_dollar_quoted_bodies_casts_and_escape_strings() {
    let lines = standard(
        "CREATE FUNCTION total(a integer) RETURNS integer AS $$\n  SELECT a + 1; -- 'no string'\n$$ LANGUAGE plpgsql;",
    );
    assert_eq!(
        lines[0],
        vec![
            ("CREATE", Keyword),
            ("FUNCTION", Keyword),
            ("total", Function),
            ("(", P),
            ("integer", TypeName),
            (")", P),
            ("RETURNS", Keyword),
            ("integer", TypeName),
            ("AS", Keyword),
            ("$$", STR),
        ]
    );
    assert_eq!(lines[1], vec![("  SELECT a + 1; -- 'no string'", STR)]);
    assert_eq!(lines[2], vec![("$$", STR), ("LANGUAGE", Keyword), (";", P)]);
    assert_eq!(
        standard("DO $body$ BEGIN RAISE NOTICE 'x$$y'; END $body$;")[0],
        vec![
            ("DO", Keyword),
            ("$body$ BEGIN RAISE NOTICE 'x$$y'; END $body$", STR),
            (";", P),
        ]
    );
    assert_eq!(
        standard("SELECT $1::int, 'it''s', E'a\\'b', \"Order\" FROM t /* c */;")[0],
        vec![
            ("SELECT", Keyword),
            ("$1", Key),
            ("::", Operator),
            ("int", TypeName),
            (",", P),
            ("'it''s'", STR),
            (",", P),
            ("E'a\\'b'", STR),
            (",", P),
            ("\"Order\"", Key),
            ("FROM", Keyword),
            ("/* c */", Comment),
            (";", P),
        ]
    );
    // An array subscript is no T-SQL name; a name after `.` no type.
    assert_eq!(
        standard("SELECT arr[1], t.date, now()")[0],
        vec![
            ("SELECT", Keyword),
            ("[", P),
            ("1", Number),
            ("]", P),
            (",", P),
            (".", P),
            (",", P),
            ("now", Function),
            ("(", P),
            (")", P),
        ]
    );
}

#[test]
fn block_comments_nest_and_strings_run_over_lines() {
    let lines = standard("/* outer /* inner */\nstill */ SELECT 1;\n'a\nb' x");
    assert_eq!(lines[0], vec![("/* outer /* inner */", Comment)]);
    assert_eq!(
        lines[1],
        vec![
            ("still */", Comment),
            ("SELECT", Keyword),
            ("1", Number),
            (";", P)
        ]
    );
    assert_eq!(lines[2], vec![("'a", STR)]);
    assert_eq!(lines[3], vec![("b'", STR)]);
    // A name in backticks or brackets ends with its line.
    let lines = lex("`a\nb [c\nd", Dialect::MySql);
    assert_eq!(lines[0], vec![("`a", Key)]);
    assert_eq!(lines[1], vec![("[", P)]);
    assert!(lines[2].is_empty());
}

#[test]
fn mysql_reads_hash_comments_double_quoted_strings_and_backslashes() {
    let line = "SELECT `id`, \"text\" FROM users WHERE name = 'O\\'Brien'; # done";
    assert_eq!(
        lex(line, Dialect::MySql)[0],
        vec![
            ("SELECT", Keyword),
            ("`id`", Key),
            (",", P),
            ("\"text\"", STR),
            ("FROM", Keyword),
            ("WHERE", Keyword),
            ("=", Operator),
            ("'O\\'Brien'", STR),
            (";", P),
            ("# done", Comment),
        ]
    );
    assert_eq!(
        lex("SET @x := 0x1F, @@sql_mode = 1abc;", Dialect::MySql)[0],
        vec![
            ("SET", Keyword),
            ("@x", Key),
            (":=", Operator),
            ("0x1F", Number),
            (",", P),
            ("@@sql_mode", Key),
            ("=", Operator),
            (";", P),
        ]
    );
    // The standard reading: `"` names, `''` the only escape, `#` no comment.
    let standard = standard("SELECT \"text\" # x");
    assert_eq!(
        standard[0],
        vec![("SELECT", Keyword), ("\"text\"", Key), ("#", Operator)]
    );
}

#[test]
fn sqlite_types_constraints_and_numbers() {
    let line = "CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY AUTOINCREMENT, data BLOB, x REAL DEFAULT 1.5e3, y TEXT NULL) WITHOUT ROWID;";
    assert_eq!(
        standard(line)[0],
        vec![
            ("CREATE", Keyword),
            ("TABLE", Keyword),
            ("IF", Keyword),
            ("NOT", Keyword),
            ("EXISTS", Keyword),
            ("(", P),
            ("INTEGER", TypeName),
            ("PRIMARY", Keyword),
            ("KEY", Keyword),
            ("AUTOINCREMENT", Keyword),
            (",", P),
            ("BLOB", TypeName),
            (",", P),
            ("REAL", TypeName),
            ("DEFAULT", Keyword),
            ("1.5e3", Number),
            (",", P),
            ("TEXT", TypeName),
            ("NULL", Literal),
            (")", P),
            ("WITHOUT", Keyword),
            (";", P),
        ]
    );
    assert_eq!(
        standard("SELECT .5, 42, count(*) -- n")[0],
        vec![
            ("SELECT", Keyword),
            (".5", Number),
            (",", P),
            ("42", Number),
            (",", P),
            ("count", Function),
            ("(", P),
            ("*", Operator),
            (")", P),
            ("-- n", Comment),
        ]
    );
}

#[test]
fn tsql_bracket_names_unicode_strings_variables_and_temporary_tables() {
    let lines = standard(
        "SELECT TOP (10) [Order ID], N'x' FROM [dbo].[Orders] WHERE id = @id;\nGO\nSELECT * FROM #tmp, ##all;",
    );
    assert_eq!(
        lines[0],
        vec![
            ("SELECT", Keyword),
            ("TOP", Keyword),
            ("(", P),
            ("10", Number),
            (")", P),
            ("[Order ID]", Key),
            (",", P),
            ("N'x'", STR),
            ("FROM", Keyword),
            ("[dbo]", Key),
            (".", P),
            ("[Orders]", Key),
            ("WHERE", Keyword),
            ("=", Operator),
            ("@id", Key),
            (";", P),
        ]
    );
    assert_eq!(lines[1], vec![("GO", Keyword)]);
    assert_eq!(
        lines[2],
        vec![
            ("SELECT", Keyword),
            ("*", Operator),
            ("FROM", Keyword),
            (",", P),
            (";", P),
        ]
    );
}

#[test]
fn a_dollar_tag_too_long_to_keep_is_no_quote() {
    let tag = "t".repeat(MAX_TAG + 1);
    let line = format!("${tag}$ x");
    let lines = standard(&line);
    assert_eq!(lines[0][0], ("$", Operator));
    assert!(lines[0].iter().all(|(_, class)| *class != STR));
    assert!(Tag::new(&[b'a'; MAX_TAG]).is_some());
}

#[test]
fn every_token_sits_on_character_boundaries() {
    let line = "SELECT 'Gr\u{fc}\u{df}e', caf\u{e9}(\u{e9}) /* \u{4e00} */ \u{fc}ber";
    let mut tokens: Vec<Token> = Vec::new();
    lex_line(
        line.as_bytes(),
        LineState::default(),
        Dialect::Standard,
        &mut tokens,
    );
    for token in &tokens {
        let start = token.start as usize;
        assert!(line.get(start..start + token.len as usize).is_some());
    }
    assert_eq!(tokens.len(), 7);
}
