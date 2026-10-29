//! The focus ring (RC-080-16) of the questions and the dialogs over the connection manager: the
//! host-key and certificate questions keep their own walk over their buttons (Tab and the
//! arrows, the button under the keyboard is the primary, Enter takes it) and have every button in
//! the ring as well; the MCP question has Deny as its one stop, the ring on it as it opens, and
//! Allow only for a click; the login-script editor and "Run command" walk every control.

use std::time::Duration;

use iced::keyboard::key::Named;
use rc_engine::{CommandClass, ConnectionId, McpAnswer, McpAsk, McpVerdict};
use rc_remote::ftp::{CertificateRejection, fingerprint};

use super::probe::{
    HOST, Screen, harness, host_key_question, kinds, press_every_stop, press_on, walk_round,
};
use crate::app::harness::headless_engine;
use crate::{
    app::{
        Message,
        cert_trust::{CertTrustMsg, Choice as CertChoice},
        connections::ConnMsg,
        harness::Harness,
        hostkeys::{Choice as KeyChoice, HostKeyMsg},
        login_scripts, mcp_confirm,
        mcp_confirm::McpConfirmMsg,
        ssh_exec::{self, ExecMsg},
    },
    theme::{self, focus::Kind},
};

const NAS: &[u8] = include_bytes!("../../../rc-remote/tests/fixtures/tls/nas.der");

/// The id `mcp_confirm` gives Deny, which has the ring as a question opens.
const DENY_ID: &str = "rcommander.mcp_confirm.deny";

#[tokio::test]
async fn the_host_key_question_walks_its_own_buttons_round() {
    let (_root, mut harness) = harness().await;
    host_key_question(&mut harness).await;
    let dialogs = &harness.app.host_keys;
    let question = &dialogs.shown().expect("the question").question;
    assert_eq!(question.host, HOST);
    assert_eq!(
        dialogs.buttons(),
        [
            KeyChoice::ConnectOnce,
            KeyChoice::TrustAndSave,
            KeyChoice::Cancel
        ]
    );
    assert_eq!(
        dialogs.focused(),
        Some(KeyChoice::Cancel),
        "on the safe button"
    );
    // Tab goes round every button once, from Cancel to the first; Shift+Tab and the arrows
    // go back and forth.
    let mut seen = Vec::new();
    for _ in 0..3 {
        harness.press("Tab").await;
        seen.push(harness.app.host_keys.focused().expect("a button"));
    }
    assert_eq!(
        seen,
        [
            KeyChoice::ConnectOnce,
            KeyChoice::TrustAndSave,
            KeyChoice::Cancel
        ]
    );
    harness.press("Shift+Tab").await;
    assert_eq!(
        harness.app.host_keys.focused(),
        Some(KeyChoice::TrustAndSave)
    );
    harness.press("ArrowRight").await;
    assert_eq!(harness.app.host_keys.focused(), Some(KeyChoice::Cancel));
    harness.press("ArrowLeft").await;
    assert_eq!(
        harness.app.host_keys.focused(),
        Some(KeyChoice::TrustAndSave)
    );
    // Every button is a stop of the ring too; Enter there answers with that button.
    {
        let layer = harness
            .app
            .host_key_layer(theme::tokens(false))
            .expect("the dialog");
        let mut screen = Screen::new(layer).await;
        let stops = walk_round(&mut screen, "host key");
        assert_eq!(kinds(&stops), [Some(Kind::Button); 3]);
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        let cancel = press_on(&mut screen, "Cancel", Named::Enter);
        assert!(matches!(
            cancel[..],
            [Message::HostKey(HostKeyMsg::Choose(_, KeyChoice::Cancel))]
        ));
    }
    // Enter takes the button the keyboard is on: Cancel, after one more step.
    harness.press("ArrowRight").await;
    harness.press("Enter").await;
    assert!(harness.app.host_keys.shown().is_none(), "answered");
}

#[tokio::test]
async fn the_certificate_question_walks_its_own_buttons_round() {
    let (_root, mut harness) = harness().await;
    let _refused = harness
        .app
        .engine()
        .certificate_refused(CertificateRejection {
            host: "nas.example".to_owned(),
            port: 990,
            fingerprint: fingerprint(NAS),
            reason: "invalid peer certificate: UnknownIssuer".to_owned(),
            system_verified: false,
            end_entity: NAS.to_vec(),
        })
        .await;
    harness.settle().await;
    let dialogs = &harness.app.cert_trust;
    assert_eq!(
        dialogs.buttons(),
        [
            CertChoice::AcceptOnce,
            CertChoice::TrustAndSave,
            CertChoice::Cancel
        ]
    );
    assert_eq!(
        dialogs.focused(),
        Some(CertChoice::Cancel),
        "on the safe button"
    );
    let mut seen = Vec::new();
    for _ in 0..3 {
        harness.press("Tab").await;
        seen.push(harness.app.cert_trust.focused().expect("a button"));
    }
    assert_eq!(
        seen,
        [
            CertChoice::AcceptOnce,
            CertChoice::TrustAndSave,
            CertChoice::Cancel
        ]
    );
    harness.press("Shift+Tab").await;
    assert_eq!(
        harness.app.cert_trust.focused(),
        Some(CertChoice::TrustAndSave)
    );
    harness.press("ArrowRight").await;
    assert_eq!(harness.app.cert_trust.focused(), Some(CertChoice::Cancel));
    {
        let layer = harness
            .app
            .cert_trust_layer(theme::tokens(false))
            .expect("the dialog");
        let mut screen = Screen::new(layer).await;
        let stops = walk_round(&mut screen, "certificate");
        assert_eq!(kinds(&stops), [Some(Kind::Button); 3]);
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        let accept = press_on(&mut screen, "Accept once", Named::Space);
        assert!(matches!(
            accept[..],
            [Message::CertTrust(CertTrustMsg::Choose(
                _,
                CertChoice::AcceptOnce
            ))]
        ));
    }
    harness.press("Enter").await;
    assert!(harness.app.cert_trust.shown().is_none(), "answered");
}

/// A call of the class `exec` asks in the window: Deny is the one stop and has the ring as the
/// question opens, so Enter and Space deny; Allow is no stop - only a click allows.
#[tokio::test]
async fn the_mcp_question_starts_on_deny_and_allows_only_by_a_click() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    engine.attach_window();
    let mut harness = Harness::new(engine);
    let engine = harness.app.engine().clone();
    let ask = McpAsk {
        tool: "terminal_open".to_owned(),
        class: CommandClass::Exec,
        arguments: Vec::new(),
        panel: None,
        cursor: false,
        session: None,
    };
    let call = tokio::spawn(async move { engine.ask_mcp(ask, Duration::from_secs(30)).await });
    while harness.app.engine().mcp_questions().is_empty() {
        tokio::task::yield_now().await;
    }
    harness.settle().await;
    let id = harness.app.mcp_questions.first().expect("a question").id;
    {
        let layer =
            mcp_confirm::layer(&harness.app.mcp_questions, theme::tokens(false)).expect("shown");
        let mut screen = Screen::new(layer).await;
        screen.focus(DENY_ID);
        assert_eq!(
            screen.at(),
            Some(0),
            "Deny has the ring as the question opens"
        );
        let stops = walk_round(&mut screen, "MCP question");
        assert_eq!(kinds(&stops), [Some(Kind::Button)], "Deny is the one stop");
        assert!(stops[0].text.contains("Deny"), "{stops:?}");
        assert_eq!(
            screen.loose_buttons(),
            2,
            "the head's X and Allow, which only a click presses"
        );
        for key in [Named::Enter, Named::Space] {
            let deny = press_on(&mut screen, "Deny", key);
            assert!(matches!(
                deny[..],
                [Message::McpConfirm(McpConfirmMsg::Answer(
                    _,
                    McpAnswer::Deny
                ))]
            ));
        }
    }
    harness
        .send(Message::McpConfirm(McpConfirmMsg::Answer(
            id,
            McpAnswer::Deny,
        )))
        .await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
}

#[tokio::test]
async fn the_login_script_editor_walks_its_list_form_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness
        .send(Message::Connections(ConnMsg::LoginScripts))
        .await;
    let layer = harness
        .app
        .login_scripts_layer(theme::tokens(false))
        .expect("the editor");
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "login scripts");
    assert_eq!(
        stops[0].kind,
        Some(Kind::Button),
        "New script: no script yet"
    );
    assert!(kinds(&stops).contains(&None), "the fields");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "login scripts");
    let new = press_on(&mut screen, "New script", Named::Enter);
    assert!(matches!(
        new[..],
        [Message::LoginScripts(login_scripts::Message::New)]
    ));
    let close = press_on(&mut screen, "Close", Named::Enter);
    assert!(matches!(close[..], [Message::CloseLoginScripts]));
}

#[tokio::test]
async fn run_command_walks_its_field_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness
        .send(Message::SshExec(ExecMsg::Open {
            connection: ConnectionId(7),
            name: "web".to_owned(),
        }))
        .await;
    harness
        .send(Message::SshExec(ExecMsg::Command("uptime".to_owned())))
        .await;
    let layer =
        ssh_exec::layer(harness.app.ssh_exec.as_ref(), theme::tokens(false)).expect("the dialog");
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "run command");
    assert_eq!(
        kinds(&stops),
        [None, Some(Kind::Button), Some(Kind::Button)],
        "the command, Run, Close"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let run = press_on(&mut screen, "Run", Named::Enter);
    assert!(matches!(run[..], [Message::SshExec(ExecMsg::Run)]));
    let close = press_on(&mut screen, "Close", Named::Space);
    assert!(matches!(close[..], [Message::SshExec(ExecMsg::Close)]));
}
