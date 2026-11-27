//! The samples of the plugin commands after `registry_samples_more.rs` was full (RC-110-55).

use crate::{Command, PluginVersionRef};

/// One sample of each later plugin command.
pub(super) fn plugins() -> Vec<Command> {
    vec![Command::RemoveOldPlugins {
        versions: Some(vec![PluginVersionRef {
            id: "p".into(),
            version: "1.0.0".into(),
        }]),
    }]
}
