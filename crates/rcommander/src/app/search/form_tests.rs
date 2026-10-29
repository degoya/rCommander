use super::*;

#[test]
fn an_empty_form_finds_every_name_below_the_start_folder() {
    let form = Form::starting_in("C:\\work".to_owned());
    let query = form.query().expect("a query");
    assert_eq!(query.name, "*.*");
    assert_eq!(query.content, None);
    assert_eq!(query.max_depth, None);
    assert_eq!(query.filters, Filters::default());
    assert_eq!(form.start_folders(), ["C:\\work"]);
}

#[test]
fn every_field_reaches_the_query() {
    let mut form = Form::starting_in(" a ; ;b".to_owned());
    form.name = " *.log ".to_owned();
    form.name_case = true;
    form.depth = "2".to_owned();
    form.text = "error".to_owned();
    form.text_words = true;
    form.utf16_le = true;
    form.windows1252 = true;
    form.not_containing = true;
    form.min_size = "1k".to_owned();
    form.max_size = "2k".to_owned();
    form.days = "3".to_owned();
    form.kind = KindFilter::Files;
    form.readonly = Tri::Unset;
    form.archive = Tri::Set;
    form.skip_hidden = true;
    let query = form.query().expect("a query");
    assert_eq!(form.start_folders(), ["a", "b"]);
    assert_eq!(query.name, "*.log");
    assert!(query.case_sensitive);
    assert_eq!(query.max_depth, Some(2));
    let content = query.content.expect("a content search");
    assert_eq!(content.text, "error");
    assert!(content.whole_words && content.not_containing);
    assert_eq!(
        content.encodings,
        [TextEncoding::Utf16Le, TextEncoding::Windows1252]
    );
    assert_eq!(query.filters.min_size, Some(1024));
    assert_eq!(query.filters.max_size, Some(2048));
    assert_eq!(query.filters.max_age_secs, Some(3 * DAY_SECS));
    assert_eq!(query.filters.kind, KindFilter::Files);
    assert_eq!(query.filters.readonly, Some(false));
    assert_eq!(query.filters.archive, Some(true));
    assert_eq!(query.filters.hidden, None);
    assert!(query.skip_hidden);
}

#[test]
fn a_day_range_covers_both_whole_days() {
    let mut form = Form::starting_in(String::new());
    form.after = "2026-01-01".to_owned();
    form.before = "2026-01-02".to_owned();
    let filters = form.query().expect("a query").filters;
    let (after, before) = (
        filters.modified_after.expect("after"),
        filters.modified_before.expect("before"),
    );
    // Two local days, less the last second; a day may be an hour off around a clock change.
    assert!((2 * DAY_SECS - 3601..=2 * DAY_SECS + 3599).contains(&(before - after)));
}

#[test]
fn the_first_field_that_does_not_read_is_named() {
    for (field, text) in [
        (Field::Depth, "deep"),
        (Field::MinSize, "big"),
        (Field::MaxSize, "-1"),
        (Field::Days, "a week"),
        (Field::After, "01.01.2026"),
        (Field::Before, "2026-13-01"),
    ] {
        let mut form = Form::starting_in(String::new());
        *form.text_mut(field) = text.to_owned();
        assert_eq!(form.query(), Err(field), "{field:?}");
    }
}

/// A "before" day that ends at the epoch - 1969-12-31 where the clock is UTC - overflowed on
/// `end - 1` (RC-080-44). It is no date to search; west of UTC the day still ends after it.
#[test]
fn a_before_day_that_ends_at_the_epoch_does_not_overflow() {
    let mut form = Form::starting_in(String::new());
    form.before = "1969-12-31".to_owned();
    match form.query() {
        Err(field) => assert_eq!(field, Field::Before),
        Ok(query) => {
            let before = query.filters.modified_before.expect("a bound");
            assert!(before < 2 * DAY_SECS, "{before}");
        }
    }
}
