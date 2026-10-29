use std::time::{Duration, SystemTime};

use zip::DateTime;

use super::dos_time;

#[test]
fn a_time_before_1980_or_unknown_is_the_dos_epoch() {
    assert_eq!(dos_time(None), DateTime::default());
    assert_eq!(dos_time(Some(SystemTime::UNIX_EPOCH)), DateTime::default());
}

#[test]
fn a_time_keeps_its_date() {
    // 2024-05-17 12:34:56 UTC: the date holds in every zone but the day's edges.
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_949_296);
    let stamp = dos_time(Some(time));
    assert_eq!((stamp.year(), stamp.month()), (2024, 5));
    assert_eq!(stamp.second() % 2, 0, "DOS times step by two seconds");
}
