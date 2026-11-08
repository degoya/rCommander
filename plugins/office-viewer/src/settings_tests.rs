use super::*;

#[test]
fn sheet_rows_reads_each_value_and_defaults_to_10000() {
    assert_eq!(SheetRows::from_setting(Some("1000")).rows(), 1_000);
    assert_eq!(SheetRows::from_setting(Some("10000")).rows(), 10_000);
    assert_eq!(SheetRows::from_setting(Some("100000")).rows(), 100_000);
    assert_eq!(SheetRows::from_setting(None), SheetRows::TenThousand);
    assert_eq!(SheetRows::from_setting(Some("all")), SheetRows::TenThousand);
}

#[test]
fn settings_take_the_value() {
    assert_eq!(
        Settings::from_values(Some("1000")),
        Settings {
            sheet_rows: SheetRows::Thousand,
        }
    );
    assert_eq!(Settings::from_values(None), Settings::default());
}
