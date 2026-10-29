//! The terminal tools end to end over an in-process pipe: the confirmation code of
//! `terminal_send_input`, and a credential on the screen leaving masked.

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

async fn context() -> (McpContext, TempDir) {
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let built = Fixture::new().with_codes().build().await;
    (built.context, built.root)
}

fn code_of(asked: &Value) -> String {
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    asked["confirmation"].as_str().expect("code").to_owned()
}

#[tokio::test]
async fn send_input_wants_a_code_for_this_tool_and_this_terminal_once() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let input = |terminal: u64, code: Option<&str>| {
        let mut arguments = json!({ "terminal": terminal, "text": "ls\r" });
        if let Some(code) = code {
            arguments["confirmation"] = json!(code);
        }
        arguments
    };

    let asked = call(&client, "terminal_send_input", input(1, None)).await;
    assert_eq!(asked["class"], "exec");
    let code = code_of(&asked);
    let elsewhere = call(&client, "terminal_send_input", input(2, Some(&code))).await;
    code_of(&elsewhere);

    let code = code_of(&call(&client, "terminal_send_input", input(1, None)).await);
    let other_tool = call(
        &client,
        "terminal_close",
        json!({ "terminal": 1, "confirmation": code }),
    )
    .await;
    code_of(&other_tool);

    let code = code_of(&call(&client, "terminal_send_input", input(1, None)).await);
    let ran = call(&client, "terminal_send_input", input(1, Some(&code))).await;
    assert_eq!(
        ran["code"], "terminal_not_found",
        "the code let the tool run: {ran}"
    );
    let again = call(&client, "terminal_send_input", input(1, Some(&code))).await;
    code_of(&again);
}

#[tokio::test]
async fn reading_tools_need_no_code() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let listed = call(&client, "terminal_list", json!({})).await;
    assert_eq!(listed["kind"], "terminals", "{listed}");
    assert!(
        listed["shells"]
            .as_array()
            .is_some_and(|shells| !shells.is_empty())
    );
    let missing = call(&client, "terminal_read_screen", json!({ "terminal": 999 })).await;
    assert_eq!(missing["code"], "terminal_not_found", "{missing}");
    let not_shown = call(&client, "terminal_show", json!({ "terminal": 999 })).await;
    assert_eq!(not_shown["code"], "terminal_not_found", "{not_shown}");
    let files = call(&client, "show_files", json!({ "panel": "right" })).await;
    assert_eq!(files["panel"], "right", "{files}");
    assert_eq!(files["view"], "files", "{files}");
}

/// The code is bound to the distribution; a distribution with another shell is refused; without
/// WSL (every platform but Windows) a WSL shell is `wsl_unavailable`.
#[tokio::test]
async fn a_wsl_shell_is_asked_for_by_distribution() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let wsl = |distro: &str, code: Option<&str>| {
        let mut arguments = json!({ "shell": "wsl", "distro": distro });
        if let Some(code) = code {
            arguments["confirmation"] = json!(code);
        }
        arguments
    };
    let code = code_of(&call(&client, "terminal_open", wsl("Ubuntu", None)).await);
    code_of(&call(&client, "terminal_open", wsl("Debian", Some(&code))).await);

    let code = code_of(
        &call(
            &client,
            "terminal_open",
            json!({ "shell": "cmd", "distro": "Ubuntu" }),
        )
        .await,
    );
    let refused = call(
        &client,
        "terminal_open",
        json!({ "shell": "cmd", "distro": "Ubuntu", "confirmation": code }),
    )
    .await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");

    if cfg!(not(windows)) {
        let code = code_of(&call(&client, "terminal_open", wsl("Ubuntu", None)).await);
        let opened = call(&client, "terminal_open", wsl("Ubuntu", Some(&code))).await;
        assert_eq!(opened["code"], "wsl_unavailable", "{opened}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_credential_on_the_screen_is_masked() {
    use std::time::Duration;

    use rc_engine::{Command, Outcome};

    const CANARY: &str = "canary-4711";
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let asked = call(&client, "terminal_open", json!({ "shell": "sh" })).await;
    assert_eq!(asked["class"], "exec", "starting a shell is exec: {asked}");
    let code = code_of(&asked);
    let other_shell = call(
        &client,
        "terminal_open",
        json!({ "shell": "bash", "confirmation": code }),
    )
    .await;
    code_of(&other_shell);
    let code = code_of(&call(&client, "terminal_open", json!({ "shell": "sh" })).await);
    let opened = call(
        &client,
        "terminal_open",
        json!({ "shell": "sh", "confirmation": code }),
    )
    .await;
    assert_eq!(opened["state"]["status"], "running", "{opened}");
    let terminal = rc_engine::TermId(opened["terminal"].as_u64().expect("id"));
    let engine = context.engine();
    // The canary is put together by the shell, so only the output line carries it.
    engine
        .dispatch(Command::SendInput {
            terminal,
            text: "printf 'password=%s%s\\n' cana ry-4711\r".into(),
        })
        .await
        .expect("input");
    let mut shown = false;
    for _ in 0..1000 {
        if let Ok(Outcome::Screen { screen, .. }) = engine
            .dispatch(Command::ReadScreen {
                terminal,
                scrollback: 0,
            })
            .await
            && screen.lines.iter().any(|line| line.contains(CANARY))
        {
            shown = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(shown, "the shell printed the canary");

    let screen = call(
        &client,
        "terminal_read_screen",
        json!({ "terminal": terminal.0 }),
    )
    .await;
    let rendered = screen.to_string();
    assert!(!rendered.contains(CANARY), "{rendered}");
    assert!(rendered.contains("password=[redacted]"), "{rendered}");

    let code = code_of(&call(&client, "terminal_close", json!({ "terminal": terminal.0 })).await);
    let closed = call(
        &client,
        "terminal_close",
        json!({ "terminal": terminal.0, "confirmation": code }),
    )
    .await;
    assert_eq!(closed["state"]["status"], "exited", "{closed}");
}
