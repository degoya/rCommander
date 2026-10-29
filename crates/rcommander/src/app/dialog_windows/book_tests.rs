use iced::{Point, Size, window};
use rc_engine::JobId;

use super::{DialogWindow, DialogWindows};
use crate::app::window_state::{Geometry, WindowChange};

const SETTINGS: DialogWindow = DialogWindow::Settings;

fn main_at(x: f32, y: f32) -> Geometry {
    Geometry {
        width: 1300.0,
        height: 840.0,
        position: Some((x, y)),
        maximized: false,
    }
}

#[test]
fn a_dialog_the_state_shows_gets_a_window_and_loses_it_when_it_closes() {
    let mut books = DialogWindows::new(window::Id::unique(), Vec::new());
    let (opening, closing) = books.difference(&[SETTINGS]);
    assert_eq!(opening, [SETTINGS]);
    assert!(closing.is_empty());

    let id = window::Id::unique();
    books.opened(SETTINGS, id, books.geometry(SETTINGS, main_at(0.0, 0.0)));
    assert_eq!(books.kind_of(id), Some(SETTINGS));
    assert_ne!(books.main(), Some(id));
    // Open already: nothing to do while the state still shows it.
    assert_eq!(books.difference(&[SETTINGS]), (Vec::new(), Vec::new()));

    let (opening, closing) = books.difference(&[]);
    assert!(opening.is_empty());
    assert_eq!(closing, [id]);
    assert_eq!(books.closed(id).map(|(kind, _)| kind), Some(SETTINGS));
    assert_eq!(books.kind_of(id), None);
    assert_eq!(books.closed(id), None, "closed once");
}

#[test]
fn the_main_window_is_no_dialogs_window() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    assert_eq!(books.main(), Some(main));
    assert_eq!(books.kind_of(main), None);
    let moved = WindowChange::Moved(Point::new(10.0, 10.0));
    assert!(
        !books.observe(main, moved, false),
        "the main window's own books keep it"
    );
}

#[test]
fn the_first_opening_is_centred_over_the_main_window() {
    let books = DialogWindows::default();
    let size = SETTINGS.default_size();
    let geometry = books.geometry(SETTINGS, main_at(100.0, 50.0));
    assert_eq!((geometry.width, geometry.height), (size.width, size.height));
    let (x, y) = geometry.position.expect("a place over the main window");
    assert_eq!(x, 100.0 + (1300.0 - size.width) / 2.0);
    assert_eq!(y, 50.0 + (840.0 - size.height) / 2.0);

    let maximized = Geometry {
        maximized: true,
        ..main_at(100.0, 50.0)
    };
    assert_eq!(
        books.geometry(SETTINGS, maximized).position,
        None,
        "the screen's centre"
    );
    let unknown = Geometry {
        position: None,
        ..main_at(0.0, 0.0)
    };
    assert_eq!(books.geometry(SETTINGS, unknown).position, None);
}

#[test]
fn a_dialog_window_opens_where_it_was_left() {
    let mut books = DialogWindows::new(window::Id::unique(), Vec::new());
    let id = window::Id::unique();
    books.opened(SETTINGS, id, books.geometry(SETTINGS, main_at(0.0, 0.0)));
    assert!(books.observe(id, WindowChange::Moved(Point::new(1700.0, 220.0)), false));
    assert!(books.observe(id, WindowChange::Resized(Size::new(1000.0, 700.0)), false));
    let left = Geometry {
        width: 1000.0,
        height: 700.0,
        position: Some((1700.0, 220.0)),
        maximized: false,
    };
    assert_eq!(books.open_geometries(), [(SETTINGS, left)]);
    assert_eq!(books.closed(id), Some((SETTINGS, left)));
    assert_eq!(books.geometry(SETTINGS, main_at(0.0, 0.0)), left);
}

#[test]
fn a_stored_place_comes_back_after_a_restart() {
    let stored = Geometry {
        width: 820.0,
        height: 600.0,
        position: Some((-1500.0, 80.0)),
        maximized: false,
    };
    let books = DialogWindows::new(window::Id::unique(), vec![(SETTINGS, stored)]);
    assert_eq!(books.geometry(SETTINGS, main_at(0.0, 0.0)), stored);
}

#[test]
fn every_dialog_window_has_a_key_of_its_own_and_fits_its_first_size() {
    let keys: Vec<&str> = DialogWindow::ALL.iter().map(|kind| kind.key()).collect();
    for (index, key) in keys.iter().enumerate() {
        assert!(key.starts_with("window.dialog."), "{key}");
        assert!(!keys[index + 1..].contains(key), "{key} twice");
    }
    for kind in DialogWindow::ALL {
        let (first, least) = (kind.default_size(), kind.min_size());
        assert!(first.width >= least.width && first.height >= least.height);
    }
}

/// The settings lock the main window as Total Commander's configuration does, and so does
/// multi-rename; the queue's, the connection manager's, find files' and compare's windows are
/// modeless (owner, 2026-10-18; RC-010-14, RC-080-15): `platform::dialog_window::show` and
/// `close` are given `modal()` and lock or release the main window only for a modal one.
#[test]
fn the_settings_lock_the_main_window_and_the_queue_does_not() {
    assert!(SETTINGS.modal());
    assert!(!DialogWindow::Queue.modal());
    assert!(DialogWindow::MultiRename.modal());
    for kind in [
        DialogWindow::Connections,
        DialogWindow::Search,
        DialogWindow::Compare,
        DialogWindow::Checksum,
        DialogWindow::LoginScripts,
        DialogWindow::Lister(7),
    ] {
        assert!(!kind.modal(), "{kind:?} is modeless");
        assert!(kind.takes_questions());
    }
    assert!(!DialogWindow::Queue.takes_questions());
}

/// The standalone dialogs are modal as in Total Commander, owned by the main window, and take
/// the small questions (RC-080-23); the main window stays locked until the last modal dialog's
/// window closes.
#[test]
fn the_standalone_dialogs_are_modal_and_the_lock_holds_to_the_last() {
    for kind in [
        DialogWindow::Database,
        DialogWindow::Properties,
        DialogWindow::QuickConnect,
        DialogWindow::Network,
        DialogWindow::Archive,
        DialogWindow::ChecksumForm,
        DialogWindow::Hotlist,
        DialogWindow::SshExec,
    ] {
        assert!(kind.modal(), "{kind:?} is modal");
        assert!(kind.owned() && kind.takes_questions(), "{kind:?}");
        assert!(
            DialogWindow::ALL.contains(&kind),
            "{kind:?} keeps its place"
        );
    }
    let main = main_at(0.0, 0.0);
    let mut books = DialogWindows::new(window::Id::unique(), Vec::new());
    assert!(!books.modal_open());
    let (queue, settings, database) = (
        window::Id::unique(),
        window::Id::unique(),
        window::Id::unique(),
    );
    books.opened(
        DialogWindow::Queue,
        queue,
        books.geometry(DialogWindow::Queue, main),
    );
    assert!(!books.modal_open(), "the queue locks nothing");
    books.opened(SETTINGS, settings, books.geometry(SETTINGS, main));
    let at = books.geometry(DialogWindow::Database, main);
    books.opened(DialogWindow::Database, database, at);
    assert!(books.closed(database).is_some());
    assert!(books.modal_open(), "the settings still lock it");
    assert!(books.closed(settings).is_some());
    assert!(!books.modal_open());
}

/// The main window owns every dialog's window but Lister's, a window of its own as in Total
/// Commander; all Listers keep one place.
#[test]
fn listers_are_their_own_windows_and_share_one_place() {
    for kind in DialogWindow::ALL {
        assert_eq!(kind.owned(), !matches!(kind, DialogWindow::Lister(_)));
    }
    assert_eq!(DialogWindow::Lister(1).key(), DialogWindow::Lister(2).key());
    let stored = Geometry {
        width: 700.0,
        height: 500.0,
        position: Some((40.0, 60.0)),
        maximized: false,
    };
    let mut books = DialogWindows::new(
        window::Id::unique(),
        vec![(DialogWindow::Lister(0), stored)],
    );
    let lister = DialogWindow::Lister(3);
    assert_eq!(books.geometry(lister, main_at(0.0, 0.0)), stored);
    let id = window::Id::unique();
    books.opened(lister, id, stored);
    assert!(books.observe(id, WindowChange::Moved(Point::new(900.0, 30.0)), false));
    let next = books.geometry(DialogWindow::Lister(4), main_at(0.0, 0.0));
    assert_eq!(next.position, Some((900.0, 30.0)), "where the open one is");
    assert_eq!(books.difference(&[lister]), (Vec::new(), Vec::new()));
    let (opening, _) = books.difference(&[lister, DialogWindow::Lister(4)]);
    assert_eq!(opening, [DialogWindow::Lister(4)], "a window per Lister");
    books.closed(id);
    let after = books.geometry(DialogWindow::Lister(5), main_at(0.0, 0.0));
    assert_eq!(after.position, Some((900.0, 30.0)), "kept once for all");
}

/// A new question of the file operations wants the main window in front while a modeless
/// dialog's window has the keyboard - the queue's too -, once per question; not while the main
/// window has it, nor while a modal dialog's window locks it.
#[test]
fn a_new_question_brings_the_main_window_to_the_front() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let queue = window::Id::unique();
    let geometry = books.geometry(DialogWindow::Queue, main_at(0.0, 0.0));
    books.opened(DialogWindow::Queue, queue, geometry);
    let (first, second): (JobId, JobId) = (1, 2);

    books.focus(main, true);
    assert_eq!(books.question(Some(first)), None, "the main window has it");
    books.focus(queue, true);
    assert_eq!(books.question(Some(first)), None, "seen already");
    assert_eq!(books.question(Some(second)), Some(main), "a new one");
    assert_eq!(books.question(None), None);

    let settings = window::Id::unique();
    books.opened(SETTINGS, settings, geometry);
    books.focus(settings, true);
    assert_eq!(
        books.question(Some(first)),
        None,
        "the main window is locked"
    );
}

/// The dialog whose window has the keyboard; none while the main window has it.
#[test]
fn the_focused_window_names_its_dialog() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let search = window::Id::unique();
    let geometry = books.geometry(DialogWindow::Search, main_at(0.0, 0.0));
    books.opened(DialogWindow::Search, search, geometry);
    assert_eq!(books.focused_kind(), None);
    books.focus(search, true);
    assert_eq!(books.focused_kind(), Some(DialogWindow::Search));
    books.focus(main, true);
    assert_eq!(books.focused_kind(), None, "the main window is no dialog's");
}

/// A window has the keyboard from its focus event until its loss of it; a late loss of another
/// window's focus changes nothing.
#[test]
fn the_focus_follows_the_system_s_events() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let queue = window::Id::unique();
    let geometry = books.geometry(DialogWindow::Queue, main_at(0.0, 0.0));
    books.opened(DialogWindow::Queue, queue, geometry);
    assert!(!books.has_focus(DialogWindow::Queue));
    books.focus(queue, true);
    books.focus(main, false);
    assert!(books.has_focus(DialogWindow::Queue));
    books.focus(queue, false);
    assert!(!books.has_focus(DialogWindow::Queue));
    books.focus(queue, true);
    assert!(books.closed(queue).is_some());
    assert!(
        !books.has_focus(DialogWindow::Queue),
        "a closed window has no keys"
    );
}

/// The file operations' windows are modal and owned as Total Commander's dialogs (RC-080-23,
/// stage 2), keep their places, and all but the progress window are hidden and taken back
/// rather than made anew; the questions are the ones on top.
#[test]
fn the_file_operations_windows_are_modal_and_the_small_ones_reused() {
    let ops = [
        DialogWindow::Transfer,
        DialogWindow::NewItem,
        DialogWindow::Delete,
        DialogWindow::Conflict,
        DialogWindow::JobError,
        DialogWindow::Progress,
    ];
    for kind in ops {
        assert!(kind.modal() && kind.owned() && kind.file_ops(), "{kind:?}");
        assert!(
            DialogWindow::ALL.contains(&kind),
            "{kind:?} keeps its place"
        );
        assert_eq!(kind.reused(), kind != DialogWindow::Progress, "{kind:?}");
        assert_eq!(
            kind.asks(),
            matches!(kind, DialogWindow::Conflict | DialogWindow::JobError)
        );
    }
    for kind in DialogWindow::ALL {
        assert_eq!(kind.file_ops(), ops.contains(&kind), "{kind:?}");
    }
}

/// A window being made is no window to close yet and has locked nothing; being shown it holds
/// the lock; shown, it closes like any other.
#[test]
fn a_window_closes_only_once_shown() {
    let mut books = DialogWindows::new(window::Id::unique(), Vec::new());
    let id = window::Id::unique();
    let at = books.geometry(DialogWindow::Delete, main_at(0.0, 0.0));
    books.opening(DialogWindow::Delete, id, at);
    assert_eq!(books.kind_of(id), Some(DialogWindow::Delete));
    assert_eq!(books.difference(&[]), (Vec::new(), Vec::new()), "not yet");
    assert_eq!(
        books.difference(&[DialogWindow::Delete]),
        (Vec::new(), Vec::new()),
        "no second window"
    );
    assert!(!books.modal_open(), "it locked nothing");
    assert_eq!(books.visible(DialogWindow::Delete), None);

    books.advance(id, true);
    assert!(books.modal_open(), "showing, it may lock");
    assert_eq!(books.difference(&[]).1, Vec::<window::Id>::new());
    books.advance(id, false);
    assert_eq!(books.visible(DialogWindow::Delete), Some(id));
    assert_eq!(books.difference(&[]).1, [id], "shown, it closes");
}

/// A hidden window comes back for its dialog where it was, booked as being made; a dialog with
/// none hidden gets none.
#[test]
fn a_hidden_window_comes_back_for_its_dialog() {
    let mut books = DialogWindows::new(window::Id::unique(), Vec::new());
    let id = window::Id::unique();
    let at = books.geometry(DialogWindow::NewItem, main_at(0.0, 0.0));
    books.opened(DialogWindow::NewItem, id, at);
    assert!(books.observe(id, WindowChange::Moved(Point::new(300.0, 200.0)), false));
    let (kind, left) = books.closed(id).expect("open");
    books.park(kind, id);
    assert_eq!(books.kind_of(id), None, "hidden, no dialog's");
    assert_eq!(books.reopen(DialogWindow::Delete, left), None);

    let again = books.geometry(DialogWindow::NewItem, main_at(0.0, 0.0));
    assert_eq!(again.position, Some((300.0, 200.0)), "where it was hidden");
    assert_eq!(books.reopen(DialogWindow::NewItem, again), Some(id));
    assert_eq!(books.kind_of(id), Some(DialogWindow::NewItem));
    assert_eq!(books.visible(DialogWindow::NewItem), None, "being shown");
    assert_eq!(
        books.reopen(DialogWindow::NewItem, again),
        None,
        "taken once"
    );
}
