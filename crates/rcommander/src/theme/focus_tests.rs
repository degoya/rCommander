use iced::{
    Color, Element, Point, Rectangle, Size,
    advanced::widget::{Tree, operation::Focusable},
    keyboard::{Key, Modifiers, key::Named},
    widget::text,
};

use super::{Kind, RING_GAP, RING_WIDTH, State, Stop, button, choice, neighbours, ring, toggle};

type Member<'a> = Element<'a, u8>;

fn named(key: Named) -> Key {
    Key::Named(key)
}

fn stop<'a>(kind: Kind, label: &str, press: Option<u8>) -> Stop<'a, u8> {
    Stop::new(text("label"), kind, label, press, Some(1), Some(2))
}

#[test]
fn a_button_takes_enter_and_space_a_toggle_space_only() {
    let plain = Modifiers::default();
    let ok = stop(Kind::Button, "OK", Some(7));
    assert_eq!(ok.action(&named(Named::Enter), plain), Some(7));
    assert_eq!(ok.action(&named(Named::Space), plain), Some(7));
    assert_eq!(
        ok.action(&named(Named::ArrowDown), plain),
        None,
        "a button has no arrows"
    );
    let check = stop(Kind::Toggle, "Hidden files", Some(9));
    assert_eq!(check.action(&named(Named::Space), plain), Some(9));
    assert_eq!(
        check.action(&named(Named::Enter), plain),
        None,
        "Enter on a checkbox is the dialog's default button"
    );
    assert_eq!(ok.action(&named(Named::Enter), Modifiers::CTRL), None);
    assert_eq!(ok.action(&named(Named::Space), Modifiers::ALT), None);
    assert_eq!(
        ok.action(&named(Named::Escape), plain),
        None,
        "Escape is the dialog's"
    );
    assert_eq!(
        ok.action(&named(Named::Tab), plain),
        None,
        "Tab walks the ring"
    );
}

#[test]
fn a_choice_takes_the_arrows_and_leaves_enter_and_space() {
    let plain = Modifiers::default();
    let list = stop(Kind::Choice, "Encoding", None);
    assert_eq!(list.action(&named(Named::ArrowUp), plain), Some(1));
    assert_eq!(list.action(&named(Named::ArrowLeft), plain), Some(1));
    assert_eq!(list.action(&named(Named::ArrowDown), plain), Some(2));
    assert_eq!(list.action(&named(Named::ArrowRight), plain), Some(2));
    assert_eq!(list.action(&named(Named::Enter), plain), None);
    assert_eq!(list.action(&named(Named::Space), plain), None);
}

#[test]
fn the_neighbours_of_a_choice_stop_at_the_ends() {
    let options = ["a", "b", "c"];
    assert_eq!(neighbours(&options, Some(&"b")), (Some(&"a"), Some(&"c")));
    assert_eq!(neighbours(&options, Some(&"a")), (None, Some(&"b")));
    assert_eq!(neighbours(&options, Some(&"c")), (Some(&"b"), None));
    assert_eq!(
        neighbours(&options, None),
        (None, Some(&"a")),
        "none chosen: the first"
    );
    assert_eq!(neighbours::<&str>(&[], None), (None, None));
}

fn state(tree: &mut Tree) -> &mut State {
    tree.state.downcast_mut::<State>()
}

/// A stop keeps its ring while the view is rebuilt, and loses it to another control at its
/// place or when it is disabled.
#[test]
fn the_ring_stays_with_its_control_only() {
    let first: Member<'_> = button(text("OK"), "OK", Some(1));
    let mut tree = Tree::new(first.as_widget());
    state(&mut tree).focus();
    let again: Member<'_> = button(text("OK"), "OK", Some(1));
    tree.diff(again.as_widget());
    assert!(state(&mut tree).is_focused(), "the same button");
    let other: Member<'_> = button(text("Delete"), "Delete", Some(2));
    tree.diff(other.as_widget());
    assert!(
        !state(&mut tree).is_focused(),
        "another button at its place"
    );
    state(&mut tree).focus();
    let disabled: Member<'_> = button(text("Delete"), "Delete", None);
    tree.diff(disabled.as_widget());
    assert!(!state(&mut tree).is_focused(), "disabled");
}

#[test]
fn each_helper_makes_its_kind_of_stop() {
    let stops: [(Member<'_>, Kind); 3] = [
        (button(text("OK"), "OK", Some(1)), Kind::Button),
        (toggle(text("Hidden"), "Hidden", Some(1)), Kind::Toggle),
        (choice(text("Sort"), "Sort", None, None), Kind::Choice),
    ];
    for (stop, kind) in stops {
        let mut tree = Tree::new(stop.as_widget());
        assert_eq!(state(&mut tree).kind(), kind);
        assert_eq!(
            tree.children.len(),
            1,
            "the control is the stop's one child"
        );
    }
}

#[test]
fn the_ring_stands_round_the_control() {
    let bounds = Rectangle::new(Point::new(10.0, 20.0), Size::new(80.0, 28.0));
    let quad = ring(bounds, Color::BLACK);
    assert_eq!(quad.bounds, bounds.expand(RING_GAP));
    assert_eq!(quad.border.width, RING_WIDTH);
    assert_eq!(quad.border.color, Color::BLACK);
}
