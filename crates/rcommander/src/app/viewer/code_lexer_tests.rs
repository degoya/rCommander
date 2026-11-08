//! The code block lexer (RC-110-06): each language's tokens, the bounds, the tags.

use super::{LineTokens, MAX_BLOCK_BYTES, MAX_LINE_BYTES, colours, lang_of, lex_block, lex_line};
use crate::app::viewer::{document::TokenClass, markdown};

use TokenClass as T;

/// The tokens of `line` in the language `tag`, as (text, class).
fn tokens(tag: &str, line: &str) -> Vec<(String, TokenClass)> {
    let lang = lang_of(tag).expect("a known tag");
    texts(line, &lex_line(lang, line, &mut None))
}

fn texts(line: &str, tokens: &LineTokens) -> Vec<(String, TokenClass)> {
    tokens
        .iter()
        .map(|(range, class)| (line[range.clone()].to_owned(), *class))
        .collect()
}

fn has(found: &[(String, TokenClass)], text: &str, class: TokenClass) -> bool {
    found.iter().any(|(t, c)| t == text && *c == class)
}

#[test]
fn rust_keywords_types_strings_numbers_comments_and_lifetimes() {
    let found = tokens(
        "rust",
        "pub fn name(x: &'a str) -> Option<u32> { let c = 'x'; f(\"a // b\", 0x1F) } // end",
    );
    assert!(has(&found, "pub", T::Keyword));
    assert!(has(&found, "fn", T::Keyword));
    assert!(has(&found, "name", T::Function));
    assert!(has(&found, "'a", T::Literal), "{found:?}");
    assert!(has(&found, "str", T::TypeName));
    assert!(has(&found, "Option", T::TypeName));
    assert!(has(&found, "'x'", T::String));
    assert!(
        has(&found, "\"a // b\"", T::String),
        "a comment inside a string is the string"
    );
    assert!(has(&found, "0x1F", T::Number));
    assert!(has(&found, "// end", T::Comment));
}

#[test]
fn json_keys_differ_from_values() {
    let found = tokens(
        "json",
        r#"{"name": "rc", "n":1.5, "ok": true, "none": null}"#,
    );
    assert!(has(&found, "\"name\"", T::Key));
    assert!(has(&found, "\"n\"", T::Key));
    assert!(has(&found, "\"rc\"", T::String));
    assert!(has(&found, "1.5", T::Number));
    assert!(has(&found, "true", T::Literal));
    assert!(has(&found, "null", T::Literal));
}

#[test]
fn yaml_toml_and_ini_keys_sections_and_comments() {
    let yaml = tokens("yml", "- url: http://example.com # where");
    assert!(has(&yaml, "url", T::Key));
    assert!(
        !has(&yaml, "http", T::Key),
        "a colon without a space after it is no key"
    );
    assert!(has(&yaml, "# where", T::Comment));
    let toml = tokens("toml", "[package.metadata]");
    assert!(has(&toml, "[package.metadata]", T::TypeName));
    let toml = tokens("toml", "rust-version = \"1.99\" # pinned");
    assert!(has(&toml, "rust-version", T::Key));
    assert!(has(&toml, "\"1.99\"", T::String));
    let ini = tokens("ini", "; a comment");
    assert_eq!(ini, vec![("; a comment".to_owned(), T::Comment)]);
}

#[test]
fn shell_variables_keywords_and_a_hash_inside_a_word() {
    let found = tokens(
        "sh",
        "if [ \"$HOME\" ]; then echo ${PATH} $# a#b; fi # done",
    );
    assert!(has(&found, "if", T::Keyword));
    assert!(has(&found, "\"$HOME\"", T::String));
    assert!(has(&found, "${PATH}", T::Literal));
    assert!(has(&found, "fi", T::Keyword));
    assert!(has(&found, "# done", T::Comment));
    assert!(
        !found
            .iter()
            .any(|(t, c)| *c == T::Comment && t.starts_with("# a")),
        "`$#` and `a#b` start no comment: {found:?}"
    );
}

#[test]
fn python_sql_c_powershell_and_script_basics() {
    let py = tokens("python", "def f(x): return None  # no");
    assert!(has(&py, "def", T::Keyword) && has(&py, "None", T::Literal));
    assert!(has(&py, "# no", T::Comment));
    let sql = tokens("sql", "SELECT name FROM users WHERE id = 1 -- one");
    assert!(has(&sql, "SELECT", T::Keyword) && has(&sql, "FROM", T::Keyword));
    assert!(has(&sql, "-- one", T::Comment));
    let c = tokens("cpp", "int main(void) { return NULL; }");
    assert!(has(&c, "int", T::TypeName) && has(&c, "main", T::Function));
    assert!(has(&c, "NULL", T::Literal));
    let ps = tokens(
        "powershell",
        "if ($x -eq 1) { Write-Host \"a`\"b\" } <# c #>",
    );
    assert!(has(&ps, "$x", T::Literal) && has(&ps, "eq", T::Keyword));
    assert!(has(&ps, "\"a`\"b\"", T::String), "{ps:?}");
    assert!(has(&ps, "<# c #>", T::Comment));
    let ts = tokens("ts", "const x: number = `t` ?? undefined;");
    assert!(has(&ts, "const", T::Keyword) && has(&ts, "number", T::TypeName));
    assert!(has(&ts, "`t`", T::String) && has(&ts, "undefined", T::Literal));
}

#[test]
fn markup_tags_attributes_and_comments() {
    let found = tokens("html", "<a href=\"x\"></a> <!-- note -->");
    assert!(has(&found, "a", T::Keyword));
    assert!(has(&found, "href", T::Key));
    assert!(has(&found, "\"x\"", T::String));
    assert!(has(&found, "<!-- note -->", T::Comment));
}

#[test]
fn diff_lines_by_their_start() {
    let lines: Vec<String> = ["--- a/x", "+++ b/x", "@@ -1 +1 @@", "-old", "+new", " same"]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    let lang = lang_of("diff").expect("diff");
    let found = lex_block(lang, &lines).expect("small");
    let classes: Vec<Option<TokenClass>> = found
        .iter()
        .map(|line| line.first().map(|(_, class)| *class))
        .collect();
    assert_eq!(
        classes,
        vec![
            Some(T::Comment),
            Some(T::Comment),
            Some(T::Keyword),
            Some(T::Invalid),
            Some(T::Number),
            None
        ]
    );
}

#[test]
fn a_block_comment_and_a_triple_quoted_string_span_lines() {
    let lang = lang_of("c").expect("c");
    let lines: Vec<String> = ["int a; /* one", "two", "three */ int b;"]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    let found = lex_block(lang, &lines).expect("small");
    assert!(has(&texts(&lines[0], &found[0]), "/* one", T::Comment));
    assert_eq!(
        texts(&lines[1], &found[1]),
        vec![("two".to_owned(), T::Comment)]
    );
    let last = texts(&lines[2], &found[2]);
    assert!(has(&last, "three */", T::Comment) && has(&last, "int", T::TypeName));
    let py = lang_of("py").expect("python");
    let lines: Vec<String> = ["x = \"\"\"doc", "end\"\"\" + y"]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    let found = lex_block(py, &lines).expect("small");
    assert!(has(&texts(&lines[1], &found[1]), "end\"\"\"", T::String));
}

#[test]
fn tags_name_their_language_by_the_first_word_and_nothing_is_guessed() {
    for tag in [
        "rust",
        "RS",
        "sh title=\"x\"",
        "{.python}",
        "json,ignore",
        "yml",
        "ps1",
    ] {
        assert!(lang_of(tag).is_some(), "{tag}");
    }
    for tag in ["", "text", "brainfuck", "  "] {
        assert!(lang_of(tag).is_none(), "{tag:?}");
    }
}

#[test]
fn work_is_bounded_per_block_and_per_line() {
    let lang = lang_of("rust").expect("rust");
    let line = "let x = 1;".to_owned();
    let many = vec![line.clone(); MAX_BLOCK_BYTES / line.len() + 1];
    assert!(
        lex_block(lang, &many).is_none(),
        "a block over the limit stays plain"
    );
    let long = vec!["x".repeat(MAX_LINE_BYTES + 1), line];
    let found = lex_block(lang, &long).expect("under the block limit");
    assert!(found[0].is_empty(), "a line over the limit stays plain");
    assert!(!found[1].is_empty());
}

#[test]
fn every_code_block_is_numbered_and_only_a_known_tag_is_coloured() {
    let source =
        "```rust\nfn a() {}\n```\n\n- item\n\n  ```\n  plain\n  ```\n\n> ```sh\n> ls\n> ```\n";
    let preview =
        markdown::parse(source.as_bytes(), rc_engine::viewer::detect(b"")).expect("parses");
    let found = colours(&preview.items);
    assert_eq!(
        found.len(),
        3,
        "the block in the list and the quote count too"
    );
    assert!(found[0].is_some());
    assert!(found[1].is_none(), "no tag: plain");
    assert!(found[2].is_some());
}
