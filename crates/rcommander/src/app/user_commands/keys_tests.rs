use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// The message a key sends on `tab` at `focus` with nothing pending, if any.
fn run(tab: Tab, focus: Focus, text: &str) -> Option<UcMsg> {
    match key((tab, focus), None, &chord(text), false) {
        KeyAction::Run(message) => Some(message),
        _ => None,
    }
}

#[test]
fn the_list_keys_work_from_the_filter_and_the_list() {
    for focus in [Focus::Filter, Focus::List] {
        assert!(matches!(
            run(Tab::Own, focus, "ArrowDown"),
            Some(UcMsg::Step(1))
        ));
        assert!(matches!(run(Tab::Own, focus, "PageUp"), Some(UcMsg::Step(step)) if step == -PAGE));
        assert!(matches!(run(Tab::Own, focus, "Enter"), Some(UcMsg::Save)));
        assert!(matches!(run(Tab::Own, focus, "Insert"), Some(UcMsg::New)));
        assert!(matches!(run(Tab::Own, focus, "F8"), Some(UcMsg::Delete)));
        assert!(matches!(run(Tab::Own, focus, "Escape"), Some(UcMsg::Close)));
    }
    assert!(matches!(
        run(Tab::Own, Focus::List, "Delete"),
        Some(UcMsg::Delete)
    ));
    assert!(
        run(Tab::Own, Focus::Filter, "Delete").is_none(),
        "the filter's own"
    );
    assert!(
        run(Tab::Own, Focus::Filter, "Home").is_none(),
        "the filter's own"
    );
    assert!(
        run(Tab::Own, Focus::Ring, "ArrowDown").is_none(),
        "the control's own"
    );
    assert!(matches!(
        run(Tab::Own, Focus::Ring, "Escape"),
        Some(UcMsg::Close)
    ));
}

#[test]
fn only_the_tree_moves_its_rows() {
    let moves = [
        ("Ctrl+ArrowUp", Move::Up),
        ("Ctrl+ArrowDown", Move::Down),
        ("Ctrl+ArrowRight", Move::Into),
        ("Ctrl+ArrowLeft", Move::Out),
    ];
    for (text, to) in moves {
        assert!(
            matches!(run(Tab::Start, Focus::List, text), Some(UcMsg::Move(moved)) if moved == to),
            "{text}"
        );
        assert!(run(Tab::Own, Focus::List, text).is_none(), "{text}");
    }
}

#[test]
fn ctrl_tab_turns_to_the_other_tab_from_every_stop() {
    for focus in [Focus::Filter, Focus::List, Focus::Ring] {
        assert!(matches!(
            run(Tab::Own, focus, "Ctrl+Tab"),
            Some(UcMsg::Tab(Tab::Start))
        ));
        assert!(matches!(
            run(Tab::Start, focus, "Ctrl+Shift+Tab"),
            Some(UcMsg::Tab(Tab::Own))
        ));
    }
    assert!(matches!(
        key((Tab::Own, Focus::Filter), None, &chord("Tab"), false),
        KeyAction::Walk(false)
    ));
}

#[test]
fn the_delete_question_takes_its_keys_first() {
    let pending = Pending::Delete(3);
    let answer =
        |text: &str| match key((Tab::Own, Focus::List), Some(&pending), &chord(text), false) {
            KeyAction::Run(UcMsg::Confirm(yes)) => Some(yes),
            _ => None,
        };
    assert_eq!(answer("Enter"), Some(true));
    assert_eq!(answer("y"), Some(true));
    assert_eq!(answer("Escape"), Some(false));
    assert_eq!(answer("Insert"), None);
    assert!(matches!(
        key(
            (Tab::Own, Focus::List),
            Some(&pending),
            &chord("Enter"),
            true
        ),
        KeyAction::Nothing
    ));
}

#[test]
fn the_keys_follow_the_ring() {
    assert_eq!(focus_of(Some(&Id::new(FILTER_FIELD))), Focus::Filter);
    assert_eq!(focus_of(Some(&Id::new(LIST_STOP_ID))), Focus::List);
    assert_eq!(focus_of(Some(&Id::new(TREE_STOP_ID))), Focus::List);
    assert_eq!(focus_of(None), Focus::Ring);
    assert_eq!(Focus::first(Tab::Own), Focus::Filter);
    assert_eq!(Focus::first(Tab::Start), Focus::List);
}
