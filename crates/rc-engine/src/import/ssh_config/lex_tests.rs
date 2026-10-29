use super::{Directive, LineError, split_line};

fn directive(keyword: &str, args: &[&str]) -> Option<Directive> {
    Some(Directive {
        keyword: keyword.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    })
}

#[test]
fn blank_and_comment_lines_are_no_directive() {
    assert_eq!(split_line(""), Ok(None));
    assert_eq!(split_line("   \t "), Ok(None));
    assert_eq!(split_line("# Host commented"), Ok(None));
    assert_eq!(split_line("\t  # indented comment"), Ok(None));
}

#[test]
fn space_and_equals_separate_the_keyword_alike() {
    let expected = directive("Port", &["22"]);
    assert_eq!(split_line("Port 22"), Ok(expected));
    assert_eq!(split_line("Port=22"), Ok(directive("Port", &["22"])));
    assert_eq!(split_line("  Port = 22  "), Ok(directive("Port", &["22"])));
    assert_eq!(split_line("\tPort\t22"), Ok(directive("Port", &["22"])));
    assert_eq!(split_line("HOST a b"), Ok(directive("HOST", &["a", "b"])));
}

#[test]
fn quotes_group_and_escapes_follow_openssh() {
    assert_eq!(
        split_line(r#"IdentityFile "~/My Keys/id rsa""#),
        Ok(directive("IdentityFile", &["~/My Keys/id rsa"]))
    );
    assert_eq!(
        split_line("IdentityFile '~/a b'"),
        Ok(directive("IdentityFile", &["~/a b"]))
    );
    assert_eq!(
        split_line(r"IdentityFile ~/a\ b"),
        Ok(directive("IdentityFile", &["~/a b"]))
    );
    assert_eq!(
        split_line(r#"Host pre"fix mid"post"#),
        Ok(directive("Host", &["prefix midpost"]))
    );
    assert_eq!(
        split_line(r#"User "say \"hi\"""#),
        Ok(directive("User", &[r#"say "hi""#]))
    );
    assert_eq!(split_line(r#"User """#), Ok(directive("User", &[""])));
}

#[test]
fn a_windows_path_keeps_its_backslashes() {
    assert_eq!(
        split_line(r"IdentityFile C:\Users\me\.ssh\id_ed25519"),
        Ok(directive("IdentityFile", &[r"C:\Users\me\.ssh\id_ed25519"]))
    );
    assert_eq!(
        split_line(r#"IdentityFile "C:\Program Files\keys\id""#),
        Ok(directive("IdentityFile", &[r"C:\Program Files\keys\id"]))
    );
}

#[test]
fn an_unquoted_hash_at_an_argument_start_ends_the_line() {
    assert_eq!(
        split_line("Host a b # the rest is a comment"),
        Ok(directive("Host", &["a", "b"]))
    );
    assert_eq!(split_line("Host a#b"), Ok(directive("Host", &["a#b"])));
    assert_eq!(split_line(r##"Host "#a""##), Ok(directive("Host", &["#a"])));
}

#[test]
fn malformed_lines_are_errors() {
    assert_eq!(
        split_line(r#"IdentityFile "~/open"#),
        Err(LineError::UnterminatedQuote)
    );
    assert_eq!(split_line("User 'x"), Err(LineError::UnterminatedQuote));
    assert_eq!(split_line("= value"), Err(LineError::MissingKeyword));
}

#[test]
fn a_keyword_without_arguments_has_none() {
    assert_eq!(split_line("Host"), Ok(directive("Host", &[])));
    assert_eq!(split_line("Host ="), Ok(directive("Host", &[])));
}
