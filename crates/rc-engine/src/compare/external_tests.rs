use std::path::Path;

use super::*;

fn args(template: &str) -> Vec<String> {
    arguments(
        template,
        Path::new("/home/me/left file.txt"),
        Path::new("/tmp/right.txt"),
    )
    .expect("reads")
    .into_iter()
    .map(|argument| argument.to_string_lossy().into_owned())
    .collect()
}

#[test]
fn the_default_hands_both_files_as_one_argument_each() {
    let both = ["/home/me/left file.txt", "/tmp/right.txt"];
    assert_eq!(args(DEFAULT_ARGS), both);
    assert_eq!(args(""), both, "empty is the default");
    assert_eq!(args("   "), both);
}

#[test]
fn quotes_keep_spaces_and_placeholders_stand_anywhere() {
    assert_eq!(
        args(r#"/e /x "/dl" "Left side" %2 --left=%1 100%%"#),
        [
            "/e",
            "/x",
            "/dl",
            "Left side",
            "/tmp/right.txt",
            "--left=/home/me/left file.txt",
            "100%",
        ]
    );
    assert_eq!(
        args(r#""" %1"#),
        ["", "/home/me/left file.txt"],
        "an empty argument"
    );
    assert_eq!(args("%3 %"), ["%3", "%"], "only %1 and %2 are files");
}

#[test]
fn an_open_quote_does_not_read() {
    assert_eq!(check(r#""%1" "%2"#), Err(ArgsError::OpenQuote));
    assert_eq!(check(DEFAULT_ARGS), Ok(()));
}
