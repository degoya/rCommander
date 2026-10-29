//! Viewer plugins in Lister and Quick View (RC-100-13, stage 1 of `docs/plugins.md`): the
//! adapter between the plugin runtime (`rc-plugin-host`, RC-100-08) and the window's document
//! model (RC-100-10).
//!
//! - `set.rs` - [`ViewerPlugins`]: the installed, enabled viewers and which claim a file, in the
//!   order they are asked, without running any; `load.rs` - loading them (`rc_plugin_host::installed`, RC-100-09).
//! - `open.rs` - [`ViewerPlugins::open`]: the sensitive-file check, `probe` of each claim in
//!   turn, `open`, the first output into a session file, cancellation; [`Opening`] says what the
//!   window shows. `write.rs` - writing the text or the picture into a file of its own.
//! - `document.rs` - [`PluginDocument`]: the output files, the views, and the session behind the
//!   token spans, the tree and the output written on the first switch to it (RC-100-29).
//! - `streams.rs` - the file as the plugin's source on any backend, the output sink and the line
//!   starts counted while it is written; `settings.rs` - the `plugins.*` settings it follows.
//!
//! Nothing here is a command: MCP reads files with its own tools, which never run a plugin.

mod codes;
mod document;
mod load;
mod open;
mod set;
mod settings;
mod streams;
mod write;

#[cfg(test)]
mod test_support;

pub use codes::{ALL_CODES, PLUGIN_FAILED, failure_code};
pub use document::{PictureFormat, PluginDocument};
pub use open::{OpenRequest, Opening};
pub use rc_plugin_api::viewer::{Output, Purpose, Theme, Token, TokenClass, TreeNode};
pub use rc_plugin_host::codes::{PLUGIN_CANCELLED, PLUGIN_OUTPUT_NOT_OFFERED};
pub use set::{InstalledViewer, MAX_CANDIDATES, MAX_HEAD_BYTES, ViewerPlugins};
pub use settings::PluginSettings;
pub(crate) use settings::stored_plugin_settings;
