use rc_engine::SortKey;

use super::{Column, ColumnWidths, NAME_MIN, SPACING, brief_width, columns, name_room, split};

/// The name's own width in a list `width` wide.
fn name(width: f32, columns: &[Column], widths: &ColumnWidths, icon: bool, scale: f32) -> f32 {
    name_room(split(width, columns, widths, scale)[0], icon)
}

fn default(column: Column) -> f32 {
    column.bounds().2
}

#[test]
fn the_name_takes_what_the_fixed_columns_leave() {
    let plain = columns(false, false, false);
    assert_eq!(
        plain,
        [Column::Name, Column::Ext, Column::Size, Column::Date]
    );
    let widths = ColumnWidths::default();
    let wide = name(800.0, &plain, &widths, false, 1.0);
    let narrow = name(500.0, &plain, &widths, false, 1.0);
    assert_eq!(wide - narrow, 300.0, "only the name grows with the panel");
    let fixed = default(Column::Ext) + default(Column::Size) + default(Column::Date);
    assert!(wide < 800.0 - fixed, "{wide}");
    assert!(wide > 800.0 - fixed - 80.0, "{wide}");
    assert_eq!(
        name(800.0, &plain, &widths, false, 1.0) - name(800.0, &plain, &widths, true, 1.0),
        22.0,
        "the icon and its gap"
    );
    let sizes = split(800.0, &plain, &widths, 1.0);
    assert_eq!(&sizes[1..], [56.0, 96.0, 128.0], "the defaults");
}

#[test]
fn an_archive_and_a_server_take_their_columns_from_the_name() {
    let widths = ColumnWidths::default();
    let plain = name(800.0, &columns(false, false, false), &widths, false, 1.0);
    let archive = columns(true, false, false);
    assert_eq!(archive[3], Column::Packed);
    assert_eq!(
        plain - name(800.0, &archive, &widths, false, 1.0),
        default(Column::Packed) + SPACING
    );
    let server = columns(false, true, false);
    assert_eq!(server.last(), Some(&Column::Rights));
    assert!(name(800.0, &server, &widths, false, 1.0) < plain);
    // RC-060-02: the owner, where switched on, after the rights.
    let owned = columns(false, true, true);
    assert_eq!(owned.last(), Some(&Column::Owner));
    assert_eq!(
        name(800.0, &server, &widths, false, 1.0) - name(800.0, &owned, &widths, false, 1.0),
        default(Column::Owner) + SPACING
    );
    assert!(!widths.owner, "the owner column is off by default");
}

#[test]
fn a_panel_too_narrow_or_not_measured_leaves_the_name_nothing() {
    let all = columns(true, true, false);
    let widths = ColumnWidths::default();
    assert_eq!(name(0.0, &all, &widths, true, 1.0), 0.0);
    let unmeasured = split(0.0, &all, &widths, 1.0);
    assert_eq!(
        &unmeasured[1..],
        [56.0, 96.0, 96.0, 128.0, 84.0],
        "untouched"
    );
    assert_eq!(name(200.0, &all, &widths, true, 1.0), 0.0);
    let least: Vec<f32> = all.iter().map(|column| column.bounds().0).collect();
    assert_eq!(
        split(200.0, &all, &widths, 1.0)[1..],
        least[1..],
        "the fixed columns gave way down to their least widths"
    );
    assert_eq!(brief_width(0.0, 3, true), 0.0);
    assert!(brief_width(600.0, 3, false) > brief_width(600.0, 3, true));
    assert!(brief_width(600.0, 2, false) > brief_width(600.0, 3, false));
}

/// RC-010-20: a narrow panel keeps the name its least width; the fixed columns give way in
/// proportion to what they have above their own.
#[test]
fn in_a_narrow_panel_the_fixed_columns_give_way_to_the_name() {
    let plain = columns(false, false, false);
    let widths = ColumnWidths::default();
    // 400 wide: 339 for the columns, 280 fixed, 21 short of the name's 80.
    let sizes = split(400.0, &plain, &widths, 1.0);
    assert_eq!(sizes, [NAME_MIN, 52.0, 89.0, 118.0]);
    let roomy = split(421.0, &plain, &widths, 1.0);
    assert_eq!(roomy, [NAME_MIN, 56.0, 96.0, 128.0], "just enough room");
}

/// RC-010-20: dragged widths hold within their rows' bounds and take their room from the name.
#[test]
fn dragged_widths_hold_their_bounds_and_take_from_the_name() {
    let plain = columns(false, false, false);
    let mut widths = ColumnWidths::default();
    let before = name(800.0, &plain, &widths, false, 1.0);
    widths.set(Column::Date, 200.0);
    assert_eq!(widths.get(Column::Date), Some(200.0));
    assert_eq!(before - name(800.0, &plain, &widths, false, 1.0), 72.0);
    assert_eq!(
        before - name(800.0, &plain, &widths, false, 1.5),
        (56.0 + 96.0 + 200.0) * 0.5 + 72.0,
        "scaled with the font"
    );
    widths.set(Column::Ext, 1.0);
    assert_eq!(widths.get(Column::Ext), Some(24.0), "the least width");
    widths.set(Column::Size, 10_000.0);
    assert_eq!(widths.get(Column::Size), Some(640.0), "the largest");
    widths.set(Column::Size, f32::NAN);
    assert_eq!(
        widths.get(Column::Size),
        Some(640.0),
        "no number is ignored"
    );
    widths.set(Column::Name, 300.0);
    assert_eq!(
        widths.get(Column::Name),
        None,
        "the name has no width of its own"
    );
}

#[test]
fn every_fixed_column_has_its_setting_row() {
    let widths = ColumnWidths::default();
    for column in Column::FIXED {
        let (min, max, default) = column.bounds();
        assert!(column.setting().is_some(), "{column:?}");
        assert!(0.0 < min && min < default && default < max, "{column:?}");
        assert_eq!(widths.get(column), Some(default), "{column:?}");
    }
    assert_eq!(Column::Name.setting(), None);
}

#[test]
fn every_title_but_packed_size_rights_and_owner_sorts_by_its_column() {
    let keys: Vec<Option<SortKey>> = columns(true, true, true)
        .into_iter()
        .map(Column::sort_key)
        .collect();
    assert_eq!(
        keys,
        [
            Some(SortKey::Name),
            Some(SortKey::Ext),
            Some(SortKey::Size),
            None,
            Some(SortKey::Date),
            None,
            None,
        ]
    );
    let widths = ColumnWidths::default();
    assert_eq!(
        Column::Name.width(&widths, 1.0),
        None,
        "the name fills its row"
    );
}

#[test]
fn a_larger_list_font_widens_the_fixed_columns_and_narrows_the_name() {
    let plain = columns(false, false, false);
    let widths = ColumnWidths::default();
    assert_eq!(Column::Date.width(&widths, 1.5), Some(128.0 * 1.5));
    let fixed = 56.0 + 96.0 + 128.0;
    let at_default = name(800.0, &plain, &widths, false, 1.0);
    let larger = name(800.0, &plain, &widths, false, 1.5);
    assert_eq!(at_default - larger, fixed * 0.5);
}
