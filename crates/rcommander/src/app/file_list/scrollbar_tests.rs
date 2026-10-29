use super::{Extent, Hit, MIN_THUMB};

/// The full view of 100 entries and `..`, ten rows on screen, from row `top`.
fn full(top: usize) -> Extent {
    Extent {
        top,
        page: 10,
        total: 101,
        max_top: 91,
        step: 1,
    }
}

/// The brief view of 95 entries and `..`: three columns of ten rows, from row `top`.
fn brief(top: usize) -> Extent {
    Extent {
        top,
        page: 30,
        total: 96,
        max_top: 70,
        step: 10,
    }
}

/// The room the thumb's upper edge moves in, in a track `track` long.
fn room(extent: Extent, track: f32) -> f32 {
    track - extent.thumb(track).1
}

#[test]
fn the_thumb_is_a_screens_share_of_the_track_where_the_view_stands() {
    let (offset, length) = full(0).thumb(200.0);
    assert_eq!(offset, 0.0);
    assert!((length - 200.0 * 10.0 / 101.0).abs() < 0.01, "{length}");
    let (offset, length) = full(91).thumb(200.0);
    assert!(
        (offset + length - 200.0).abs() < 0.01,
        "the last page ends the track"
    );
    let huge = Extent {
        top: 0,
        page: 40,
        total: 100_001,
        max_top: 99_961,
        step: 1,
    };
    assert_eq!(
        huge.thumb(200.0).1,
        MIN_THUMB,
        "a long list's thumb keeps a size"
    );
}

#[test]
fn the_pointer_maps_to_the_first_row_from_start_to_end() {
    let extent = full(0);
    let room = room(extent, 200.0);
    assert_eq!(extent.top_at(200.0, 0.0), 0, "the start");
    assert_eq!(extent.top_at(200.0, room), 91, "the end");
    assert_eq!(
        extent.top_at(200.0, room / 4.0),
        23,
        "a quarter: 22.75 rounded"
    );
    assert_eq!(extent.top_at(200.0, -50.0), 0, "above the bar");
    assert_eq!(extent.top_at(200.0, room + 500.0), 91, "below the bar");
    assert_eq!(extent.top_at(200.0, f32::NAN), 0);
}

#[test]
fn a_dragged_thumb_lands_on_every_row_it_is_drawn_at() {
    for top in 0..=91 {
        let extent = full(top);
        let (offset, _) = extent.thumb(2_000.0);
        assert_eq!(extent.top_at(2_000.0, offset), top);
    }
}

#[test]
fn a_huge_list_maps_the_pointer_in_proportion() {
    let huge = Extent {
        top: 0,
        page: 40,
        total: 100_001,
        max_top: 99_961,
        step: 1,
    };
    let room = room(huge, 200.0);
    assert_eq!(room, 184.0);
    assert_eq!(huge.top_at(200.0, 46.0), 24_990);
    assert_eq!(huge.top_at(200.0, room), 99_961);
}

#[test]
fn the_press_keeps_its_place_on_the_thumb_while_it_is_dragged() {
    let extent = full(0);
    assert_eq!(extent.hit(200.0, 5.0), Hit::Thumb(5.0));
    let room = room(extent, 200.0);
    // The pointer a quarter of the room further down: the thumb's edge 5 above it.
    let pointer = 5.0 + room / 4.0;
    assert_eq!(extent.top_at(200.0, pointer - 5.0), 23);
}

#[test]
fn the_track_above_and_below_the_thumb_pages() {
    assert_eq!(full(0).hit(200.0, 100.0), Hit::Below);
    assert_eq!(full(91).hit(200.0, 10.0), Hit::Above);
    let middle = full(50);
    assert_eq!((middle.page_back(), middle.page_on()), (40, 60));
    assert_eq!(full(5).page_back(), 0, "not before the first row");
    assert_eq!(full(85).page_on(), 91, "not past the last page");
}

#[test]
fn a_list_shorter_than_a_page_does_not_scroll() {
    let short = Extent {
        top: 0,
        page: 10,
        total: 6,
        max_top: 0,
        step: 1,
    };
    assert!(!short.scrolls());
    assert_eq!(
        short.thumb(100.0),
        (0.0, 100.0),
        "the thumb fills the track"
    );
    assert_eq!(short.top_at(100.0, 50.0), 0);
    assert_eq!((short.page_back(), short.page_on()), (0, 0));
    assert!(full(0).scrolls());
}

#[test]
fn the_brief_view_scrolls_by_whole_columns_over_the_same_range() {
    let extent = brief(0);
    let room = room(extent, 300.0);
    assert_eq!(extent.top_at(300.0, 0.0), 0);
    assert_eq!(extent.top_at(300.0, room), 70, "the last columns");
    assert_eq!(extent.top_at(300.0, room * 0.3), 20, "2.1 columns rounded");
    let middle = brief(40);
    assert_eq!((middle.page_back(), middle.page_on()), (10, 70));
}
