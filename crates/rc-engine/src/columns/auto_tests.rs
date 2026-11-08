use super::*;
use crate::columns::{ColumnSetSpec, Field};

fn set(id: ColumnSetId, masks: &[&str]) -> ColumnSet {
    let mut spec = ColumnSetSpec::of(&[Field::Name, Field::Size]);
    spec.masks = masks.iter().map(|mask| (*mask).to_owned()).collect();
    ColumnSet {
        id,
        name: format!("set {id}"),
        spec,
    }
}

#[test]
fn a_mask_holds_for_the_folder_and_below_it_without_regard_to_case() {
    assert!(mask_matches(r"D:\Fotos\*", r"D:\Fotos"));
    assert!(mask_matches(r"D:\Fotos\*", r"d:\fotos\2024\summer"));
    assert!(!mask_matches(r"D:\Fotos\*", r"D:\Fotosammlung"));
    assert!(mask_matches("sftp://*", "sftp://web/var/www"));
    assert!(!mask_matches("sftp://*", "ftp://web/var/www"));
    assert!(mask_matches(r"*.zip\*", r"C:\data\backup.zip\docs"));
    assert!(mask_matches(r"*.zip\*", r"C:\data\backup.zip"));
    assert!(mask_matches("/home/?ob/*", "/home/bob/src"));
    assert!(!mask_matches("  ", "/home"));
}

#[test]
fn the_first_set_in_order_wins() {
    let sets = [
        set(7, &["/srv/*"]),
        set(3, &["/home/*", "/srv/www/*"]),
        set(5, &["*"]),
    ];
    assert_eq!(matching_set(&sets, "/srv/www/site"), Some(7));
    assert_eq!(matching_set(&sets, "/home/bob"), Some(3));
    assert_eq!(matching_set(&sets, "/tmp"), Some(5));
    assert_eq!(matching_set(&sets[..2], "/tmp"), None);
}

#[test]
fn a_matching_mask_shows_its_set_and_ends_a_mode_by_hand_of_another_rule() {
    let by_hand = PanelColumns {
        manual: true,
        rule: Some(1),
        ..PanelColumns::default()
    };
    let (mode, columns) = switched(ListMode::Brief, &by_hand, Some(2), AutoDefault::Unchanged);
    assert_eq!(mode, ListMode::Custom { set: 2 });
    assert!(!columns.manual);
    assert_eq!(columns.rule, Some(2));
}

#[test]
fn a_mode_by_hand_stays_until_a_folder_with_another_set() {
    let by_hand = PanelColumns {
        manual: true,
        rule: Some(1),
        ..PanelColumns::default()
    };
    // Below the same rule's folder: the mode chosen by hand stays.
    let (mode, columns) = switched(ListMode::Full, &by_hand, Some(1), AutoDefault::Full);
    assert_eq!(mode, ListMode::Full);
    assert!(columns.manual);
    // Out of it, where no rule holds: it stays, even with a default.
    let (mode, columns) = switched(ListMode::Brief, &by_hand, None, AutoDefault::Full);
    assert_eq!(mode, ListMode::Brief);
    assert!(columns.manual);
    assert_eq!(columns.rule, None);
    // Back into the rule's folder from one without: another rule than the last, so it switches.
    let (mode, columns) = switched(ListMode::Brief, &columns, Some(1), AutoDefault::Full);
    assert_eq!(mode, ListMode::Custom { set: 1 });
    assert!(!columns.manual);
}

#[test]
fn without_a_match_the_default_decides() {
    let auto = PanelColumns {
        rule: Some(4),
        ..PanelColumns::default()
    };
    let custom = ListMode::Custom { set: 4 };
    for (default, wanted) in [
        (AutoDefault::Unchanged, custom),
        (AutoDefault::Full, ListMode::Full),
        (AutoDefault::Brief, ListMode::Brief),
    ] {
        let (mode, columns) = switched(custom, &auto, None, default);
        assert_eq!(mode, wanted, "{default:?}");
        assert_eq!(columns.rule, None);
    }
    assert_eq!(AutoDefault::from_setting("full"), AutoDefault::Full);
    assert_eq!(AutoDefault::from_setting("brief"), AutoDefault::Brief);
    assert_eq!(AutoDefault::from_setting("x"), AutoDefault::Unchanged);
}

#[test]
fn a_deleted_set_shows_the_full_view_and_the_switch_off_changes_nothing_else() {
    let path = rc_vfs::VfsPath::local("/tmp");
    let mut panel = PanelState::showing(path, Vec::new());
    panel.mode = ListMode::Custom { set: 9 };
    let rule = FolderRule {
        known: Some(vec![1, 2]),
        enabled: false,
        default: AutoDefault::Brief,
        matched: Some(1),
    };
    rule.apply(&mut panel);
    assert_eq!(panel.mode, ListMode::Full, "set 9 was deleted");

    panel.mode = ListMode::Custom { set: 2 };
    rule.apply(&mut panel);
    assert_eq!(panel.mode, ListMode::Custom { set: 2 }, "the switch is off");

    let unread = FolderRule {
        known: None,
        ..rule
    };
    panel.mode = ListMode::Custom { set: 9 };
    unread.apply(&mut panel);
    assert_eq!(
        panel.mode,
        ListMode::Custom { set: 9 },
        "unread sets leave it"
    );
}
