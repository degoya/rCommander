use iced::{
    Element,
    advanced::widget::{Tree, tree::Tag},
    widget::{text, text_input},
};

use super::{caption, check, keyed, marked_icon_button};
use crate::theme::{
    focus::{Kind, State, Stop},
    icon::Icon,
};

type Member<'a> = Element<'a, bool>;

fn stop_state(element: &Member<'_>) -> State {
    let tree = Tree::new(element.as_widget());
    assert_eq!(tree.tag, Tag::of::<State>(), "a stop of the ring");
    *tree.state.downcast_ref::<State>()
}

#[test]
fn a_checkbox_is_a_toggle_stop_with_its_labels_key() {
    let hidden: Member<'_> = check("&Hidden", false, true, |on| on);
    let state = stop_state(&hidden);
    assert_eq!(state.kind(), Kind::Toggle);
    assert_eq!(state.letter(), Some('h'));
    let plain: Member<'_> = check("Hidden", true, false, |on| on);
    assert_eq!(stop_state(&plain).letter(), None);
}

/// While the key does not show, the checkbox draws its own label, without the mark.
#[test]
fn a_checkbox_draws_its_label_without_the_mark() {
    let hidden: Member<'_> = check("&Hidden", false, true, |on| on);
    let tree = Tree::new(hidden.as_widget());
    assert!(
        tree.children[0].children.is_empty(),
        "one checkbox with its own label, no row of box and text"
    );
}

#[test]
fn a_caption_reads_as_one_text_while_its_key_does_not_show() {
    let plain = Tree::new(Member::from(text("Name")).as_widget()).tag;
    let marked: Member<'_> = caption("&Name", None);
    assert_eq!(Tree::new(marked.as_widget()).tag, plain);
}

#[test]
fn a_field_and_a_button_take_the_key_their_label_marks() {
    let field: Member<'_> = keyed(
        Stop::field(text_input("", "").on_input(|_| true), "&Name"),
        "&Name",
    );
    let state = stop_state(&field);
    assert_eq!(state.kind(), Kind::Field);
    assert_eq!(state.letter(), Some('n'));
    let copy: Member<'_> = keyed(
        Stop::button(
            iced::widget::button(marked_icon_button(Icon::Copy, "&Copy")),
            "&Copy",
            Some(true),
        ),
        "&Copy",
    );
    assert_eq!(stop_state(&copy).letter(), Some('c'));
}
