use super::*;
use crate::app::workspaces::NameFor;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// The message a key sends at `focus` with nothing pending, if any.
fn run(focus: Focus, text: &str) -> Option<WsMsg> {
    match key(focus, None, &chord(text), false) {
        KeyAction::Run(message) => Some(message),
        _ => None,
    }
}

/// The filter and the list share the list's keys; Home, End and Delete are the list's, as the
/// filter's text uses them.
#[test]
fn the_list_keys_work_from_the_filter_and_the_list() {
    for focus in [Focus::Filter, Focus::List] {
        assert!(matches!(run(focus, "ArrowDown"), Some(WsMsg::Step(1))));
        assert!(matches!(run(focus, "ArrowUp"), Some(WsMsg::Step(-1))));
        assert!(matches!(run(focus, "PageDown"), Some(WsMsg::Step(PAGE))));
        assert!(matches!(run(focus, "Enter"), Some(WsMsg::Apply)));
        assert!(matches!(run(focus, "Insert"), Some(WsMsg::New)));
        assert!(matches!(run(focus, "F2"), Some(WsMsg::Rename)));
        assert!(matches!(run(focus, "F8"), Some(WsMsg::Delete)));
        assert!(matches!(
            run(focus, "Ctrl+ArrowUp"),
            Some(WsMsg::Move(true))
        ));
        assert!(matches!(
            run(focus, "Ctrl+ArrowDown"),
            Some(WsMsg::Move(false))
        ));
        assert!(matches!(run(focus, "Escape"), Some(WsMsg::Close)));
    }
    assert!(matches!(run(Focus::List, "Delete"), Some(WsMsg::Delete)));
    assert!(matches!(
        run(Focus::List, "Home"),
        Some(WsMsg::Step(isize::MIN))
    ));
    assert!(matches!(
        run(Focus::List, "End"),
        Some(WsMsg::Step(isize::MAX))
    ));
    assert!(run(Focus::Filter, "Delete").is_none());
    assert!(run(Focus::Filter, "Home").is_none());
}

/// On any other stop the keys are the stop's; Escape still closes, Tab still walks.
#[test]
fn a_stop_of_the_form_keeps_its_keys() {
    for key in ["ArrowDown", "Enter", "Insert", "F8", "Delete"] {
        assert!(run(Focus::Ring, key).is_none(), "{key}");
    }
    assert!(matches!(run(Focus::Ring, "Escape"), Some(WsMsg::Close)));
    assert!(matches!(
        key(Focus::Ring, None, &chord("Shift+Tab"), false),
        KeyAction::Walk(true)
    ));
    assert!(matches!(
        key(Focus::Filter, None, &chord("Tab"), false),
        KeyAction::Walk(false)
    ));
}

/// A key a field or a stop used is not the list's; an access key makes the keys follow the ring.
#[test]
fn a_captured_key_is_the_fields() {
    assert!(matches!(
        key(Focus::Filter, None, &chord("Enter"), true),
        KeyAction::Nothing
    ));
    assert!(matches!(
        key(Focus::Filter, None, &chord("Alt+s"), true),
        KeyAction::Follow
    ));
}

/// The delete question answers Enter and y, Escape and n; a name being typed takes Escape.
#[test]
fn a_question_takes_its_keys_first() {
    let delete = Pending::Delete(4);
    let answer = |text: &str| match key(Focus::List, Some(&delete), &chord(text), false) {
        KeyAction::Run(WsMsg::Confirm(yes)) => Some(yes),
        _ => None,
    };
    assert_eq!(answer("Enter"), Some(true));
    assert_eq!(answer("y"), Some(true));
    assert_eq!(answer("Escape"), Some(false));
    assert_eq!(answer("n"), Some(false));
    assert_eq!(answer("ArrowDown"), None);
    let naming = Pending::Name {
        target: NameFor::New,
        text: String::new(),
    };
    assert!(matches!(
        key(Focus::List, Some(&naming), &chord("Escape"), false),
        KeyAction::Run(WsMsg::Cancel)
    ));
    assert!(matches!(
        key(Focus::List, Some(&naming), &chord("ArrowDown"), false),
        KeyAction::Nothing
    ));
}

/// The keys follow the ring: the filter, the list, any other stop.
#[test]
fn the_keys_follow_the_ring() {
    assert_eq!(focus_of(Some(&Id::new(FILTER_FIELD))), Focus::Filter);
    assert_eq!(focus_of(Some(&Id::new(LIST_STOP_ID))), Focus::List);
    assert_eq!(
        focus_of(Some(&Id::new("rcommander.elsewhere"))),
        Focus::Ring
    );
    assert_eq!(focus_of(None), Focus::Ring);
}
