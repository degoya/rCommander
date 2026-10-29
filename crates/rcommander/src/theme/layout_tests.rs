use iced::{
    Element, Length,
    advanced::{Widget, widget::Tree},
    widget::button,
};

use super::{
    Head, button_row, close_style, dialog_card, icon_button, large_dialog, marked_label,
    primary_button, primary_icon_button, secondary_button, secondary_icon_button, warning_card,
};
use crate::theme::{iced_theme, icon::Icon, mnemonic::Label, tokens};

type Row<'a> = Element<'a, ()>;

/// The children of a row of buttons: the extras, the space, the primary, the secondaries.
fn children(row: &Row<'_>) -> usize {
    Widget::children(row.as_widget()).len()
}

#[test]
fn the_row_fills_the_card_with_one_space_before_the_right_hand_buttons() {
    let row: Row<'_> = button_row(
        Some(primary_button("OK", Some(()))),
        [
            secondary_button("Cancel", Some(())),
            secondary_button("Apply", None),
        ],
        [secondary_button("Compare", Some(()))],
    );
    assert_eq!(children(&row), 5);
    assert_eq!(Widget::size(row.as_widget()).width, Length::Fill);
}

#[test]
fn a_row_without_primary_or_extras_keeps_the_space() {
    let row: Row<'_> = button_row(None, [secondary_button("Close", Some(()))], []);
    assert_eq!(children(&row), 2);
    let empty: Row<'_> = button_row(None, [], []);
    assert_eq!(children(&empty), 1);
}

/// The widgets inside a button's face: none for a bare label, the icon and the label for an
/// icon button. A dialog button is a stop of the focus ring around the button (RC-080-16).
fn face_parts(button: &Row<'_>) -> usize {
    let tree = Tree::new(button.as_widget());
    tree.children
        .first()
        .and_then(|button| button.children.first())
        .map_or(0, |face| face.children.len())
}

/// Every dialog button is a stop of the focus ring, a disabled one too (it only takes no keys);
/// the head's X is none.
#[test]
fn dialog_buttons_are_stops_of_the_ring_and_the_close_x_is_not() {
    let stop = iced::advanced::widget::tree::Tag::of::<crate::theme::focus::State>();
    for button in [
        primary_button("OK", Some(())),
        secondary_button("Cancel", Some(())),
        primary_icon_button(Icon::Save, "Save", None),
        secondary_icon_button(Icon::Close, "Close", Some(())),
    ] {
        assert_eq!(Tree::new(button.as_widget()).tag, stop);
    }
    let close: Row<'_> = super::close_button((), tokens(false));
    assert_ne!(Tree::new(close.as_widget()).tag, stop);
}

#[test]
fn an_icon_button_carries_its_icon_before_the_label() {
    let face: Row<'_> = icon_button(Icon::Save, "Save");
    assert_eq!(children(&face), 2, "the icon and the label");
    let bare: Row<'_> = icon_button(None, "Compare");
    assert_eq!(children(&bare), 0, "the label alone");
    let plain: Row<'_> = secondary_button("Compare", Some(()));
    assert_eq!(face_parts(&plain), 0);
    for button in [
        primary_icon_button(Icon::Confirm, "OK", Some(())),
        secondary_icon_button(Icon::Close, "Cancel", Some(())),
        secondary_icon_button(Some(Icon::Pause), "Pause", None),
    ] {
        assert_eq!(face_parts(&button), 2);
    }
    let row: Row<'_> = button_row(
        Some(primary_icon_button(Icon::Save, "Save", Some(()))),
        [secondary_icon_button(Icon::Close, "Close", Some(()))],
        [secondary_icon_button(Icon::Test, "Test", None)],
    );
    assert_eq!(children(&row), 4);
}

/// The icon has no colour of its own: it draws in the button's text colour, which the theme sets
/// for light and dark and halves for a disabled button.
#[test]
fn the_icons_colour_is_the_buttons_and_dims_when_disabled() {
    for dark in [false, true] {
        let theme = iced_theme(tokens(dark));
        for style in [button::primary, button::secondary] {
            let active = style(&theme, button::Status::Active).text_color;
            let disabled = style(&theme, button::Status::Disabled).text_color;
            assert!(active.a > 0.9, "dark {dark}: {active:?}");
            assert!(disabled.a < active.a * 0.75, "dark {dark}: {disabled:?}");
        }
    }
}

/// The first node under `tree` with more than one child, down the chain of single children: the
/// column inside a card's frame.
fn first_fork(tree: &Tree) -> &Tree {
    let mut node = tree;
    while let [only] = node.children.as_slice() {
        node = only;
    }
    node
}

/// Every card has its head over its content, and the head is the title and the X (RC-080-15).
#[test]
fn every_card_carries_the_title_and_the_close_x_over_its_content() {
    let head = || Head::new(iced::widget::text("Title"), ());
    let content = || iced::widget::text("Body");
    let tokens = tokens(false);
    let cards: [Row<'_>; 4] = [
        dialog_card(head(), content(), 480.0, tokens),
        warning_card(head(), content(), 680.0, true, tokens),
        large_dialog(head(), content(), tokens),
        Head::new(iced::widget::text("Title"), ()).view(tokens),
    ];
    for (index, card) in cards.iter().enumerate() {
        let tree = Tree::new(card.as_widget());
        let fork = first_fork(&tree);
        if index == 3 {
            assert_eq!(fork.children.len(), 2, "the head: the title and the X");
            continue;
        }
        assert_eq!(
            fork.children.len(),
            2,
            "card {index}: the head and the content"
        );
        let head = first_fork(&fork.children[0]);
        assert_eq!(head.children.len(), 2, "card {index}: the title and the X");
    }
}

/// The X has no fill of its own and the inactive cursor's under the pointer, in both looks.
#[test]
fn the_close_x_fills_only_under_the_pointer() {
    for dark in [false, true] {
        let tokens = tokens(dark);
        assert!(
            close_style(tokens, button::Status::Active)
                .background
                .is_none()
        );
        let hovered = close_style(tokens, button::Status::Hovered);
        assert!(hovered.background.is_some(), "dark {dark}");
        assert_eq!(hovered.text_color, tokens.text);
    }
}

/// A label's access key is underlined - three spans of rich text - only while it shows; the
/// label without one, or with the key hidden, is the one text it always was.
#[test]
fn the_access_key_is_underlined_only_while_it_shows() {
    fn tag(label: Row<'_>) -> iced::advanced::widget::tree::Tag {
        Tree::new(label.as_widget()).tag
    }
    let plain = tag(iced::widget::text("Skip").into());
    assert_eq!(tag(marked_label(Label::parse("&Skip"), false)), plain);
    assert_eq!(tag(marked_label(Label::parse("Skip"), true)), plain);
    let underlined = tag(marked_label(Label::parse("&Skip"), true));
    assert_ne!(underlined, plain);
}
