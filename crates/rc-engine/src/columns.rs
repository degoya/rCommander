//! Column sets and the comments view (RC-100-90): Total Commander's custom columns. A column set
//! is the person's own columns of the file list - each a field of the entry ([`Field`]: name,
//! extension, size, packed size, the three times, attributes, rights, owner, group, comment)
//! with a title, a width and an alignment - and the folder masks of the automatic switch.
//!
//! The spec is versioned JSON ([`ColumnSetSpec`], `spec.rs`), checked at every boundary: 1 to 16
//! columns, 0 to 32 masks, 16 KiB, at most [`MAX_SETS`] sets (`store.rs`, rows of `rc_db`'s
//! `column_sets` table). A panel shows a set in the mode [`crate::ListMode::Custom`]; the
//! comments view ([`crate::ListMode::Comments`], Ctrl+Shift+F2) is a fixed set of name,
//! extension and comment. The full view stays the fixed standard with `columns.*_width`.
//!
//! Entering a folder, a panel switches to the first set whose mask matches it while
//! [`AUTO_SWITCH_KEY`] is on, else [`AUTO_DEFAULT_KEY`] decides; a mode chosen by hand stays
//! until a folder of another set (`auto.rs`). The folder's `descript.ion` is read only while the
//! mode shows a comment column, after the listing and outside the state's lock (`view.rs`).

mod auto;
mod error;
mod panel;
mod spec;
mod store;
mod view;

pub(crate) use auto::FolderRule;
pub use auto::{AutoDefault, mask_matches, matching_set};
pub use error::ColumnSetError;
pub use panel::{FolderComments, PanelColumns};
pub use spec::{
    Align, Column, ColumnSet, ColumnSetId, ColumnSetSpec, ColumnSetsOutcome, Field, MAX_COLUMNS,
    MAX_MASK_CHARS, MAX_MASKS, MAX_NAME_CHARS, MAX_SETS, MAX_SPEC_BYTES, MAX_TITLE_CHARS,
    MAX_WIDTH, MIN_WIDTH, SPEC_VERSION, clean_name,
};

/// Whether a panel entering a folder switches to the set whose mask matches it (Total
/// Commander's "Auto switch mode", `cm_ToggleAutoViewModeSwitch`).
pub const AUTO_SWITCH_KEY: &str = "columns.auto_switch";

/// What a panel shows where no set's mask matches: `unchanged` (the default), `full` or `brief`
/// ([`AutoDefault`]).
pub const AUTO_DEFAULT_KEY: &str = "columns.auto_default";
