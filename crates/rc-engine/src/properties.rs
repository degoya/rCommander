//! What the properties commands answer (RC-020-06): the properties dialog's data, occupied
//! space, the size column's counted folders and the clipboard's text of names.

use rc_ops::DirUsage;
use rc_vfs::{Details, Entry, PropertyCapabilities, VfsPath};
use serde::{Deserialize, Serialize};

/// One entry as the properties dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemProperties {
    pub path: VfsPath,
    /// Name, kind (a link with its target), size, times, attributes.
    pub entry: Entry,
    /// Access time, bytes on disk, links, owner - what the backend tells.
    pub details: Details,
    /// A folder's contents, when they were counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<DirUsage>,
    /// The `descript.ion` comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// The properties of one or more entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertiesReport {
    pub items: Vec<ItemProperties>,
    /// The sums over several entries: files and their bytes, folders and (when counted)
    /// their contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<DirUsage>,
    /// What can be changed where the entries are: the dialog greys out the rest.
    pub capabilities: PropertyCapabilities,
}

/// One start of a size count and what it found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathUsage {
    pub path: VfsPath,
    pub usage: DirUsage,
}

/// A folder's size in the size column: being counted, or counted (Space, Alt+Shift+Enter).
/// Kept until the panel lists its folder again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirSize {
    Counting,
    Bytes(u64),
}

/// What `clipboard.copy_names` puts on the clipboard, one line per entry: Total Commander's
/// `cm_CopyNamesToClip`, `cm_CopyFullNamesToClip`, `cm_CopyDetailsToClip` and
/// `cm_CopyFullDetailsToClip`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameFormat {
    /// The names.
    #[default]
    Names,
    /// The full paths.
    FullPaths,
    /// Name, size and date, separated by tabs.
    Details,
    /// Full path, size and date, separated by tabs.
    FullDetails,
}
