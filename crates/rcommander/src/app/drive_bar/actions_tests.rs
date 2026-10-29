use rc_engine::{Chord, Drive, DriveGroup, DriveKind, DriveState, PanelId};

use super::{ContextKey, DriveAction, DriveContext, context_actions, menu_actions};

fn drive(id: &str, group: DriveGroup, kind: DriveKind) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind,
        label: id.to_owned(),
        root: None,
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: (kind == DriveKind::Network).then(|| r"\\nas\media".to_owned()),
    }
}

#[test]
fn the_menu_offers_mapping_and_disconnecting_while_a_network_drive_is_there() {
    let local = [drive("C:", DriveGroup::Drives, DriveKind::Fixed)];
    assert_eq!(menu_actions(&local, true), [DriveAction::MapNetwork]);
    let mut both = local.to_vec();
    both.push(drive("Z:", DriveGroup::Drives, DriveKind::Network));
    assert_eq!(
        menu_actions(&both, true),
        [DriveAction::MapNetwork, DriveAction::UnmapNetwork]
    );
    assert!(
        menu_actions(&both, false).is_empty(),
        "not on this platform"
    );
}

#[test]
fn a_drive_s_context_menu_disconnects_only_a_network_drive() {
    let share = drive("Z:", DriveGroup::Drives, DriveKind::Network);
    assert_eq!(
        context_actions(&share, true),
        [DriveAction::MapNetwork, DriveAction::UnmapNetwork]
    );
    let fixed = drive("C:", DriveGroup::Drives, DriveKind::Fixed);
    assert_eq!(context_actions(&fixed, true), [DriveAction::MapNetwork]);
    let distro = drive("Ubuntu", DriveGroup::Wsl, DriveKind::Wsl);
    assert!(context_actions(&distro, true).is_empty());
    assert!(context_actions(&share, false).is_empty());
}

#[test]
fn the_context_menu_s_keys_move_run_and_close() {
    let mut menu = DriveContext {
        panel: PanelId::Left,
        drive: "Z:".to_owned(),
        actions: vec![DriveAction::MapNetwork, DriveAction::UnmapNetwork],
        cursor: 0,
    };
    let mut key = |keys: &str| {
        let chord: Chord = keys.parse().expect("a chord");
        menu.key(&chord)
    };
    assert_eq!(key("ArrowUp"), ContextKey::Stay);
    assert_eq!(key("ArrowDown"), ContextKey::Stay);
    assert_eq!(key("ArrowDown"), ContextKey::Stay);
    assert_eq!(key("Enter"), ContextKey::Run(DriveAction::UnmapNetwork));
    assert_eq!(key("Home"), ContextKey::Stay);
    assert_eq!(key("Space"), ContextKey::Run(DriveAction::MapNetwork));
    assert_eq!(key("x"), ContextKey::Stay, "swallowed");
    assert_eq!(key("Escape"), ContextKey::Close);
}

/// RC-080-16: a letter runs the entry starting with it; Shift+F10 closes, as it opened.
#[test]
fn a_letter_runs_its_entry_and_shift_f10_closes() {
    let mut menu = DriveContext {
        panel: PanelId::Left,
        drive: "Z:".to_owned(),
        actions: vec![DriveAction::MapNetwork, DriveAction::UnmapNetwork],
        cursor: 0,
    };
    let mut key = |keys: &str| {
        let chord: Chord = keys.parse().expect("a chord");
        menu.key(&chord)
    };
    assert_eq!(key("d"), ContextKey::Run(DriveAction::UnmapNetwork));
    assert_eq!(key("m"), ContextKey::Run(DriveAction::MapNetwork));
    assert_eq!(key("Shift+F10"), ContextKey::Close);
    assert_eq!(key("ContextMenu"), ContextKey::Close);
}
