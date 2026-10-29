//! The codes a plugin's failure shows as.

use super::{PLUGIN_FAILED, failure_code};

#[test]
fn a_host_code_stays_and_a_plugins_own_is_one_code() {
    assert_eq!(failure_code(Some("plugin_crashed")), "plugin_crashed");
    assert_eq!(failure_code(Some("json.broken")), PLUGIN_FAILED);
    assert_eq!(failure_code(None), PLUGIN_FAILED);
}
