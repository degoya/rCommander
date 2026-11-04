//! A stored offer is installed only while it is newer than the running version.

use super::not_older;
use crate::updates::RUNNING;

#[test]
fn only_a_newer_stored_offer_is_downloaded_or_installed() {
    assert!(not_older("999.0.0").is_ok());
    for stale in [RUNNING, "0.1.0", "not a version"] {
        let refused = not_older(stale).expect_err(stale);
        assert_eq!(refused.code(), "update.no_offer", "{stale}");
    }
}
