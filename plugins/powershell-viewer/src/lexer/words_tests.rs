use super::*;

#[test]
fn numbers_take_hex_exponents_suffixes_and_multipliers() {
    for number in [
        "0", "42", "1.5", ".5", "1e3", "1.5E-3", "0x1F", "0XFFl", "10kb", "1.5GB", "3MB", "42l",
        "2ul", "7d", "1n", "5TB", "8pb",
    ] {
        assert!(is_number(number.as_bytes()), "{number}");
    }
    for word in ["7zip", "1st", "0x", "1e", "1.2.3", ".", "kb", "10kbx", "-1"] {
        assert!(!is_number(word.as_bytes()), "{word}");
    }
    // A range ends the number before its `..`.
    assert_eq!(number_end(b"1..10", 0), Some(1));
    assert_eq!(number_end(b"(5)", 1), Some(2));
}

#[test]
fn variables_take_scopes_drives_braces_and_the_special_ones() {
    let end = |text: &str| variable_end(text.as_bytes(), 0);
    assert_eq!(end("$x"), Some(2));
    assert_eq!(end("$x.Name"), Some(2));
    assert_eq!(end("$_.Length"), Some(2));
    assert_eq!(end("$env:PATH;"), Some(9));
    assert_eq!(end("$script:count+1"), Some(13));
    assert_eq!(end("$x::y"), Some(2));
    assert_eq!(end("$x: y"), Some(2));
    assert_eq!(end("${a b}c"), Some(6));
    assert_eq!(end("${open"), None);
    assert_eq!(end("$$ $? $^"), Some(2));
    assert_eq!(end("$\u{e4}x"), Some(4));
    assert_eq!(end("$ x"), None);
    assert_eq!(end("$"), None);
    assert_eq!(end("$("), None);
    assert_eq!(variable_class(b"$true"), TokenClass::Literal);
    assert_eq!(variable_class(b"$NULL"), TokenClass::Literal);
    assert_eq!(variable_class(b"$False"), TokenClass::Literal);
    assert_eq!(variable_class(b"$trueish"), TokenClass::Key);
}

#[test]
fn redirections_and_their_streams() {
    let end = |text: &str| redirect_end(text.as_bytes(), 0);
    assert_eq!(end("> out"), Some(1));
    assert_eq!(end(">> out"), Some(2));
    assert_eq!(end("2>&1"), Some(4));
    assert_eq!(end("2>$null"), Some(2));
    assert_eq!(end("*>&1"), Some(4));
    assert_eq!(end("3>>log"), Some(3));
    assert_eq!(end("< in"), Some(1));
    assert_eq!(end("7>x"), None);
    assert_eq!(end("2"), None);
}

#[test]
fn keywords_in_any_case_and_cmdlet_names() {
    assert!(is_keyword(b"ForEach"));
    assert!(is_keyword(b"ELSEIF"));
    assert!(!is_keyword(b"foreach-object"));
    assert!(!is_keyword(b"in"));
    for name in [
        "Get-ChildItem",
        "ConvertTo-Json",
        "Get-AzVM",
        "Out-Null",
        "set-item",
    ] {
        assert!(is_verb_noun(name.as_bytes()), "{name}");
    }
    for word in [
        "Get",
        "Get-",
        "-Item",
        "Get-Item-X",
        "C:\\x-y",
        "Get-9",
        "x.y-z",
    ] {
        assert!(!is_verb_noun(word.as_bytes()), "{word}");
    }
}

#[test]
fn a_word_ends_at_the_syntax_and_takes_whole_characters() {
    assert_eq!(word_end(b"C:\\temp\\x.ps1 -Force", 0, true), 13);
    assert_eq!(word_end(b"a=b c", 0, true), 3);
    assert_eq!(word_end(b"a=b c", 0, false), 1);
    assert_eq!(word_end(b"if(", 0, false), 2);
    assert_eq!(word_end("\u{e4}\u{f6}|".as_bytes(), 0, true), 4);
    assert_eq!(word_end(b"(", 0, true), 1);
}
