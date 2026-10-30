//! `sftp.case_sensitivity` (RC-080-60): `auto` takes the backend's probe, `sensitive` and
//! `insensitive` are taken over it, for the backends opened after the change.

use rc_vfs::Vfs;
use serde_json::json;

use super::tests::harness;
use super::*;
use crate::{Command, settings::SFTP_CASE_KEY};

/// Whether the backend `harness` opens for its connection, with the setting at `value`, tells
/// names apart by case.
async fn case_sensitive_with(value: &str) -> bool {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::SetSetting {
            key: SFTP_CASE_KEY.into(),
            value: json!(value),
        })
        .await
        .expect("set");
    harness
        .engine
        .open_sftp(harness.id, true)
        .await
        .expect("opens")
        .capabilities()
        .case_sensitive
}

#[test]
fn the_setting_reads_as_a_case_rule() {
    assert_eq!(case_rule(Some("auto")), SftpCase::Detect);
    assert_eq!(case_rule(Some("sensitive")), SftpCase::Sensitive);
    assert_eq!(case_rule(Some("insensitive")), SftpCase::Insensitive);
    assert_eq!(case_rule(None), SftpCase::Detect);
}

#[tokio::test]
async fn auto_takes_what_the_probe_finds() {
    let harness = harness().await;
    let vfs = harness
        .engine
        .open_sftp(harness.id, true)
        .await
        .expect("opens");
    assert_eq!(
        vfs.capabilities().case_sensitive,
        rc_remote::ftp::test_server::case_sensitive(harness.served.path()),
        "the served disk decides"
    );
}

#[tokio::test]
async fn sensitive_and_insensitive_are_taken_over_the_probe() {
    assert!(case_sensitive_with("sensitive").await);
    assert!(!case_sensitive_with("insensitive").await);
}
