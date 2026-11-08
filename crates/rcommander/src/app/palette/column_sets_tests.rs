use rc_engine::{ColumnSet, ColumnSetSpec, Command, Field, ListMode};

use super::add_rows;
use crate::app::palette::Action;

#[test]
fn every_set_is_a_row_that_shows_it_and_the_shown_one_is_marked() {
    let sets = [
        ColumnSet {
            id: 3,
            name: "Photos".into(),
            spec: ColumnSetSpec::of(&[Field::Name]),
        },
        ColumnSet {
            id: 5,
            name: "Code".into(),
            spec: ColumnSetSpec::of(&[Field::Name]),
        },
    ];
    let mut entries = Vec::new();
    add_rows(&mut entries, &sets, ListMode::Custom { set: 3 });
    assert_eq!(entries.len(), 2);
    assert!(entries[0].label.ends_with("Code"), "sorted by label");
    let photos = &entries[1];
    assert!(photos.current);
    assert_eq!(
        photos.action,
        Action::Run(Box::new(Command::SetListMode {
            panel: None,
            mode: ListMode::Custom { set: 3 },
        }))
    );
    assert!(!entries[0].current);
}
