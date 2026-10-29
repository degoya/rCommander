use super::SearchReplace;
use crate::error::RenameError;

fn search(find: &str, replace: &str) -> SearchReplace {
    SearchReplace {
        find: find.to_owned(),
        replace: replace.to_owned(),
        ..SearchReplace::default()
    }
}

fn applied(search: &SearchReplace, text: &str) -> String {
    match search.compile() {
        Ok(Some(replacer)) => replacer.apply(text),
        Ok(None) => text.to_owned(),
        Err(error) => panic!("{}: {error}", search.find),
    }
}

#[test]
fn search_and_replace_follows_its_options() {
    let regex = |find: &str, replace: &str| SearchReplace {
        regex: true,
        ..search(find, replace)
    };
    let cases = [
        // Plain text: every occurrence, case ignored, regex characters literal.
        (search("a", "o"), "Banana", "Bonono".to_owned()),
        (search("a.b", "-"), "a.b axb A.B", "- axb -".to_owned()),
        (search("$1", "x"), "cost $1", "cost x".to_owned()),
        (search("", "x"), "same", "same".to_owned()),
        (
            SearchReplace {
                case_sensitive: true,
                ..search("a", "o")
            },
            "Banana A",
            "Bonono A".to_owned(),
        ),
        (
            SearchReplace {
                first_only: true,
                ..search("a", "o")
            },
            "Banana",
            "Bonana".to_owned(),
        ),
        // A plain replacement keeps a `$` as it is.
        (search("x", "$1"), "axb", "a$1b".to_owned()),
        // Regular expressions: groups by number and by name, `$$` for a dollar.
        (
            regex(r"(\d+)-(\d+)", "$2-$1"),
            "img 01-02",
            "img 02-01".to_owned(),
        ),
        (
            regex(r"(?P<year>\d{4})_(?P<rest>.*)", "${rest} (${year})"),
            "2019_holiday",
            "holiday (2019)".to_owned(),
        ),
        (regex(r"\s+", "$$"), "a  b c", "a$b$c".to_owned()),
        (regex("^the ", ""), "The Band", "Band".to_owned()),
        (
            SearchReplace {
                case_sensitive: true,
                ..regex("^the ", "")
            },
            "The Band",
            "The Band".to_owned(),
        ),
        (
            SearchReplace {
                first_only: true,
                ..regex(r"\d", "#")
            },
            "a1b2c3",
            "a#b2c3".to_owned(),
        ),
        // Case is ignored for letters beyond ASCII too.
        (search("\u{c4}", "ae"), "B\u{e4}r", "Baer".to_owned()),
    ];
    for (search, text, expected) in cases {
        assert_eq!(applied(&search, text), expected, "{search:?} on {text}");
    }
}

#[test]
fn an_invalid_expression_is_an_error_with_a_code() {
    let search = SearchReplace {
        regex: true,
        ..search("(unclosed", "")
    };
    let error = search.compile().expect_err("does not compile");
    assert!(matches!(error, RenameError::Regex { .. }), "{error:?}");
    assert_eq!(error.code(), "rename_regex_invalid");
}

#[test]
fn look_around_is_refused_not_ignored() {
    let search = SearchReplace {
        regex: true,
        ..search("a(?=b)", "")
    };
    assert!(search.compile().is_err());
}

#[test]
fn a_rule_reads_with_defaults_for_what_it_leaves_out() {
    let search: SearchReplace =
        serde_json::from_str(r#"{"find": "x", "regex": true}"#).expect("parses");
    assert_eq!(
        search,
        SearchReplace {
            regex: true,
            ..self::search("x", "")
        }
    );
}

#[test]
fn plain_text_takes_several_terms_separated_by_a_bar() {
    let cases = [
        // Each term by the replacement at its position, all in one pass.
        (
            search("\u{e4}|\u{f6}|\u{fc}", "ae|oe|ue"),
            "S\u{fc}\u{df}e K\u{e4}se",
            "Sue\u{df}e Kaese",
        ),
        (search("a|b", "b|a"), "abba", "baab"),
        // Without a bar in the replacement every term gets all of it.
        (search(" |_", "-"), "a b_c", "a-b-c"),
        // Missing replacements are empty; empty terms are skipped.
        (search("x|y|z", "1"), "xyz", "111"),
        (search("x|y|z", "1|2"), "xyz", "12"),
        (search("x||", "1"), "x|", "1|"),
        (search("|", "1"), "a|b", "a|b"),
    ];
    for (search, text, expected) in cases {
        assert_eq!(applied(&search, text), expected, "{search:?} on {text}");
    }
}

#[test]
fn substitution_replaces_the_whole_name_when_the_search_matches() {
    let subst = |find: &str, replace: &str, regex: bool| SearchReplace {
        regex,
        substitute: true,
        ..search(find, replace)
    };
    let cases = [
        (
            subst(r"(\d{4})-(\d\d)", "$2.$1", true),
            "report 2024-03 final",
            "03.2024",
        ),
        (subst(r"\d+", "x", true), "no digits", "no digits"),
        (subst("draft", "final", false), "my draft v2", "final"),
        (subst("a|b", "1|2", false), "xbxa", "2"),
    ];
    for (search, text, expected) in cases {
        assert_eq!(applied(&search, text), expected, "{search:?} on {text}");
    }
}
