use rc_engine::BarPlace;

use super::{bar_id, item_id, more_id, visible_count};

#[test]
fn as_many_items_as_fit_stay_and_the_rest_goes_behind_more() {
    let sizes = [30.0, 30.0, 30.0];
    assert_eq!(visible_count(&sizes, 100.0, 20.0), 3, "all fit");
    // 30 + 3 + 30 = 63 leaves no room for a third and the 20 of `»`.
    assert_eq!(visible_count(&sizes, 90.0, 20.0), 2);
    assert_eq!(visible_count(&sizes, 10.0, 20.0), 0);
    assert_eq!(visible_count(&[], 10.0, 20.0), 0);
}

#[test]
fn every_item_has_an_id_of_its_own() {
    let ids = [
        bar_id(BarPlace::Horizontal),
        bar_id(BarPlace::Vertical),
        item_id(BarPlace::Horizontal, 0),
        item_id(BarPlace::Horizontal, 1),
        item_id(BarPlace::Vertical, 0),
        more_id(BarPlace::Horizontal),
        more_id(BarPlace::Vertical),
    ];
    for (at, id) in ids.iter().enumerate() {
        for other in &ids[at + 1..] {
            assert_ne!(id, other);
        }
    }
}
