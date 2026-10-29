use super::rows_at;

#[test]
fn the_lines_that_fit_follow_the_line_height() {
    // 18 at the default 13: a 376-high view (8 padding at each end) holds 20 lines.
    assert_eq!(rows_at(376.0, 18.0), 20);
    assert_eq!(rows_at(376.0, 36.0), 10);
    assert_eq!(rows_at(10.0, 18.0), 1, "one at least");
}
