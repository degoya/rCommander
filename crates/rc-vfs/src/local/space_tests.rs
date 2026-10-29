use std::env;

use super::disk_space;

#[test]
fn the_temp_dir_has_a_total_and_no_more_available_than_that() {
    let (available, total) =
        disk_space(&env::temp_dir()).expect("the temp dir is on a file system");
    assert!(total > 0);
    assert!(available <= total);
}

#[test]
fn a_missing_path_is_the_system_error() {
    let missing = env::temp_dir().join("rc-vfs-space-test-no-such-dir");
    assert!(disk_space(&missing).is_err());
}
