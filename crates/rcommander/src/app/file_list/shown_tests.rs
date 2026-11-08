use rc_engine::{Align, ColumnSetSpec, Field};

use super::{column_of, custom, full};
use crate::app::{
    file_list::columns::{Column, ColumnWidths, split_shown},
    resize::Target,
};

#[test]
fn a_set_shows_the_name_first_and_its_own_widths_titles_and_alignment() {
    let mut spec = ColumnSetSpec::of(&[Field::Size, Field::Name, Field::Comment]);
    spec.columns[0].width = 70;
    spec.columns[1].title = Some("File".into());
    spec.columns[2].align = Align::Right;
    let shown = custom(&spec, Some(7));
    let columns: Vec<Column> = shown.iter().map(|shown| shown.column).collect();
    assert_eq!(columns, [Column::Name, Column::Size, Column::Comment]);
    assert_eq!(shown[0].title.as_deref(), Some("File"));
    assert_eq!(shown[0].width, None, "the name takes the rest");
    assert_eq!(shown[1].width, Some(70.0));
    assert!(shown[1].right, "a size stands right by default");
    assert!(shown[2].right);
    assert_eq!(
        shown[2].grip,
        Some(Target::SetColumn { set: 7, index: 2 }),
        "the grip drags the set's own column"
    );
    let comments = custom(&ColumnSetSpec::comments(), None);
    assert!(comments.iter().all(|shown| shown.grip.is_none()));
}

#[test]
fn the_full_view_keeps_its_settings_and_the_set_splits_the_same_way() {
    let widths = ColumnWidths::default();
    let shown = full(&[Column::Name, Column::Ext, Column::Size], &widths);
    assert_eq!(shown[1].grip, Some(Target::Column(Column::Ext)));
    assert_eq!(shown[1].width, widths.get(Column::Ext));
    let spec = ColumnSetSpec::of(&[Field::Name, Field::Created, Field::Accessed]);
    let set = custom(&spec, Some(1));
    let sizes = split_shown(900.0, &set, 1.0);
    assert_eq!(&sizes[1..], [128.0, 128.0]);
    assert!(sizes[0] > 400.0, "{sizes:?}");
    let narrow = split_shown(260.0, &set, 1.0);
    assert!(narrow[1] < 128.0 && narrow[1] >= 24.0, "{narrow:?}");
}

#[test]
fn every_field_has_its_column() {
    assert_eq!(column_of(Field::Modified), Column::Date);
    assert_eq!(column_of(Field::Owner), Column::User);
    assert_eq!(column_of(Field::Group), Column::Group);
    for field in Field::ALL {
        let _ = column_of(field);
    }
}
