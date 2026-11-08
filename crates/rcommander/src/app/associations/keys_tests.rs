use rc_engine::Chord;

use super::{Focus, KeyAction, key};
use crate::app::associations::AssocMsg;

fn run(focus: Focus, keys: &str) -> Option<String> {
    let chord: Chord = keys.parse().expect("a chord");
    match key(focus, &chord, false) {
        KeyAction::Run(message) => Some(format!("{message:?}")),
        KeyAction::Walk(back) => Some(format!("Walk({back})")),
        KeyAction::Follow => Some("Follow".into()),
        KeyAction::Nothing => None,
    }
}

fn message(message: AssocMsg) -> Option<String> {
    Some(format!("{message:?}"))
}

#[test]
fn the_list_moves_the_cursor_and_the_rules() {
    assert_eq!(run(Focus::List, "ArrowDown"), message(AssocMsg::Step(1)));
    assert_eq!(
        run(Focus::List, "Home"),
        message(AssocMsg::Step(isize::MIN))
    );
    assert_eq!(
        run(Focus::List, "Ctrl+ArrowUp"),
        message(AssocMsg::Move(true))
    );
    assert_eq!(
        run(Focus::Filter, "Ctrl+ArrowDown"),
        message(AssocMsg::Move(false))
    );
    assert_eq!(run(Focus::List, "Insert"), message(AssocMsg::New));
    assert_eq!(run(Focus::List, "Delete"), message(AssocMsg::Delete));
    assert_eq!(run(Focus::Filter, "Delete"), None, "the filter's own");
    assert_eq!(run(Focus::Filter, "Enter"), message(AssocMsg::Save));
}

#[test]
fn escape_closes_and_tab_walks_from_every_stop() {
    for focus in [Focus::Filter, Focus::List, Focus::Ring] {
        assert_eq!(run(focus, "Escape"), message(AssocMsg::Close), "{focus:?}");
        assert_eq!(run(focus, "Tab"), Some("Walk(false)".into()), "{focus:?}");
    }
    assert_eq!(run(Focus::Ring, "ArrowDown"), None, "a field's own");
    let chord: Chord = "ArrowDown".parse().expect("a chord");
    assert!(matches!(key(Focus::List, &chord, true), KeyAction::Nothing));
}
