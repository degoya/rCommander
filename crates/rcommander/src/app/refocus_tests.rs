//! The return from another program (RC-100-69): told apart from a switch between own windows by
//! the time no own window had the keyboard, and the panels it reads again - by backend, view,
//! drive and the two switches - through `apply`.

use std::time::{Duration, Instant};

use iced::window;
use rc_engine::{
    Command, CursorTarget, Drive, DriveGroup, DriveKind, DriveState, PanelId, VfsPath,
};
use rc_vfs::BackendId;

use super::{AWAY, Refocus, Shown, Switches, on_network, refreshes};
use crate::app::{Message, harness::Harness, queue_window::QueueMsg};

/// `start` and `millis` later.
fn after(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn the_first_focus_after_the_start_is_no_return() {
    let mut refocus = Refocus::default();
    assert!(!refocus.seen(window::Id::unique(), true, Instant::now()));
}

#[test]
fn a_switch_between_own_windows_is_no_return() {
    let (main, dialog) = (window::Id::unique(), window::Id::unique());
    let start = Instant::now();
    let mut refocus = Refocus::default();
    refocus.seen(main, true, start);
    // Main window to a dialog's window and back, each in a few milliseconds.
    assert!(!refocus.seen(main, false, after(start, 1_000)));
    assert!(!refocus.seen(dialog, true, after(start, 1_010)));
    assert!(!refocus.seen(dialog, false, after(start, 5_000)));
    assert!(!refocus.seen(main, true, after(start, 5_020)));
}

#[test]
fn the_new_window_s_focus_before_the_old_one_s_loss_is_no_return() {
    let (main, dialog) = (window::Id::unique(), window::Id::unique());
    let start = Instant::now();
    let mut refocus = Refocus::default();
    refocus.seen(main, true, start);
    assert!(!refocus.seen(dialog, true, after(start, 1_000)));
    // The main window's loss comes late and changes nothing: the dialog has the keyboard.
    assert!(!refocus.seen(main, false, after(start, 1_001)));
    assert!(!refocus.seen(main, true, after(start, 9_000)));
}

#[test]
fn coming_back_from_another_program_is_a_return() {
    let main = window::Id::unique();
    let start = Instant::now();
    let mut refocus = Refocus::default();
    refocus.seen(main, true, start);
    assert!(!refocus.seen(main, false, after(start, 1_000)));
    assert!(refocus.seen(main, true, after(start, 4_000)));
    // Once: a second focus without a loss between is none.
    assert!(!refocus.seen(main, true, after(start, 9_000)));
    // Into a dialog's window counts as well: the app was away.
    let dialog = window::Id::unique();
    refocus.seen(main, false, after(start, 10_000));
    assert!(refocus.seen(dialog, true, after(start, 12_000)));
}

#[test]
fn the_threshold_is_three_hundred_milliseconds() {
    assert_eq!(AWAY, Duration::from_millis(300));
    let main = window::Id::unique();
    let start = Instant::now();
    let mut quick = Refocus::default();
    quick.seen(main, true, start);
    quick.seen(main, false, after(start, 1_000));
    assert!(!quick.seen(main, true, after(start, 1_299)));
    let mut away = Refocus::default();
    away.seen(main, true, start);
    away.seen(main, false, after(start, 1_000));
    assert!(away.seen(main, true, after(start, 1_300)));
}

#[test]
fn the_loss_of_a_window_without_the_keyboard_starts_no_clock() {
    let (main, other) = (window::Id::unique(), window::Id::unique());
    let start = Instant::now();
    let mut refocus = Refocus::default();
    refocus.seen(main, true, start);
    assert!(!refocus.seen(other, false, after(start, 100)));
    assert!(!refocus.seen(main, true, after(start, 5_000)));
}

/// A shown file list of `path`, plain and complete.
fn files(path: &VfsPath) -> Shown<'_> {
    Shown {
        path,
        files: true,
        plain: true,
        settled: true,
        quiet: false,
    }
}

const DEFAULTS: Switches = Switches {
    on: true,
    network: false,
};

#[test]
fn the_defaults_are_the_schema_s() {
    assert_eq!(Switches::default(), DEFAULTS);
}

#[test]
fn a_local_folder_and_a_wsl_share_are_read_again() {
    let local = VfsPath::local("/home/me/work");
    assert!(refreshes(files(&local), false, DEFAULTS));
    let wsl = VfsPath::local(r"\\wsl.localhost\Ubuntu\home\me");
    assert!(!on_network(&wsl, &[]));
    assert!(refreshes(files(&wsl), on_network(&wsl, &[]), DEFAULTS));
    let legacy = VfsPath::local(r"\\wsl$\Ubuntu\home\me");
    assert!(!on_network(&legacy, &[]));
}

#[test]
fn the_switch_off_reads_nothing_again() {
    let local = VfsPath::local("/home/me/work");
    let off = Switches {
        on: false,
        network: true,
    };
    assert!(!refreshes(files(&local), false, off));
}

#[test]
fn a_terminal_a_flat_view_and_a_running_listing_are_left_alone() {
    let local = VfsPath::local("/home/me/work");
    let terminal = Shown {
        files: false,
        ..files(&local)
    };
    let flat = Shown {
        plain: false,
        ..files(&local)
    };
    let listing = Shown {
        settled: false,
        ..files(&local)
    };
    for shown in [terminal, flat, listing] {
        assert!(!refreshes(shown, false, DEFAULTS));
    }
}

/// RC-110-02: a panel whose system watcher reported nothing since it was last read is as shown;
/// coming back does not read it.
#[test]
fn a_panel_with_a_quiet_watcher_is_left_alone() {
    let local = VfsPath::local("/home/me/work");
    let quiet = Shown {
        quiet: true,
        ..files(&local)
    };
    assert!(!refreshes(quiet, false, DEFAULTS));
    assert!(
        refreshes(files(&local), false, DEFAULTS),
        "a watcher that saw a change"
    );
}

#[test]
fn servers_and_archives_are_never_read_again() {
    let all = Switches {
        on: true,
        network: true,
    };
    for backend in [
        BackendId::Sftp { connection_id: 1 },
        BackendId::Ftp { connection_id: 2 },
        BackendId::Archive {
            outer: Box::new(VfsPath::local("/home/me/a.zip")),
            format: rc_vfs::ArchiveFormat::Zip,
        },
    ] {
        let path = VfsPath {
            backend,
            path: "/".into(),
        };
        assert!(!refreshes(files(&path), false, all));
    }
}

/// The drive bar's entry of a network drive mounted at `root`.
fn network_drive(root: &str) -> Drive {
    Drive {
        id: root.to_owned(),
        group: DriveGroup::Drives,
        kind: DriveKind::Network,
        label: root.to_owned(),
        root: Some(VfsPath::local(root)),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        folder_buttons: Vec::new(),
        remote: Some(r"\\server\share".to_owned()),
    }
}

#[test]
fn network_drives_and_shares_only_with_their_switch() {
    let drives = [network_drive("/mnt/share")];
    let mapped = VfsPath::local("/mnt/share/docs");
    let share = VfsPath::local(r"\\server\share\docs");
    let local = VfsPath::local("/home/me");
    assert!(on_network(&mapped, &drives));
    assert!(on_network(&share, &[]));
    assert!(!on_network(&local, &drives));
    let network = Switches {
        on: true,
        network: true,
    };
    for path in [&mapped, &share] {
        let on = on_network(path, &drives);
        assert!(!refreshes(files(path), on, DEFAULTS));
        assert!(refreshes(files(path), on, network));
    }
}

/// Both panels on a folder with `a.txt` and `c.txt`, the left cursor on `c.txt` and `a.txt`
/// selected there; the main window has the keyboard.
async fn two_panels() -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["a.txt", "c.txt"] {
        std::fs::write(root.path().join(name), b"x").expect("write");
    }
    let mut harness = Harness::open(root.path(), root.path()).await;
    let left = Some(PanelId::Left);
    for command in [
        Command::SetCursor {
            panel: left,
            target: CursorTarget::Name("c.txt".into()),
        },
        Command::ToggleSelection {
            panel: left,
            names: vec!["a.txt".into()],
        },
    ] {
        harness.app.engine().dispatch(command).await.expect("done");
    }
    harness.settle().await;
    let main = window::Id::unique();
    harness
        .send(Message::Queue(QueueMsg::Focused(main, true)))
        .await;
    (root, harness, main)
}

fn names(harness: &Harness, panel: PanelId) -> Vec<String> {
    let pane = harness.app.pane(panel);
    pane.entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[tokio::test]
async fn coming_back_reads_both_panels_again_and_keeps_cursor_and_selection() {
    let (root, mut harness, main) = two_panels().await;
    harness
        .send(Message::Queue(QueueMsg::Focused(main, false)))
        .await;
    std::fs::write(root.path().join("b.txt"), b"saved").expect("write");
    // The app was away longer than the threshold: the loss is moved back in time.
    let earlier = Instant::now()
        .checked_sub(Duration::from_secs(2))
        .expect("an earlier instant");
    harness.app.dialog_windows.refocus.seen(main, true, earlier);
    harness
        .app
        .dialog_windows
        .refocus
        .seen(main, false, earlier);
    harness
        .send(Message::Queue(QueueMsg::Focused(main, true)))
        .await;
    for panel in PanelId::BOTH {
        assert_eq!(names(&harness, panel), ["a.txt", "b.txt", "c.txt"]);
    }
    let left = &harness.app.pane(PanelId::Left).state;
    assert_eq!(left.entries[left.cursor].name, "c.txt");
    assert!(left.selection.contains("a.txt"));
}

#[tokio::test]
async fn a_switch_between_own_windows_reads_nothing_again() {
    let (root, mut harness, main) = two_panels().await;
    std::fs::write(root.path().join("b.txt"), b"saved").expect("write");
    let dialog = window::Id::unique();
    for (id, focused) in [(main, false), (dialog, true), (dialog, false), (main, true)] {
        harness
            .send(Message::Queue(QueueMsg::Focused(id, focused)))
            .await;
    }
    for panel in PanelId::BOTH {
        assert_eq!(names(&harness, panel), ["a.txt", "c.txt"]);
    }
}

#[tokio::test]
async fn the_switches_read_decide() {
    let (root, mut harness, _) = two_panels().await;
    std::fs::write(root.path().join("b.txt"), b"saved").expect("write");
    let off = Switches {
        on: false,
        network: false,
    };
    harness.send(Message::Refocus(off)).await;
    assert_eq!(names(&harness, PanelId::Left), ["a.txt", "c.txt"]);
    harness.send(Message::Refocus(DEFAULTS)).await;
    assert_eq!(names(&harness, PanelId::Left), ["a.txt", "b.txt", "c.txt"]);
}

/// Linux and macOS: the main window's focus under a modal dialog's window goes back to it, once
/// while that is under way (RC-080-73); it is no return from another program.
#[cfg(not(windows))]
#[tokio::test]
async fn a_focus_under_a_modal_dialog_is_handed_back_once() {
    use crate::app::{
        dialog_windows::{DialogWindow, DialogWindows},
        window_state::Geometry,
    };
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    let geometry = Geometry {
        width: 900.0,
        height: 640.0,
        position: None,
        maximized: false,
    };
    harness
        .app
        .dialog_windows
        .opened(DialogWindow::Settings, window::Id::unique(), geometry);
    assert_eq!(harness.app.refocused(main, true).len(), 1);
    assert!(harness.app.refocused(main, true).is_empty(), "under way");
}
