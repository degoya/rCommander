use std::path::Path;

use base64::Engine as _;

use super::{
    BASH_SNIPPET, InheritedEnv, Integration, PWSH_SNIPPET, ZSH_SNIPPET, encoded_command, zsh_dotdir,
};
use crate::LocalCommand;

fn var<'a>(integration: &'a Integration, name: &str) -> Option<&'a str> {
    integration
        .env
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn powershell_gets_its_snippet_encoded_on_the_command_line() {
    let integration = Integration::local("pwsh", &InheritedEnv::default(), None).expect("pwsh");
    assert_eq!(integration.args[..2], ["-NoExit", "-EncodedCommand"]);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&integration.args[2])
        .expect("base64");
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(String::from_utf16(&units).expect("utf-16"), PWSH_SNIPPET);
    assert_eq!(
        encoded_command("ab"),
        "YQBiAA==",
        "UTF-16LE, standard base64"
    );
    assert!(Integration::local("powershell", &InheritedEnv::default(), None).is_some());
}

#[test]
fn bash_evaluates_its_snippet_before_the_users_prompt_command() {
    let plain = Integration::local("bash", &InheritedEnv::default(), None).expect("bash");
    assert!(plain.args.is_empty());
    assert_eq!(
        var(&plain, "PROMPT_COMMAND"),
        Some("eval \"$RC_SHELL_INTEGRATION\"")
    );
    assert_eq!(
        var(&plain, "RC_SHELL_INTEGRATION"),
        Some(BASH_SNIPPET.replace("\r\n", "\n").as_str())
    );
    let inherited = InheritedEnv {
        prompt_command: Some("history -a".to_owned()),
        ..InheritedEnv::default()
    };
    let kept = Integration::local("bash", &inherited, None).expect("bash");
    assert_eq!(
        var(&kept, "PROMPT_COMMAND"),
        Some("eval \"$RC_SHELL_INTEGRATION\";history -a")
    );
}

#[test]
fn wsl_hands_bashs_variables_through_wslenv() {
    let fresh = Integration::wsl(&InheritedEnv::default());
    assert_eq!(
        var(&fresh, "WSLENV"),
        Some("RC_SHELL_INTEGRATION/u:PROMPT_COMMAND/u")
    );
    let inherited = InheritedEnv {
        wslenv: Some("USERPROFILE/p".to_owned()),
        ..InheritedEnv::default()
    };
    let kept = Integration::wsl(&inherited);
    assert_eq!(
        var(&kept, "WSLENV"),
        Some("USERPROFILE/p:RC_SHELL_INTEGRATION/u:PROMPT_COMMAND/u")
    );
}

#[test]
fn cmd_wraps_the_users_prompt_in_the_marks() {
    let fresh = Integration::local("cmd", &InheritedEnv::default(), None).expect("cmd");
    assert_eq!(
        var(&fresh, "PROMPT"),
        Some(r"$E]9;9;$P$E\$E]133;A$E\$P$G$E]133;B$E\")
    );
    let inherited = InheritedEnv {
        prompt: Some("$T $P$G".to_owned()),
        ..InheritedEnv::default()
    };
    let own = Integration::local("cmd", &inherited, None).expect("cmd");
    assert!(var(&own, "PROMPT").is_some_and(|prompt| prompt.contains("133;A$E\\$T $P$G$E]133;B")));
}

#[test]
fn zsh_needs_its_folder_and_keeps_the_users_zdotdir() {
    let inherited = InheritedEnv {
        zdotdir: Some("/home/x/.config/zsh".to_owned()),
        ..InheritedEnv::default()
    };
    assert_eq!(Integration::local("zsh", &inherited, None), None);
    let zsh = Integration::local("zsh", &inherited, Some(Path::new("/data/zsh"))).expect("zsh");
    assert_eq!(var(&zsh, "ZDOTDIR"), Some("/data/zsh"));
    assert_eq!(var(&zsh, "RC_USER_ZDOTDIR"), Some("/home/x/.config/zsh"));
}

#[test]
fn shells_without_integration_get_none() {
    for shell in ["sh", "fish", "nu", "wsl"] {
        assert_eq!(
            Integration::local(shell, &InheritedEnv::default(), None),
            None,
            "{shell}"
        );
    }
}

#[test]
fn the_integration_is_added_to_the_command() {
    let mut command = LocalCommand {
        program: "pwsh.exe".into(),
        args: vec!["-NoLogo".to_owned()],
        cwd: None,
        env: vec![("A".to_owned(), "1".to_owned())],
        end_programs_on_close: false,
    };
    let integration = Integration {
        args: vec!["-NoExit".to_owned()],
        env: vec![("B".to_owned(), "2".to_owned())],
    };
    integration.apply(&mut command);
    assert_eq!(command.args, ["-NoLogo", "-NoExit"]);
    assert_eq!(command.env.len(), 2);
    assert!(
        !command.end_programs_on_close,
        "the start's own choice stays"
    );
}

#[test]
fn zshs_folder_is_written_once_and_private() {
    let base = tempfile::tempdir().expect("temp dir");
    let dir = zsh_dotdir(base.path()).expect("written");
    let file = dir.join(".zshenv");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        ZSH_SNIPPET.replace("\r\n", "\n")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    assert_eq!(zsh_dotdir(base.path()).expect("again"), dir);
}

#[test]
fn the_snippets_report_folder_and_prompt() {
    for snippet in [PWSH_SNIPPET, BASH_SNIPPET, ZSH_SNIPPET] {
        assert!(snippet.contains("133;A"), "a prompt mark");
        assert!(
            snippet.contains("]7;") || snippet.contains("]9;9;"),
            "a folder"
        );
    }
}
