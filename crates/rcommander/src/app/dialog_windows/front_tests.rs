//! Which window has the keyboard after a dialog's window went, and where a focus under a question
//! or a modal dialog goes back to (RC-080-73): the choices, the hand-back's guard against
//! ping-pong, and the books that feed them. The windows themselves need a display.

use std::time::{Duration, Instant};

use iced::window;

use super::{HandBack, SETTLE, Up, after_close, top, under};
use crate::app::{
    dialog_windows::{DialogWindow, DialogWindows, questions::Question},
    window_state::Geometry,
};

fn up(kind: DialogWindow) -> Up {
    Up {
        kind,
        id: window::Id::unique(),
    }
}

const GEOMETRY: Geometry = Geometry {
    width: 600.0,
    height: 400.0,
    position: None,
    maximized: false,
};

#[test]
fn a_question_over_a_modal_dialog_gives_the_keyboard_to_the_dialog() {
    let main = Some(window::Id::unique());
    let settings = up(DialogWindow::Settings);
    // The question went: the settings are left, opened from the main window.
    assert_eq!(after_close(&[settings], main, main), Some(settings.id));
    // While the question is up it is the top one, whichever opened later.
    let question = up(DialogWindow::Question(Question::Mcp));
    assert_eq!(top(&[question, settings]), Some(question.id));
}

#[test]
fn a_modal_dialog_over_a_modeless_one_gives_it_back_the_keyboard_it_was_opened_from() {
    let main = Some(window::Id::unique());
    let connections = up(DialogWindow::Connections);
    let queue = up(DialogWindow::Queue);
    // "Run command" opened from the connection manager went.
    assert_eq!(
        after_close(&[connections, queue], Some(connections.id), main),
        Some(connections.id)
    );
}

#[test]
fn a_lister_without_an_owner_gives_the_keyboard_to_where_it_was_opened() {
    let main = Some(window::Id::unique());
    let search = up(DialogWindow::Search);
    assert_eq!(after_close(&[search], main, main), main);
    // Opened from find files' results, it goes back there.
    assert_eq!(
        after_close(&[search], Some(search.id), main),
        Some(search.id)
    );
    // A modal dialog up meanwhile comes first.
    let delete = up(DialogWindow::Delete);
    assert_eq!(
        after_close(&[search, delete], Some(search.id), main),
        Some(delete.id)
    );
}

#[test]
fn the_last_dialog_gives_the_keyboard_to_the_main_window() {
    let main = Some(window::Id::unique());
    assert_eq!(after_close(&[], None, main), main);
    // The window it was opened from is gone.
    assert_eq!(after_close(&[], Some(window::Id::unique()), main), main);
}

#[test]
fn the_newest_modal_dialog_is_on_top() {
    let progress = up(DialogWindow::Progress);
    let conflict = up(DialogWindow::Conflict);
    let lister = up(DialogWindow::Lister(1));
    assert_eq!(top(&[progress, conflict, lister]), Some(conflict.id));
    assert_eq!(top(&[lister]), None, "a modeless window is never on top");
}

#[test]
fn a_focus_on_the_main_window_under_a_modal_dialog_goes_back_to_it() {
    let main = window::Id::unique();
    let settings = up(DialogWindow::Settings);
    assert_eq!(
        under(&[settings], None, Some(main), main),
        Some(settings.id)
    );
    assert_eq!(under(&[settings], None, Some(main), settings.id), None);
    // No modal dialog: the main window keeps it.
    let queue = up(DialogWindow::Queue);
    assert_eq!(under(&[queue], None, Some(main), main), None);
}

#[test]
fn a_focus_on_a_modal_dialog_under_a_newer_one_or_a_question_goes_to_the_top() {
    let main = Some(window::Id::unique());
    let progress = up(DialogWindow::Progress);
    let conflict = up(DialogWindow::Conflict);
    assert_eq!(
        under(&[progress, conflict], None, main, progress.id),
        Some(conflict.id)
    );
    let settings = up(DialogWindow::Settings);
    let question = up(DialogWindow::Question(Question::HostKey));
    let windows = [settings, question];
    assert_eq!(
        under(&windows, Some(settings.id), main, settings.id),
        Some(question.id)
    );
    assert_eq!(under(&windows, Some(settings.id), main, question.id), None);
}

#[test]
fn a_modeless_window_keeps_the_focus_beside_a_modal_one() {
    let main = Some(window::Id::unique());
    let settings = up(DialogWindow::Settings);
    let lister = up(DialogWindow::Lister(3));
    let queue = up(DialogWindow::Queue);
    let windows = [lister, settings, queue];
    assert_eq!(under(&windows, None, main, lister.id), None);
    assert_eq!(under(&windows, None, main, queue.id), None);
    // A window the books do not know (gone, or another app's id) is left alone.
    assert_eq!(under(&windows, None, main, window::Id::unique()), None);
}

#[test]
fn the_sync_confirmation_locks_the_compare_window_only() {
    let main = window::Id::unique();
    let compare = up(DialogWindow::Compare);
    let confirm = up(DialogWindow::Question(Question::SyncConfirm));
    let windows = [compare, confirm];
    assert_eq!(
        under(&windows, Some(compare.id), Some(main), compare.id),
        Some(confirm.id)
    );
    assert_eq!(under(&windows, Some(compare.id), Some(main), main), None);
}

#[test]
fn a_hand_back_is_sent_once_until_it_lands_or_settles() {
    let mut handing = HandBack::default();
    let (main, settings) = (window::Id::unique(), window::Id::unique());
    let start = Instant::now();
    assert_eq!(handing.hand(main, Some(settings), start), Some(settings));
    // The window manager gives the main window the focus again at once: no second request.
    let soon = start + Duration::from_millis(50);
    assert_eq!(handing.hand(main, Some(settings), soon), None);
    // Refused for longer than the settle time (Wayland): one more try, no loop.
    let later = start + SETTLE + Duration::from_millis(1);
    assert_eq!(handing.hand(main, Some(settings), later), Some(settings));
    // It landed: the next click on the main window is handed back at once.
    assert_eq!(handing.hand(settings, None, later), None);
    assert_eq!(handing.hand(main, Some(settings), later), Some(settings));
}

#[test]
fn the_books_name_the_window_that_had_the_keyboard_when_one_opened() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    books.focus(main, true);
    let lister = window::Id::unique();
    books.opened(DialogWindow::Lister(1), lister, GEOMETRY);
    books.focus(main, false);
    books.focus(lister, true);
    let delete = window::Id::unique();
    books.opened(DialogWindow::Delete, delete, GEOMETRY);
    books.focus(lister, false);
    books.focus(delete, true);
    // Delete from the Lister: the keyboard goes back to it.
    assert_eq!(books.front_after(delete), Some(lister));
    // A window without the keyboard takes nobody's focus when it goes.
    assert_eq!(books.front_after(lister), None);
}

#[test]
fn a_window_that_goes_while_another_program_has_the_keyboard_moves_no_focus() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let progress = window::Id::unique();
    books.opened(DialogWindow::Progress, progress, GEOMETRY);
    books.focus(progress, true);
    books.focus(progress, false);
    assert_eq!(books.front_after(progress), None);
}

#[test]
fn a_question_closed_before_its_owner_leaves_the_choice_to_the_owners_close() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let settings = window::Id::unique();
    books.opened(DialogWindow::Settings, settings, GEOMETRY);
    let question = window::Id::unique();
    books.opened(DialogWindow::Question(Question::Quit), question, GEOMETRY);
    books.focus(question, true);
    assert_eq!(books.front_after(question), Some(settings));
    let _ = books.closed(question);
    // The question's window is gone and told no loss yet: the keyboard is still the app's.
    assert_eq!(books.front_after(settings), Some(main));
}

#[test]
fn the_books_hand_a_focus_under_a_question_back_to_it() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    let settings = window::Id::unique();
    books.opened(DialogWindow::Settings, settings, GEOMETRY);
    let question = window::Id::unique();
    books.opened(DialogWindow::Question(Question::Mcp), question, GEOMETRY);
    books.question_owner = Some((question, settings));
    let now = Instant::now();
    assert_eq!(books.hand_back(settings, now), Some(question));
    assert_eq!(books.hand_back(question, now), None, "landed");
    assert_eq!(books.hand_back(main, now), Some(question));
}

#[test]
fn a_window_still_being_made_is_not_on_top() {
    let main = window::Id::unique();
    let mut books = DialogWindows::new(main, Vec::new());
    books.opening(DialogWindow::Settings, window::Id::unique(), GEOMETRY);
    assert!(books.up().is_empty());
    assert_eq!(books.hand_back(main, Instant::now()), None);
}
