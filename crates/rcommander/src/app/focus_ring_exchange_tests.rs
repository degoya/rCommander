//! The focus ring (RC-080-16) of the database dialog's export and import (RC-080-19): the modes,
//! the path, every box, password field and button in reading order, Space and Enter on each;
//! the import's rows a stop each once the file is read. No button but the head's X is left out.

use iced::keyboard::key::Named;
use rc_engine::exchange::{ExchangePart, ExchangeReport, ExchangeRow, ExportParts, RowStatus};

use super::probe::{
    Screen, Seen, harness, kinds, press_at, press_every_stop, press_on, walk_round,
};
use crate::{
    app::{
        Message,
        database_dialog::{self, DatabaseMsg, DialogMode, ExchangeMsg},
    },
    theme::{self, focus::Kind},
};

/// The dialog's default button: the last button before Close (the tabs name the modes too).
fn primary(stops: &[Seen]) -> usize {
    let buttons: Vec<usize> = (0..stops.len())
        .filter(|&at| stops[at].kind == Some(Kind::Button))
        .collect();
    buttons[buttons.len() - 2]
}

fn row(key: &str, status: RowStatus, selected: bool) -> ExchangeRow {
    ExchangeRow {
        key: key.to_owned(),
        part: ExchangePart::Connections,
        name: key.to_owned(),
        old: None,
        new: Some("ssh web.example.org:22".to_owned()),
        warning: None,
        secrets: false,
        selected,
        status,
    }
}

#[tokio::test]
async fn the_export_walks_its_parts_passwords_and_buttons() {
    let (_root, mut harness) = harness().await;
    let open = DatabaseMsg::OpenMode(DialogMode::Export);
    harness.send(Message::Database(open)).await;
    let dialog = harness.app.database.as_ref().expect("the dialog");
    let mut screen = Screen::new(database_dialog::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "export");
    assert_eq!(
        kinds(&stops)[..2],
        [Some(Kind::Choice), None],
        "the modes, the path"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "export");
    let part = press_on(&mut screen, "Settings", Named::Space);
    assert!(matches!(
        part[..],
        [Message::Database(DatabaseMsg::Exchange(ExchangeMsg::Part(
            _,
            false
        )))]
    ));
    let encrypt = press_on(&mut screen, "Encrypt", Named::Space);
    assert!(matches!(
        encrypt[..],
        [Message::Database(DatabaseMsg::Exchange(
            ExchangeMsg::Encrypted(false)
        ))]
    ));
    let export = press_at(&mut screen, primary(&stops), Named::Enter);
    assert!(matches!(
        export[..],
        [Message::Database(DatabaseMsg::Confirm)]
    ));
}

#[tokio::test]
async fn the_import_walks_its_password_rows_and_buttons() {
    let (_root, mut harness) = harness().await;
    let open = DatabaseMsg::OpenMode(DialogMode::Import);
    harness.send(Message::Database(open)).await;
    {
        let dialog = harness.app.database.as_ref().expect("the dialog");
        let mut screen = Screen::new(database_dialog::overlay(dialog, theme::tokens(false))).await;
        let stops = walk_round(&mut screen, "import");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "import");
        let read = press_on(&mut screen, "Read", Named::Enter);
        assert!(matches!(
            read[..],
            [Message::Database(DatabaseMsg::Exchange(ExchangeMsg::Read))]
        ));
    }
    let dialog = harness.app.database.as_mut().expect("the dialog");
    dialog.exchange.previewed(ExchangeReport {
        dry_run: true,
        encrypted: true,
        created_at: "2026-10-18T10:00:00Z".to_owned(),
        secrets: false,
        parts: ExportParts::ALL,
        rows: vec![
            row("connection:a", RowStatus::New, true),
            row("connection:b", RowStatus::Exists, false),
        ],
    });
    let dialog = harness.app.database.as_ref().expect("the dialog");
    let mut screen = Screen::new(database_dialog::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "import preview");
    assert_eq!(
        kinds(&stops)
            .iter()
            .filter(|kind| **kind == Some(Kind::Toggle))
            .count(),
        1,
        "a box for the new row only"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "import preview");
    let tick = press_on(&mut screen, "connection:a", Named::Space);
    assert!(matches!(
        tick[..],
        [Message::Database(DatabaseMsg::Exchange(
            ExchangeMsg::Choose(_, false)
        ))]
    ));
    let import = press_at(&mut screen, primary(&stops), Named::Enter);
    assert!(matches!(
        import[..],
        [Message::Database(DatabaseMsg::Confirm)]
    ));
}
