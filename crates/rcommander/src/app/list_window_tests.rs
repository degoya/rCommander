use super::ListWindow;

#[test]
fn the_cursor_stays_on_screen_and_the_wheel_moves_only_the_view() {
    let mut rows = ListWindow::new(10);
    rows.step(15, 100);
    assert_eq!((rows.cursor(), rows.top()), (15, 6));
    rows.step(200, 100);
    assert_eq!((rows.cursor(), rows.visible(100)), (99, 90..100));
    rows.point_at(100, 100);
    assert_eq!(rows.cursor(), 99, "no row there");
    rows.scroll(-50, 100);
    assert_eq!((rows.cursor(), rows.top()), (99, 40));
    rows.scroll(500, 100);
    assert_eq!(rows.top(), 90, "never past the end");
    rows.reset();
    assert_eq!((rows.cursor(), rows.top(), rows.page()), (0, 0, 10));
}

#[test]
fn a_taller_list_stays_full_and_a_lower_one_keeps_the_cursor() {
    let mut rows = ListWindow::new(10);
    rows.step(99, 100);
    rows.set_page(20, 100);
    assert_eq!(rows.visible(100), 80..100);
    rows.set_page(0, 100);
    assert_eq!((rows.page(), rows.top()), (1, 99));
    rows.set_top(3, 2);
    assert_eq!(rows.top(), 1);
    // Without a cursor the view keeps its place.
    let mut view = ListWindow::new(1);
    view.set_top(50, 100);
    view.fit(10, 100);
    assert_eq!(view.visible(100), 50..60);
}
