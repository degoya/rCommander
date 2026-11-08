use rc_engine::{ColumnSet, ColumnSetSpec, Field, ListMode, PanelId};

use crate::app::tabs::popup::{Popup, PopupAction};

fn set(id: i64, name: &str) -> ColumnSet {
    ColumnSet {
        id,
        name: name.into(),
        spec: ColumnSetSpec::of(&[Field::Name, Field::Comment]),
    }
}

#[test]
fn the_views_list_the_sets_in_order_and_mark_the_one_shown() {
    let sets = [set(4, "Photos"), set(2, "Code")];
    let popup = Popup::views(PanelId::Left, &sets, ListMode::Custom { set: 2 });
    let actions: Vec<Option<PopupAction>> =
        popup.rows.iter().map(|row| row.action.clone()).collect();
    assert_eq!(
        actions,
        [
            Some(PopupAction::View(ListMode::Full)),
            Some(PopupAction::View(ListMode::Brief)),
            Some(PopupAction::View(ListMode::Comments)),
            Some(PopupAction::View(ListMode::Custom { set: 4 })),
            Some(PopupAction::View(ListMode::Custom { set: 2 })),
            Some(PopupAction::ConfigureColumns),
        ]
    );
    assert_eq!(popup.rows[3].label, "Photos");
    assert_eq!(popup.cursor, 4, "on the set shown");
    assert!(popup.rows[4].current);
    let full = Popup::views(PanelId::Right, &[], ListMode::Full);
    assert_eq!(full.cursor, 0);
    assert_eq!(full.rows.len(), 4);
}
