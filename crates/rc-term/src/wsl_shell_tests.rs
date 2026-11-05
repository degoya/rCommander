use super::{query_args, syntax_of_passwd};
use crate::ShellSyntax;

#[test]
fn the_login_shell_of_a_passwd_line_gives_the_syntax() {
    let line = |shell: &str| format!("ada:x:1000:1000:Ada,,,:/home/ada:{shell}\n");
    assert_eq!(syntax_of_passwd(&line("/bin/tcsh")), ShellSyntax::Csh);
    assert_eq!(syntax_of_passwd(&line("/usr/bin/csh")), ShellSyntax::Csh);
    assert_eq!(syntax_of_passwd(&line("/bin/bash")), ShellSyntax::Posix);
    assert_eq!(syntax_of_passwd(&line("/usr/bin/zsh")), ShellSyntax::Posix);
    assert_eq!(syntax_of_passwd(&line("/usr/bin/fish")), ShellSyntax::Posix);
    // No answer worth the name: POSIX, as before.
    assert_eq!(syntax_of_passwd(""), ShellSyntax::Posix);
    assert_eq!(syntax_of_passwd("garbage"), ShellSyntax::Posix);
}

#[test]
fn the_question_is_a_fixed_line_to_a_plausible_distribution() {
    let args = query_args(Some("Ubuntu")).expect("asked");
    assert_eq!(
        args,
        [
            "-d",
            "Ubuntu",
            "-e",
            "sh",
            "-c",
            r#"getent passwd "$(id -un)""#
        ]
    );
    assert_eq!(query_args(None).expect("asked")[..2], ["-e", "sh"]);
    assert_eq!(query_args(Some("--exec")), None);
    assert_eq!(query_args(Some("a b")), None);
}

#[cfg(not(windows))]
#[tokio::test]
async fn outside_windows_nothing_is_asked() {
    let syntax = super::wsl_login_syntax(std::path::Path::new("/nowhere/wsl.exe"), None).await;
    assert_eq!(syntax, ShellSyntax::Posix);
}
