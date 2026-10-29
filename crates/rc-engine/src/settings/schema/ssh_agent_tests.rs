use super::{AGENT_MAX_KEYS, AGENT_MAX_KEYS_KEY};
use crate::settings::SettingKind;

#[test]
fn the_bounds_are_the_ssh_sessions_own() {
    assert_eq!(
        AGENT_MAX_KEYS.kind,
        SettingKind::Int {
            min: 1,
            max: i64::from(rc_remote::ssh::MAX_AGENT_KEYS),
            default: i64::from(rc_remote::ssh::DEFAULT_MAX_AGENT_KEYS),
        }
    );
    assert!(super::super::lookup::is_protected(AGENT_MAX_KEYS_KEY));
}
