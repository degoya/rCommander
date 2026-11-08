use std::collections::HashMap;

use super::{
    BarButton, BarItem, BarSpec, ButtonAction, IconRef, MAX_BUTTONS, MAX_SUB_BAR_DEPTH,
    SPEC_VERSION, check_links, clean_name,
};
use crate::{
    ButtonBarError,
    user_commands::{CommandKind, CommandRef, MAX_FIELD},
};

fn program(command: &str) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef {
            command: command.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    })
}

fn sub_bar(id: i64) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::SubBar(id),
        icon: None,
        tooltip: "More".into(),
    })
}

fn spec(items: Vec<BarItem>) -> BarSpec {
    BarSpec {
        version: SPEC_VERSION,
        items,
    }
}

#[test]
fn a_spec_round_trips_through_its_json() {
    let made = spec(vec![
        BarItem::Button(BarButton {
            action: ButtonAction::Command(CommandRef {
                command: "notepad.exe".into(),
                params: "\"%P%N\"".into(),
                ..CommandRef::default()
            }),
            icon: Some(IconRef {
                path: "C:\\Windows\\notepad.exe".into(),
                index: 0,
            }),
            tooltip: "Notepad".into(),
        }),
        BarItem::Separator,
        sub_bar(3),
    ]);
    let read = BarSpec::from_json(&made.to_json()).expect("reads");
    assert_eq!(read, made);
    assert_eq!(read.sub_bars(), [3]);
}

#[test]
fn blanks_go_and_an_empty_symbol_is_none() {
    let mut item = program("np.exe");
    if let BarItem::Button(button) = &mut item {
        button.tooltip = "  Notepad ".into();
        button.icon = Some(IconRef {
            path: "  ".into(),
            index: 2,
        });
    }
    let checked = spec(vec![item]).checked().expect("fits");
    let BarItem::Button(button) = &checked.items[0] else {
        panic!("a button");
    };
    assert_eq!(button.tooltip, "Notepad");
    assert!(button.icon.is_none());
}

#[test]
fn what_does_not_fit_is_refused() {
    let newer = BarSpec {
        version: SPEC_VERSION + 1,
        items: Vec::new(),
    };
    let too_many = spec(vec![BarItem::Separator; MAX_BUTTONS + 1]);
    let mut control = program("np.exe");
    if let BarItem::Button(button) = &mut control {
        button.tooltip = "a\u{7}b".into();
    }
    let mut long = program("np.exe");
    if let BarItem::Button(button) = &mut long {
        button.icon = Some(IconRef {
            path: "x".repeat(MAX_FIELD + 1),
            index: 0,
        });
    }
    let bad_rows = BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Internal,
            command: "not a row!".into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    });
    for refused in [
        newer,
        too_many,
        spec(vec![control]),
        spec(vec![long]),
        spec(vec![bad_rows]),
        spec(vec![program("  ")]),
    ] {
        let error = refused.checked().expect_err("refused");
        assert!(matches!(error, ButtonBarError::Invalid { .. }), "{error:?}");
    }
    assert!(BarSpec::from_json("{\"items\": 3}").is_err());
    assert!(
        spec(vec![BarItem::Separator; MAX_BUTTONS])
            .checked()
            .is_ok()
    );
}

#[test]
fn a_name_is_trimmed_and_checked() {
    assert_eq!(clean_name("  Tools ").expect("fits"), "Tools");
    for refused in [
        "",
        "   ",
        "a\tb",
        &"x".repeat(rc_db::MAX_BUTTON_BAR_NAME + 1),
    ] {
        assert!(clean_name(refused).is_err(), "{refused:?}");
    }
}

fn links(pairs: &[(i64, &[i64])]) -> HashMap<i64, Vec<i64>> {
    pairs
        .iter()
        .map(|(id, children)| (*id, children.to_vec()))
        .collect()
}

#[test]
fn sub_bars_exist_and_open_no_bar_above_them() {
    assert!(check_links(&links(&[(1, &[2, 3]), (2, &[3]), (3, &[])])).is_ok());
    assert!(matches!(
        check_links(&links(&[(1, &[1])])),
        Err(ButtonBarError::Cycle)
    ));
    assert!(matches!(
        check_links(&links(&[(1, &[2]), (2, &[3]), (3, &[1])])),
        Err(ButtonBarError::Cycle)
    ));
    assert!(matches!(
        check_links(&links(&[(1, &[9])])),
        Err(ButtonBarError::Unknown { id: 9 })
    ));
}

#[test]
fn sub_bars_nest_four_deep_and_no_deeper() {
    let depth = i64::try_from(MAX_SUB_BAR_DEPTH).expect("small");
    let chain = |levels: i64| -> HashMap<i64, Vec<i64>> {
        (0..=levels)
            .map(|id| {
                let children = if id < levels {
                    vec![id + 1]
                } else {
                    Vec::new()
                };
                (id, children)
            })
            .collect()
    };
    assert!(check_links(&chain(depth)).is_ok());
    assert!(matches!(
        check_links(&chain(depth + 1)),
        Err(ButtonBarError::TooDeep)
    ));
}
