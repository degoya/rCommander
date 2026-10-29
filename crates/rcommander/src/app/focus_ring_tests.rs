//! The focus ring of the dialogs (RC-080-16), each dialog opened through the app as a person
//! opens it, laid out without a window and walked by the operations Tab runs: Tab reaches every
//! stop once, in reading order, and comes round to the first; Shift+Tab goes back; Enter presses
//! the focused button, Space toggles the focused checkbox, an arrow chooses in a focused list;
//! and no button of a dialog is left out of its ring but the X of its head. The other dialogs'
//! rings are in the `focus_ring_*_tests.rs` beside this file.

use iced::{
    Element,
    keyboard::key::Named,
    widget::{Id, Space},
};

use super::probe::{Screen, harness, kinds, press_on, walk_round};
use crate::{
    app::{
        Message, connections, file_ops, quick_connect, search,
        search::{SearchMsg, Tab},
        settings::SettingsMessage,
        vault_offer::VaultOfferMsg,
    },
    theme::{self, focus::Kind, layout},
};

#[tokio::test]
async fn the_vault_offer_walks_its_two_buttons() {
    let (_root, mut harness) = harness().await;
    harness.app.vault_offer = true;
    let layer = harness
        .app
        .vault_offer_layer(theme::tokens(false))
        .expect("the offer");
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "vault offer");
    assert_eq!(kinds(&stops), [Some(Kind::Button); 2], "Remove, Keep");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let keep = press_on(&mut screen, "Keep", Named::Enter);
    assert!(matches!(
        keep[..],
        [Message::VaultOffer(VaultOfferMsg::Keep)]
    ));
    let keep = press_on(&mut screen, "Keep", Named::Space);
    assert!(matches!(
        keep[..],
        [Message::VaultOffer(VaultOfferMsg::Keep)]
    ));
    // A key a stop does not take is the dialog's.
    let (messages, taken) = screen.press(Named::Escape);
    assert!(messages.is_empty() && !taken);
}

#[tokio::test]
async fn the_settings_window_walks_its_pages_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let tokens = theme::tokens(false);
    let dialog = harness.app.settings.as_ref().expect("the settings");
    let mut screen = Screen::new(layout::window_body(
        dialog.view(harness.app.settings_tabs, tokens),
        tokens,
    ))
    .await;
    let stops = walk_round(&mut screen, "settings");
    assert_eq!(stops[0].kind, Some(Kind::Choice), "the page list first");
    assert!(stops.iter().any(|seen| seen.text.contains("Cancel")));
    assert_eq!(
        screen.loose_buttons(),
        0,
        "the window's title bar has the X"
    );
    let page = press_on(&mut screen, "Appearance", Named::ArrowDown);
    assert!(matches!(
        page[..],
        [Message::Settings(SettingsMessage::Page(_))]
    ));
}

#[tokio::test]
async fn the_connection_manager_follows_the_ring() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    let tokens = theme::tokens(false);
    let id = {
        let manager = harness.app.connections.as_ref().expect("the manager");
        let mut screen = Screen::new(connections::layer(manager, tokens)).await;
        let stops = walk_round(&mut screen, "connection manager");
        let filter = stops[0].id.as_deref().unwrap_or_default();
        assert!(filter.contains("connections.filter"), "{stops:?}");
        assert_eq!(stops[1].kind, Some(Kind::Choice), "the tree");
        for label in ["New connection", "New folder", "Close"] {
            assert!(
                stops.iter().any(|seen| seen.text.contains(label)),
                "{label} in {stops:?}"
            );
        }
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        for _ in 0..stops.len() {
            if screen.at() == Some(1) {
                break;
            }
            screen.tab(false);
        }
        screen.focused_id()
    };
    assert_eq!(id, Some(Id::new(connections::TREE_STOP_ID)));
    harness
        .send(Message::Connections(connections::ConnMsg::Focused(id)))
        .await;
    let manager = harness.app.connections.as_ref().expect("the manager");
    assert_eq!(manager.focus, connections::Focus::Tree);
}

#[tokio::test]
async fn quick_connect_walks_its_fields_switch_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+N").await;
    let dialog = harness.app.quick.as_ref().expect("the dialog");
    let mut screen = Screen::new(quick_connect::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "quick connect");
    assert!(stops.iter().any(|seen| seen.kind.is_none()), "its fields");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let save = press_on(&mut screen, "Save as connection", Named::Space);
    assert!(matches!(
        save[..],
        [Message::QuickConnect(quick_connect::QuickMsg::Save(true))]
    ));
}

#[tokio::test]
async fn find_files_walks_its_tabs_fields_and_hits() {
    let (_root, mut harness) = harness().await;
    harness.press("Alt+F7").await;
    let dialog = harness.app.search.as_ref().expect("the dialog");
    let mut screen = Screen::new(search::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "find files");
    assert_eq!(stops[0].kind, Some(Kind::Choice), "the tabs first");
    assert!(kinds(&stops).contains(&Some(Kind::Toggle)), "the switches");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let tab = press_on(&mut screen, "General", Named::ArrowRight);
    assert!(matches!(
        tab[..],
        [Message::Search(SearchMsg::Tab(Tab::Advanced))]
    ));
}

#[tokio::test]
async fn the_new_folder_dialog_walks_its_field_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.press("F7").await;
    assert!(
        harness.app.file_ops.dialog.is_some(),
        "F7 asks for the name"
    );
    let base: Element<'_, Message> = Space::new().into();
    let layer = harness.app.file_ops_layer(base, theme::tokens(false));
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "new folder");
    assert_eq!(stops[0].kind, None, "the name first");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let cancel = press_on(&mut screen, "Cancel", Named::Enter);
    assert!(matches!(
        cancel[..],
        [Message::FileOps(file_ops::FileOpsMsg::Close)]
    ));
}
