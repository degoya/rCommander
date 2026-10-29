use super::*;

#[test]
fn an_assignment_is_found_ahead() {
    assert_eq!(assignment(b"FOO=1 x"), Some((3, 1)));
    assert_eq!(assignment(b"x+=y"), Some((1, 2)));
    assert_eq!(assignment(b"arr[$i]=v"), Some((7, 1)));
    assert_eq!(assignment(b"_a1=\"q\""), Some((3, 1)));
    let none: [&[u8]; 8] = [
        b"1x=2",
        b"--opt=v",
        b"a b=c",
        b"arr[x y]=1",
        b"a",
        b"=x",
        b"a+b=1",
        b"",
    ];
    for text in none {
        assert_eq!(assignment(text), None, "{}", String::from_utf8_lossy(text));
    }
}

#[test]
fn a_function_definition_has_its_parentheses_ahead() {
    assert!(function_parens(b"() {"));
    assert!(function_parens(b" ( ) {"));
    assert!(!function_parens(b" (x)"));
    assert!(!function_parens(b"; x"));
    assert!(!function_parens(b""));
}

#[test]
fn an_fd_number_before_a_redirection() {
    assert_eq!(fd_prefix(b"2>&1"), Some(1));
    assert_eq!(fd_prefix(b"10<x"), Some(2));
    assert_eq!(fd_prefix(b"2 >x"), None);
    assert_eq!(fd_prefix(b"12"), None);
    assert_eq!(fd_prefix(b">x"), None);
}

#[test]
fn numbers_names_keywords_and_builtins() {
    for number in ["0", "42", "0.5"] {
        assert!(is_number(number.as_bytes()), "{number}");
    }
    for text in ["", ".", "1.", ".5", "1a", "-1"] {
        assert!(!is_number(text.as_bytes()), "{text}");
    }
    assert!(is_name(b"a_1"));
    assert!(!is_name(b"1a"));
    assert!(!is_name(b""));
    assert_eq!(keyword(b"if"), Some(Pos::Command));
    assert_eq!(keyword(b"case"), Some(Pos::CaseWord));
    assert_eq!(keyword(b"for"), Some(Pos::ForVar));
    assert_eq!(keyword(b"function"), Some(Pos::FuncName));
    assert_eq!(keyword(b"fi"), Some(Pos::Args));
    assert_eq!(keyword(b"echo"), None);
    for builtin in ["echo", "[", ".", "source", "trap"] {
        assert!(is_builtin(builtin.as_bytes()), "{builtin}");
    }
    assert!(!is_builtin(b"ls"));
    assert!(declares(b"local"));
    assert!(!declares(b"echo"));
}

#[test]
fn what_a_dollar_expands() {
    for byte in *b"x_1{(@?#" {
        assert!(expands(byte), "{}", byte as char);
    }
    for byte in *b" /\"'" {
        assert!(!expands(byte), "{}", byte as char);
    }
    assert!(ends_plain(b'$'));
    assert!(ends_plain(b';'));
    assert!(!ends_plain(b'#'));
    assert!(!is_meta(b'"'));
}
