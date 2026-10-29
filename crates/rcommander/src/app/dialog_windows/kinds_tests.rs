//! The standalone dialogs in windows of their own through `apply` (RC-080-23): each opens a
//! modal window while `appearance.dialog_windows` is on, its keys reach it there and never the
//! panels, Escape and the title bar's X close it, and the setting off keeps it a card. With the
//! cards, the unlock prompt a connection asked for alone draws over a modal dialog's window and
//! has its keys; with windows it is a question of its own (`questions_tests.rs`). The
//! windows themselves need a display; their books are `book_tests.rs`, the fields that take the
//! keyboard `field_walk/focus_ring_windows_tests.rs`.

use iced::window;
use rc_engine::{Chord, Command, ConnectionId, EngineEvent, PanelId, settings::DIALOG_WINDOWS_KEY};

use super::first_field;
use crate::{
    app::{
        Effect, Message,
        database_dialog::DatabaseMsg,
        dialog_windows::{DialogWindow, DialogWindows},
        drive_bar::{DriveMessage, actions::DriveAction},
        harness::Harness,
        queue_window::QueueMsg,
        settings::SettingsMessage,
        ssh_exec::ExecMsg,
        tabs::editor::HotlistEditor,
    },
    theme,
};

/// The app beside a file `a.txt`, under the cursor, with `appearance.dialog_windows` as `own`
/// and its main window `main`.
async fn app(own: bool) -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(own)).await;
    harness.press("ArrowDown").await;
    (root, harness, main)
}

/// The standalone dialogs, each with the way it opens.
const STANDALONE: [DialogWindow; 8] = [
    DialogWindow::Database,
    DialogWindow::Properties,
    DialogWindow::QuickConnect,
    DialogWindow::Network,
    DialogWindow::Archive,
    DialogWindow::ChecksumForm,
    DialogWindow::Hotlist,
    DialogWindow::SshExec,
];

/// Opens the dialog of `kind` as a person or a menu does.
async fn open(harness: &mut Harness, kind: DialogWindow) {
    match kind {
        DialogWindow::Database => {
            harness.send(Message::Database(DatabaseMsg::Open)).await;
        }
        DialogWindow::Properties => harness.press("Alt+Enter").await,
        DialogWindow::QuickConnect => harness.press("Ctrl+N").await,
        DialogWindow::Network => {
            let map = DriveMessage::Run(PanelId::Left, DriveAction::MapNetwork, None);
            harness.send(Message::Drive(map)).await;
        }
        DialogWindow::Archive => harness.press("Alt+F5").await,
        DialogWindow::ChecksumForm => {
            let create = EngineEvent::ChecksumDialog {
                panel: PanelId::Left,
                dialog: rc_engine::ChecksumDialog::Create,
            };
            harness.send(Message::Engine(create)).await;
        }
        DialogWindow::Hotlist => harness.app.tabs.editor = Some(HotlistEditor::default()),
        DialogWindow::SshExec => {
            let open = ExecMsg::Open {
                connection: ConnectionId(7),
                name: "web".to_owned(),
            };
            harness.send(Message::SshExec(open)).await;
        }
        other => panic!("{other:?} is no standalone dialog"),
    }
}

/// Whether the dialog of `kind` is open.
fn is_open(harness: &Harness, kind: DialogWindow) -> bool {
    let app = &harness.app;
    match kind {
        DialogWindow::Database => app.database.is_some(),
        DialogWindow::Properties => app.properties.is_some(),
        DialogWindow::QuickConnect => app.quick.is_some(),
        DialogWindow::Network => app.drive_bar.dialog.is_some(),
        DialogWindow::Archive => app.archive.is_some(),
        DialogWindow::ChecksumForm => app.checksum.is_some(),
        DialogWindow::Hotlist => app.tabs.editor.is_some(),
        DialogWindow::SshExec => app.ssh_exec.is_some(),
        other => panic!("{other:?} is no standalone dialog"),
    }
}

/// The window `sync_dialog_windows` would open for `kind`, booked, with the keyboard: its id.
async fn window_of(harness: &mut Harness, main: window::Id, kind: DialogWindow) -> window::Id {
    let wanted = harness.app.wanted_windows();
    assert_eq!(wanted, [kind], "a window for {kind:?} alone");
    let books = &mut harness.app.dialog_windows;
    let (opening, _) = books.difference(&wanted);
    assert_eq!(opening, [kind]);
    let id = window::Id::unique();
    let geometry = books.geometry(kind, harness.app.window.geometry());
    books.opened(kind, id, geometry);
    for (window, focused) in [(main, false), (id, true)] {
        harness
            .send(Message::Queue(QueueMsg::Focused(window, focused)))
            .await;
    }
    id
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

/// Each standalone dialog opens a modal window of its own, locking the main window; Tab walks
/// its fields there, a panel's key does nothing, Escape closes it and its window follows.
#[tokio::test]
async fn each_standalone_dialog_has_a_modal_window_its_keys_reach() {
    for kind in STANDALONE {
        let (_root, mut harness, main) = app(true).await;
        open(&mut harness, kind).await;
        assert!(is_open(&harness, kind), "{kind:?} opens");
        let id = window_of(&mut harness, main, kind).await;
        assert!(kind.modal(), "{kind:?} is modal");
        assert!(harness.app.main_locked(), "{kind:?} locks the main window");
        assert!(harness.app.dialog_open(), "{kind:?} has the keys");

        let effects = harness.app.apply(Message::Key(chord("Tab")));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::WalkFields { back: false, .. })),
            "Tab walks {kind:?}'s fields"
        );
        harness.press("F7").await;
        assert!(
            harness.app.file_ops.dialog.is_none(),
            "{kind:?}'s keys are no panel's"
        );
        assert!(is_open(&harness, kind), "{kind:?} stays open");

        harness.press("Escape").await;
        assert!(!is_open(&harness, kind), "Escape closes {kind:?}");
        let (_, closing) = harness
            .app
            .dialog_windows
            .difference(&harness.app.wanted_windows());
        assert_eq!(closing, [id], "{kind:?}'s window closes next");
    }
}

/// The title bar's X and Alt+F4 close each standalone dialog as Escape does, and so does F10
/// in its window (`keys.f10`, RC-080-17); none of them quits.
#[tokio::test]
async fn the_x_and_f10_close_each_standalone_dialog() {
    for kind in STANDALONE {
        let (_root, mut harness, main) = app(true).await;
        open(&mut harness, kind).await;
        let id = window_of(&mut harness, main, kind).await;
        harness.send(Message::CloseRequested(id)).await;
        assert!(!is_open(&harness, kind), "the X closes {kind:?}");
        assert!(harness.app.dialog_windows.closed(id).is_some());

        open(&mut harness, kind).await;
        let _ = window_of(&mut harness, main, kind).await;
        harness.press("F10").await;
        assert!(!is_open(&harness, kind), "F10 closes {kind:?}");
        assert!(!harness.app.quit.leaving, "and quits nothing");
    }
}

/// With the setting off every standalone dialog lies over the main window as a card and has the
/// keys there, as before; the main window is not locked.
#[tokio::test]
async fn off_the_standalone_dialogs_stay_cards() {
    for kind in STANDALONE {
        let (_root, mut harness, _main) = app(false).await;
        open(&mut harness, kind).await;
        assert!(harness.app.wanted_windows().is_empty(), "{kind:?} a card");
        assert!(!harness.app.main_locked());
        harness.press("Escape").await;
        assert!(!is_open(&harness, kind), "Escape closes {kind:?}");
    }
}

/// The setting switches an open dialog between its window and its card.
#[tokio::test]
async fn the_setting_follows_while_a_dialog_is_open() {
    let (_root, mut harness, _main) = app(true).await;
    harness.press("Ctrl+N").await;
    assert_eq!(harness.app.wanted_windows(), [DialogWindow::QuickConnect]);
    let changed = EngineEvent::SettingChanged {
        key: DIALOG_WINDOWS_KEY.to_owned(),
    };
    assert!(
        !harness.app.apply(Message::Engine(changed)).is_empty(),
        "the setting is read again"
    );
    harness.send(Message::DialogWindows(false)).await;
    assert!(harness.app.wanted_windows().is_empty(), "a card now");
    assert!(harness.app.quick.is_some(), "the dialog stays open");
}

/// The archive's password is a small question with a window of its own (`questions.rs`), not the
/// archive dialog's; packing and unpacking have that window.
#[tokio::test]
async fn the_archive_password_is_a_question_window() {
    use crate::app::archive_ops::{self, ArchiveDialog, PasswordPrompt, Typed};
    let (_root, mut harness, _main) = app(true).await;
    harness.press("Alt+F5").await;
    let dialog = harness.app.archive.as_ref().expect("the pack dialog");
    assert!(matches!(dialog, ArchiveDialog::Pack(_)));
    assert!(archive_ops::windowed(dialog));
    assert_eq!(harness.app.wanted_windows(), [DialogWindow::Archive]);

    harness.app.archive = Some(ArchiveDialog::Password(PasswordPrompt {
        job: 1,
        archive: "a.zip".to_owned(),
        wrong: false,
        typed: Typed::default(),
    }));
    assert_eq!(
        harness.app.wanted_windows(),
        [DialogWindow::Question(
            super::super::questions::Question::ArchivePassword
        )],
        "a question's window"
    );
    assert!(harness.app.main_locked());
}

/// The fields the windows give the keyboard at their opening, one per dialog that has a field
/// to type into first; the properties and the hotlist start on their list and boxes.
#[test]
fn the_standalone_dialogs_name_their_first_field() {
    for kind in STANDALONE {
        let expected = !matches!(kind, DialogWindow::Properties | DialogWindow::Hotlist);
        assert_eq!(first_field(kind).is_some(), expected, "{kind:?}");
    }
}

/// While a modal dialog's window is open, the main window's own layers take no input; without
/// one they do.
#[tokio::test]
async fn a_modal_window_keeps_the_input_from_the_main_window() {
    let (_root, mut harness, main) = app(true).await;
    let key = Message::Key(chord("Escape"));
    assert!(harness.app.main_reaches(&key));
    harness.press("Ctrl+N").await;
    let _ = window_of(&mut harness, main, DialogWindow::QuickConnect).await;
    assert!(!harness.app.main_reaches(&key), "input stays the dialog's");
    assert!(
        harness.app.main_reaches(&Message::EngineLagged),
        "the rest goes on"
    );
}

/// With the dialogs as cards, the unlock prompt a connection asked for alone, while the settings'
/// modal window is open - they always have one -, draws in that window - the locked main window
/// would hide it - and has the keys before the settings: Escape drops the prompt and leaves the
/// settings open.
#[tokio::test]
async fn the_unlock_prompt_draws_over_a_modal_window_and_has_its_keys() {
    let (_root, mut harness, _main) = app(false).await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let refused = vec![Command::ListDrives { refresh: false }];
    harness
        .send(Message::VaultRefused(PanelId::Left, refused))
        .await;
    let manager = harness.app.connections.as_ref().expect("the prompt");
    assert!(manager.prompt_only && manager.prompt.is_some());
    assert_eq!(harness.app.wanted_windows(), [DialogWindow::Settings]);
    let home = harness.app.manager_home();
    assert_eq!(home, Some(DialogWindow::Settings));
    let tokens = theme::tokens(false);
    assert_eq!(
        harness.app.manager_layers(home, tokens).len(),
        1,
        "the prompt over the settings"
    );

    harness.press("Escape").await;
    assert!(harness.app.connections.is_none(), "the prompt is gone");
    assert!(harness.app.settings.is_some(), "the settings stay open");
}
