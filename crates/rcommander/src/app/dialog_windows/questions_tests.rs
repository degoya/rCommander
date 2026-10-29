//! The small questions in windows of their own through `apply` (RC-080-23): each has a window
//! while `appearance.dialog_windows` is on - the top one only -, its keys reach it there and in
//! the locked main window and never the panels, Escape and the title bar's X answer it as on its
//! card, and the setting off keeps the cards. The MCP call's window opens on Deny. The question
//! of a large fetch is `drag_tests.rs`'s, the owners and the closing `question_window_tests.rs`'s,
//! the focus rings `field_walk/focus_ring_question_windows_tests.rs`'s.

use std::time::{Duration, Instant};

use iced::window;
use rc_db::Db;
use rc_engine::{Command, CommandClass, EngineEvent, McpAsk, McpVerdict, OfferedKey, PanelId};
use rc_remote::ftp::{CertificateRejection, fingerprint};
use tokio::task::JoinHandle;

use super::Question;
use crate::app::harness::headless_engine;
use crate::{
    app::{
        Message,
        archive_ops::{ArchiveDialog, PasswordPrompt, Typed},
        compare::{Confirm, SyncDialog},
        dialog_windows::{DialogWindow, DialogWindowMsg, DialogWindows, first_field},
        harness::Harness,
        mcp_confirm,
        queue_window::QueueMsg,
        settings::SettingsMessage,
    },
    theme,
};

const ED25519: &str = include_str!("../../../../rc-remote/tests/fixtures/known_hosts/ed25519.pub");
const NAS: &[u8] = include_bytes!("../../../../rc-remote/tests/fixtures/tls/nas.der");

/// The questions these tests ask; the large fetch is `drag_tests.rs`'s.
const ASKED: [Question; 8] = [
    Question::Mcp,
    Question::HostKey,
    Question::Certificate,
    Question::VaultOffer,
    Question::Quit,
    Question::Unlock,
    Question::ArchivePassword,
    Question::SyncConfirm,
];

/// The app beside a file `a.txt`, with a database, the window attached for the MCP questions,
/// `appearance.dialog_windows` as `own` and its main window `main`.
async fn app(own: bool) -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    let engine = headless_engine(root.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    engine.attach_window();
    let mut harness = Harness::new(engine);
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(own)).await;
    (root, harness, main)
}

/// Asks `question` as the engine or a person does; the MCP call that waits, for that one.
async fn ask(harness: &mut Harness, question: Question) -> Option<JoinHandle<McpVerdict>> {
    match question {
        Question::Mcp => {
            let engine = harness.app.engine().clone();
            let ask = McpAsk {
                tool: "terminal_open".to_owned(),
                class: CommandClass::Exec,
                arguments: Vec::new(),
                panel: None,
                cursor: false,
                session: None,
            };
            let call =
                tokio::spawn(async move { engine.ask_mcp(ask, Duration::from_secs(30)).await });
            while harness.app.engine().mcp_questions().is_empty() {
                tokio::task::yield_now().await;
            }
            harness.settle().await;
            return Some(call);
        }
        Question::HostKey => {
            let mut fields = ED25519.split_whitespace();
            let key =
                OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
                    .expect("a valid key");
            // Refused until someone answers; the question is what counts here.
            let _ = harness
                .app
                .engine()
                .check_host_key("files.example.org", 2222, key)
                .await;
            harness.settle().await;
        }
        Question::Certificate => {
            let _ = harness
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
        }
        Question::VaultOffer => {
            harness
                .send(Message::Engine(EngineEvent::VaultUnused))
                .await;
        }
        Question::Quit => {
            harness
                .send(Message::Engine(EngineEvent::Quit { unfinished: 2 }))
                .await;
        }
        Question::Unlock => {
            let refused = vec![Command::ListDrives { refresh: false }];
            harness
                .send(Message::VaultRefused(PanelId::Left, refused))
                .await;
        }
        Question::ArchivePassword => {
            harness.app.archive = Some(ArchiveDialog::Password(PasswordPrompt {
                job: 1,
                archive: "a.zip".to_owned(),
                wrong: false,
                typed: Typed::default(),
            }));
        }
        Question::LargeFetch => panic!("drag_tests.rs asks the large fetch"),
        Question::SyncConfirm => {
            let left = harness.app.pane(PanelId::Left).state.path.clone();
            let right = harness.app.pane(PanelId::Right).state.path.clone();
            let mut dialog = SyncDialog::new(left, right);
            dialog.confirm = Some(Confirm {
                summary: rc_engine::sync::SyncSummary::default(),
                deletes: false,
                permanent: false,
                ask_overwrite: true,
            });
            harness.app.compare.sync = Some(dialog);
        }
    }
    None
}

/// What `update` does after a message, with the system's part played: the windows to open and
/// close (`sync_dialog_windows`), and each window made ready and shown in the order it was asked
/// for (`DialogWindowMsg`).
async fn sync(harness: &mut Harness) {
    let _ = harness.app.sync_dialog_windows();
    let since = Instant::now();
    let books = &harness.app.dialog_windows;
    let made: Vec<window::Id> = harness
        .app
        .wanted_windows()
        .into_iter()
        .filter(|kind| books.visible(*kind).is_none())
        .filter_map(|kind| books.id_of(kind))
        .collect();
    for id in made {
        let reused = false;
        for message in [
            DialogWindowMsg::Ready { id, since, reused },
            DialogWindowMsg::Shown { id, since, reused },
        ] {
            harness.send(Message::DialogWindow(message)).await;
        }
    }
}

/// Opens the windows `sync_dialog_windows` would open now and gives the keyboard to `focused`'s,
/// else to the last one opened: its id.
async fn open_windows(harness: &mut Harness, focused: Option<DialogWindow>) -> window::Id {
    sync(harness).await;
    let books = &harness.app.dialog_windows;
    let id = match focused {
        Some(kind) => books.id_of(kind),
        None => harness
            .app
            .wanted_windows()
            .last()
            .and_then(|kind| books.id_of(*kind)),
    }
    .expect("an open window");
    harness
        .send(Message::Queue(QueueMsg::Focused(id, true)))
        .await;
    id
}

/// The question's window: the one wanted, last, and open.
fn question_id(harness: &Harness, question: Question) -> window::Id {
    let kind = DialogWindow::Question(question);
    assert_eq!(harness.app.wanted_windows().last(), Some(&kind));
    harness
        .app
        .dialog_windows
        .id_of(kind)
        .expect("the question's window")
}

/// Each question opens a window of its own with its title and its content, modal but for the
/// synchronization's confirmation; a panel's key does nothing there, Escape answers it as on its
/// card and its window closes next.
#[tokio::test]
async fn each_question_has_a_window_of_its_own_and_its_keys() {
    for question in ASKED {
        let (_root, mut harness, _main) = app(true).await;
        let _call = ask(&mut harness, question).await;
        assert_eq!(harness.app.small_question(), Some(question), "{question:?}");
        let id = open_windows(&mut harness, None).await;
        assert_eq!(id, question_id(&harness, question));
        assert_eq!(
            harness.app.main_locked(),
            question.modal(),
            "{question:?} locks the main window, or the compare window only"
        );
        let tokens = theme::tokens(false);
        assert_eq!(
            harness
                .app
                .dialog_layers(DialogWindow::Question(question), tokens)
                .len(),
            1,
            "{question:?} shows in its window"
        );
        assert!(!harness.app.window_title(id).is_empty(), "{question:?}");
        assert!(harness.app.question_layers(tokens).is_empty(), "no card");
        assert!(harness.app.dialog_open(), "{question:?} has the keys");

        harness.press("F7").await;
        assert!(
            harness.app.file_ops.dialog.is_none(),
            "{question:?}'s keys are no panel's"
        );
        assert_eq!(harness.app.small_question(), Some(question), "still asked");

        harness.press("Escape").await;
        assert_eq!(
            harness.app.small_question(),
            None,
            "Escape answers {question:?}"
        );
        let (_, closing) = harness
            .app
            .dialog_windows
            .difference(&harness.app.wanted_windows());
        assert!(closing.contains(&id), "{question:?}'s window closes next");
    }
}

/// The title bar's X answers each question as Escape does, and F10 in its window too; neither
/// quits.
#[tokio::test]
async fn the_x_and_f10_answer_each_question() {
    for question in ASKED {
        let (_root, mut harness, _main) = app(true).await;
        let _call = ask(&mut harness, question).await;
        let id = open_windows(&mut harness, None).await;
        harness.send(Message::CloseRequested(id)).await;
        assert_eq!(
            harness.app.small_question(),
            None,
            "the X answers {question:?}"
        );

        let (_root, mut harness, _main) = app(true).await;
        let _call = ask(&mut harness, question).await;
        let _ = open_windows(&mut harness, None).await;
        harness.press("F10").await;
        assert_eq!(
            harness.app.small_question(),
            None,
            "F10 answers {question:?}"
        );
        assert!(!harness.app.quit.leaving, "and quits nothing");
    }
}

/// With the setting off each question is the card it was: no window, the main window not
/// locked, Escape answers it.
#[tokio::test]
async fn off_the_questions_stay_cards() {
    for question in ASKED {
        let (_root, mut harness, _main) = app(false).await;
        let _call = ask(&mut harness, question).await;
        assert_eq!(harness.app.small_question(), None, "{question:?}");
        assert!(
            harness.app.wanted_windows().is_empty(),
            "{question:?} a card"
        );
        assert!(!harness.app.main_locked());
        harness.press("Escape").await;
        assert!(!harness.app.asks(question), "Escape answers {question:?}");
    }
}

/// A question that arrives while a modal dialog's window is open opens over it and takes the keys
/// that window gets - the dialog waits under it; answered, the dialog has them again.
#[tokio::test]
async fn a_question_over_a_modal_dialog_window_takes_its_keys() {
    let (_root, mut harness, _main) = app(true).await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let settings = open_windows(&mut harness, Some(DialogWindow::Settings)).await;
    let _ = ask(&mut harness, Question::HostKey).await;
    assert_eq!(
        harness.app.wanted_windows(),
        [
            DialogWindow::Settings,
            DialogWindow::Question(Question::HostKey)
        ]
    );
    sync(&mut harness).await;
    let id = question_id(&harness, Question::HostKey);
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((id, settings)),
        "owned by the settings' window, so never behind it"
    );
    // The keyboard is still in the settings' window: its keys are the question's.
    harness.press("Escape").await;
    assert!(
        harness.app.host_keys.shown().is_none(),
        "the question answered"
    );
    assert!(harness.app.settings.is_some(), "the settings stay open");
    harness.press("Escape").await;
    assert!(
        harness.app.settings.is_none(),
        "now the settings have the keys"
    );
}

/// One question has a window at a time, the top one: a quit question waits under the offer to
/// remove the vault and gets the window once the offer is answered; an MCP call takes it over
/// both.
#[tokio::test]
async fn the_top_question_has_the_window_and_the_next_follows() {
    let (_root, mut harness, _main) = app(true).await;
    let _ = ask(&mut harness, Question::Quit).await;
    let _ = ask(&mut harness, Question::VaultOffer).await;
    assert_eq!(harness.app.small_question(), Some(Question::VaultOffer));
    let offer = open_windows(&mut harness, None).await;
    let call = ask(&mut harness, Question::Mcp).await.expect("the call");
    assert_eq!(harness.app.small_question(), Some(Question::Mcp));
    let (opening, closing) = harness
        .app
        .dialog_windows
        .difference(&harness.app.wanted_windows());
    assert_eq!(opening, [DialogWindow::Question(Question::Mcp)]);
    assert_eq!(closing, [offer], "the offer makes way");

    let _ = open_windows(&mut harness, None).await;
    harness.press("Escape").await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
    assert_eq!(harness.app.small_question(), Some(Question::VaultOffer));
    harness.press("Escape").await;
    assert_eq!(harness.app.small_question(), Some(Question::Quit));
    harness.press("Escape").await;
    assert_eq!(harness.app.small_question(), None);
}

/// The MCP call's window opens on Deny, the safe answer (RC-010-13, RC-080-12): no key allows -
/// Enter and Space without a focused button, a letter, Tab - and Escape denies.
#[tokio::test]
async fn the_mcp_window_opens_on_deny_and_no_key_allows() {
    assert_eq!(
        first_field(DialogWindow::Question(Question::Mcp)),
        Some(mcp_confirm::DENY_ID)
    );
    let (_root, mut harness, _main) = app(true).await;
    let call = ask(&mut harness, Question::Mcp).await.expect("the call");
    let _ = open_windows(&mut harness, None).await;
    for key in ["Enter", "Space", "A", "Alt+A", "Tab"] {
        harness.press(key).await;
        assert_eq!(
            harness.app.small_question(),
            Some(Question::Mcp),
            "{key} answers nothing"
        );
    }
    assert!(!call.is_finished(), "the call still waits");
    harness.press("Escape").await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
}

/// The synchronization's confirmation locks only the compare window: with the main window's
/// keyboard the panels keep their keys, with the compare window's or its own it has them.
#[tokio::test]
async fn the_sync_confirmation_takes_only_the_compare_windows_keys() {
    let (_root, mut harness, main) = app(true).await;
    let _ = ask(&mut harness, Question::SyncConfirm).await;
    let compare = open_windows(&mut harness, Some(DialogWindow::Compare)).await;
    let id = question_id(&harness, Question::SyncConfirm);
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((id, compare))
    );
    assert!(!harness.app.main_locked());

    harness
        .send(Message::Queue(QueueMsg::Focused(main, true)))
        .await;
    assert_eq!(harness.app.question_with_keys(), None);
    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_some(), "a panel key");
    harness.press("Escape").await;

    harness
        .send(Message::Queue(QueueMsg::Focused(compare, true)))
        .await;
    assert_eq!(
        harness.app.question_with_keys(),
        Some(Question::SyncConfirm)
    );
    harness.press("Escape").await;
    let dialog = harness.app.compare.sync.as_ref().expect("the dialog stays");
    assert!(dialog.confirm.is_none(), "Escape went back");
}

/// Every question keeps a place of its own beside the dialogs', fits its first size and names the
/// control its window starts on: Deny, and the password fields.
#[test]
fn every_question_has_a_place_and_a_first_control() {
    for question in Question::ALL {
        let kind = DialogWindow::Question(question);
        assert!(DialogWindow::ALL.contains(&kind), "{question:?}");
        assert!(kind.is_question() && kind.owned());
        assert_eq!(kind.modal(), question != Question::SyncConfirm);
        let expected = matches!(
            question,
            Question::Mcp | Question::Unlock | Question::ArchivePassword
        );
        assert_eq!(first_field(kind).is_some(), expected, "{question:?}");
    }
}
