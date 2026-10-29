//! Real processes stand in for `wsl.exe`: `sleep` for one that hangs, `sh` for one that fails,
//! `cat` of a fixture for one that answers. Unix only, where those exist; the logic is the same
//! on Windows.

use std::time::Duration;

use tokio::process::Command;

use super::*;

#[tokio::test]
async fn a_program_that_cannot_start_lists_nothing() {
    let command = Command::new("rc-test-no-such-program");
    assert_eq!(capture(command, Duration::from_secs(5)).await, None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_hanging_call_ends_at_the_limit_with_no_names() {
    let mut command = Command::new("sleep");
    command.arg("30");
    let started = std::time::Instant::now();
    let names = names_from(command, Duration::from_millis(200)).await;
    assert!(names.is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "waited {:?} instead of the 200 ms limit",
        started.elapsed()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_failing_call_lists_nothing_whatever_it_printed() {
    let mut command = Command::new("sh");
    command.args(["-c", "printf 'Ubuntu\\n'; exit 1"]);
    assert!(names_from(command, Duration::from_secs(5)).await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn the_output_of_a_call_becomes_names() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/wsl/list-q-utf16-bom.bin"
    );
    let mut command = Command::new("cat");
    command.arg(fixture);
    assert_eq!(
        names_from(command, Duration::from_secs(5)).await,
        ["Ubuntu", "Debian"]
    );
}
