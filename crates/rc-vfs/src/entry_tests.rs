use std::time::{Duration, SystemTime};

use super::{Attributes, Entry, EntryKind, unix_seconds, unix_time};

fn file(modified: Option<SystemTime>) -> Entry {
    Entry {
        name: "a".to_owned(),
        kind: EntryKind::File,
        size: 0,
        modified,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

#[test]
fn unix_time_holds_a_real_stamp_and_refuses_an_overflow() {
    let epoch = unix_time(0).expect("the epoch fits");
    assert_eq!(epoch, SystemTime::UNIX_EPOCH);
    assert_eq!(
        unix_time(1_700_000_000),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    assert_eq!(unix_time(u64::MAX), None);
}

#[test]
fn unix_seconds_round_trip_and_before_the_epoch() {
    let time = unix_time(1_600_000_000).expect("fits");
    assert_eq!(unix_seconds(time), Some(1_600_000_000));
    assert_eq!(unix_seconds(SystemTime::UNIX_EPOCH), Some(0));
    let before = SystemTime::UNIX_EPOCH
        .checked_sub(Duration::from_secs(1))
        .expect("1969 fits");
    assert_eq!(unix_seconds(before), None);
}

#[test]
fn an_entry_reports_its_modification_time_as_unix_seconds() {
    assert_eq!(file(None).modified_unix(), None);
    assert_eq!(
        file(unix_time(1_500_000_000)).modified_unix(),
        Some(1_500_000_000)
    );
}

#[test]
fn a_mode_is_readonly_without_any_write_bit() {
    for (mode, readonly) in [
        (0o444, true),
        (0o555, true),
        (0o000, true),
        (0o644, false),
        (0o020, false),
        (0o002, false),
        (0o4555, true),
    ] {
        let attributes = Attributes::from_mode(mode);
        assert_eq!(attributes.mode, Some(mode));
        assert_eq!(attributes.readonly, readonly, "{mode:o}");
        assert_eq!(attributes.windows, None);
    }
}
