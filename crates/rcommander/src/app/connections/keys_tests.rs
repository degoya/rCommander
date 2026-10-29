use iced::widget::Id;

use super::{Focus, cycle};
use crate::app::connections::{
    ConnectionManager,
    form::{Field, Form},
    view::{FILTER_FIELD, TREE_STOP_ID},
};

#[test]
fn a_choice_wraps_round_both_ways() {
    let all = [1, 2, 3];
    assert_eq!(cycle(&all, 3, 1), 1);
    assert_eq!(cycle(&all, 1, -1), 3);
    assert_eq!(cycle(&all, 2, 1), 3);
    assert_eq!(cycle(&all, 9, 1), 1, "an unknown value starts at the first");
}

/// After Tab the keys go where the focus ring went (RC-080-16): the filter, the tree, a field of
/// the form by its id, and any other stop - a button - keeps its keys to itself.
#[test]
fn the_keys_follow_the_ring() {
    let mut manager = ConnectionManager::default();
    manager.ring_moved(Some(&Id::new(TREE_STOP_ID)));
    assert_eq!(manager.focus, Focus::Tree);
    manager.ring_moved(Some(&Id::new(FILTER_FIELD)));
    assert_eq!(manager.focus, Focus::Filter);
    manager.ring_moved(None);
    assert_eq!(manager.focus, Focus::Ring, "a button");
    manager.form = Some(Form::new(None));
    for field in [Field::Name, Field::Kind, Field::Host] {
        manager.ring_moved(Some(&Id::new(field.id())));
        assert_eq!(manager.focus, Focus::Field(field));
    }
    manager.ring_moved(Some(&Id::new("rcommander.somewhere.else")));
    assert_eq!(manager.focus, Focus::Ring);
}

/// Every control of the form has an id of its own, so the ring tells them apart.
#[test]
fn every_field_has_its_own_id() {
    let form = Form::new(None);
    let ids: Vec<&str> = form.fields().into_iter().map(Field::id).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "{ids:?}");
}

/// An access key moves the ring as Tab does, so the keys ask where it went after one: a letter
/// with Alt from anywhere, alone only from a button or a switch - in a field the letter is typed.
#[test]
fn the_keys_follow_the_ring_after_an_access_key() {
    let chord = |text: &str| -> rc_engine::Chord { text.parse().expect("chord") };
    let mut manager = ConnectionManager {
        focus: Focus::Filter,
        ..ConnectionManager::default()
    };
    assert!(manager.access_key(&chord("Alt+t")));
    assert!(!manager.access_key(&chord("t")), "the filter typed it");
    assert!(!manager.access_key(&chord("Ctrl+Alt+t")), "AltGr types");
    manager.focus = Focus::Ring;
    assert!(manager.access_key(&chord("t")));
    assert!(!manager.access_key(&chord("Enter")));
    assert!(!manager.access_key(&chord("Alt+F4")));
    manager.focus = Focus::Field(Field::Favourite);
    assert!(manager.access_key(&chord("3")));
}
