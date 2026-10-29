//! The access keys beside the buttons (RC-080-16): a checkbox toggles on the letter its label
//! marks; the letter of a caption gives its text field the keyboard and types nothing, so the
//! next key is the field's; a pick list or an option group gets the ring and sends nothing. In
//! the dialogs: find files, properties and the Shortcuts tab, whose letters go to the recording
//! while a key is recorded.

use iced::{
    Element,
    widget::{column, pick_list, text_input},
};

use super::{
    probe::{Screen, beside_a_file, harness, walk_round},
    ring_settings_tests::{settings_open, settings_window},
};
use crate::{
    app::{
        Message, properties,
        properties::PropsMsg,
        search,
        search::SearchMsg,
        settings::{Page, SettingsMessage, Tab},
    },
    theme::{
        self, focus,
        layout::{self, Head},
        mnemonic::Scope,
    },
};

/// A small card with a checkbox, a captioned field and a captioned pick list, and a button.
fn form<'a>() -> Element<'a, Message> {
    let field = text_input("", "").on_input(|_| Message::Saved);
    let sizes = [1_u8, 2, 3];
    let list = pick_list(sizes, Some(2), |_| Message::Press);
    let body = column![
        layout::check("&Hidden", false, true, Message::AccessKeys),
        layout::caption("&Name", None),
        layout::keyed(focus::Stop::field(field, "&Name"), "&Name"),
        layout::caption("&Size", None),
        layout::keyed(
            focus::Stop::pick(
                list,
                "&Size",
                focus::moves(&sizes, Some(&2), &|_| Message::Press)
            ),
            "&Size"
        ),
        layout::secondary_button("&Keep", Some(Message::Press)),
    ];
    let head = Head::new(iced::widget::text("Form"), Message::Press);
    layout::dialog_card(head, body, layout::SMALL_DIALOG_WIDTH, theme::tokens(false))
}

#[tokio::test]
async fn a_checkbox_toggles_on_its_letter() {
    let mut screen = Screen::new(form()).await;
    let stops = walk_round(&mut screen, "form");
    assert_eq!(stops[0].letter, Some('h'));
    let (messages, taken) = screen.type_letter('h', true);
    assert!(taken);
    assert!(
        matches!(messages[..], [Message::AccessKeys(true)]),
        "{messages:?}"
    );
    assert_eq!(screen.at(), Some(0), "the ring on the checkbox");
    // A checkbox has the ring: the letter alone presses Keep.
    let (messages, _) = screen.type_letter('k', false);
    assert!(matches!(messages[..], [Message::Press]), "{messages:?}");
}

#[tokio::test]
async fn a_captions_letter_gives_its_field_the_keyboard_and_types_nothing() {
    let mut screen = Screen::new(form()).await;
    let stops = walk_round(&mut screen, "form");
    let name = stops
        .iter()
        .position(|seen| seen.letter == Some('n'))
        .expect("the field answers to N");
    assert_eq!(stops[name].kind, None, "still a text field");
    let (messages, taken) = screen.type_letter('n', true);
    assert!(taken && messages.is_empty(), "{messages:?}");
    assert_eq!(screen.at(), Some(name));
    // The field has the keyboard: the next letter is typed, Keep's too.
    let (typed, _) = screen.type_letter('k', false);
    assert!(matches!(typed[..], [Message::Saved]), "{typed:?}");
    assert_eq!(screen.at(), Some(name), "typed, not pressed");
    let size = stops
        .iter()
        .position(|seen| seen.letter == Some('s'))
        .expect("the list answers to S");
    let (messages, taken) = screen.type_letter('s', true);
    assert!(taken && messages.is_empty(), "{messages:?}");
    assert_eq!(screen.at(), Some(size), "the ring on the list");
}

#[tokio::test]
async fn find_text_takes_the_keyboard_on_alt_t() {
    let (_root, mut harness) = harness().await;
    harness.press("Alt+F7").await;
    let dialog = harness.app.search.as_ref().expect("the dialog");
    let mut screen = Screen::new(search::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "find files");
    let text = stops
        .iter()
        .position(|seen| seen.letter == Some('t'))
        .expect("Find text answers to T");
    let (messages, taken) = screen.type_letter('t', true);
    assert!(taken && messages.is_empty(), "{messages:?}");
    assert_eq!(screen.at(), Some(text));
    let (typed, _) = screen.type_letter('x', false);
    assert!(
        matches!(&typed[..], [Message::Search(SearchMsg::Text(_, written))] if written == "x"),
        "{typed:?}"
    );
}

#[tokio::test]
async fn alt_r_ticks_read_only_in_the_properties() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("Alt+Enter").await;
    let dialog = harness.app.properties.as_ref().expect("the properties");
    let mut screen = Screen::new(properties::overlay(dialog, theme::tokens(false))).await;
    walk_round(&mut screen, "properties");
    let (messages, taken) = screen.type_letter('r', true);
    assert!(taken);
    assert!(
        matches!(messages[..], [Message::Properties(PropsMsg::Form(_))]),
        "{messages:?}"
    );
}

#[tokio::test]
async fn a_paused_scope_presses_nothing_while_a_shortcut_is_recorded() {
    let mut screen =
        Screen::in_scope(crate::theme::mnemonic::Scope::new(form()).paused(true)).await;
    let (messages, taken) = screen.type_letter('k', true);
    assert!(!taken, "the key goes on to the recording");
    assert!(messages.is_empty(), "{messages:?}");
    let (messages, taken) = screen.type_letter('h', true);
    assert!(!taken && messages.is_empty(), "{messages:?}");
}

/// The Shortcuts tab: Alt+C is Change key; once it records, the window's scope is paused, no
/// letter of the tab presses its button, and the key goes on to the recording - Alt with a
/// letter is one the window keeps, so the recording refuses it and waits for another.
#[tokio::test]
async fn the_shortcuts_tabs_letters_go_to_the_recording_while_it_records() {
    let (_root, mut harness) = settings_open().await;
    for message in [
        SettingsMessage::Page(Page::Keyboard),
        SettingsMessage::Tab(Tab::Shortcuts),
    ] {
        harness.send(Message::Settings(message)).await;
    }
    harness.press("ArrowDown").await;
    let change = {
        let mut screen = Screen::new(settings_window(&harness)).await;
        let stops = walk_round(&mut screen, "shortcuts, a row selected");
        for letter in ['s', 'c', 'd', 'r', 'e', 'l'] {
            assert!(
                stops.iter().any(|seen| seen.letter == Some(letter)),
                "{letter}: {stops:?}"
            );
        }
        let (messages, taken) = screen.type_letter('c', true);
        assert!(taken);
        messages
    };
    assert!(
        matches!(
            change[..],
            [Message::Settings(SettingsMessage::Shortcuts(_))]
        ),
        "{change:?}"
    );
    for message in change {
        harness.send(message).await;
    }
    assert!(harness.app.records_shortcut(), "Change key records");
    {
        let scope = Scope::new(settings_window(&harness)).paused(harness.app.records_shortcut());
        let mut screen = Screen::in_scope(scope).await;
        for letter in ['c', 'd', 'r', 'e', 'l', 'a'] {
            let (messages, taken) = screen.type_letter(letter, true);
            assert!(!taken && messages.is_empty(), "Alt+{letter}: {messages:?}");
        }
    }
    harness.press("Alt+R").await;
    assert!(
        harness.app.records_shortcut(),
        "the recording took Alt+R, refused it and waits"
    );
    harness.press("F6").await;
    assert!(!harness.app.records_shortcut(), "F6 is recorded");
}
