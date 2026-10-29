//! `clipboard.copy_names`: the text Total Commander's "copy names to clipboard" commands put
//! there, one line per selected entry (or the one under the cursor). The window writes it to
//! the clipboard; the engine only makes the text. Full paths are written as the panel shows its
//! folder, so an archive's or a connection's entry can be typed back into a path field.

use std::path::Path;

use chrono::{DateTime, Local};
use rc_vfs::{BackendId, VfsPath};

use crate::{
    ConnectionId, DirSize, Engine, EngineError, NameFormat, Outcome, PanelId,
    archives::shown_path,
    remote::{ftp_location, sftp::sftp_location},
};

/// The line break between entries, the platform's.
const LINE_BREAK: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// What the size column shows for an uncounted folder, as Total Commander writes it.
const DIR_MARKER: &str = "<DIR>";

/// How a location on a connection is written out: its connection's name and the path on it.
type Written = fn(&str, &Path) -> String;

impl Engine {
    /// The text for the clipboard; `entry_not_found` when the panel has no entry to name. A full
    /// path is the one the panel shows: inside an archive the archive file's path with the path
    /// inside appended, on a connection `ftp://<connection name>/<path>`.
    pub(crate) async fn copy_names(
        &self,
        panel: Option<PanelId>,
        format: NameFormat,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let details = matches!(format, NameFormat::Details | NameFormat::FullDetails);
        // The entries' names and details, read before the connection's name is looked up.
        let (folder, chosen) = {
            let state = self.state();
            let panel = state.panel(id);
            let entries: Vec<_> = if panel.selection.is_empty() {
                panel.cursor_entry().into_iter().collect()
            } else {
                panel
                    .entries
                    .iter()
                    .filter(|entry| panel.selection.contains(&entry.name))
                    .collect()
            };
            let chosen: Vec<(String, String)> = entries
                .into_iter()
                .map(|entry| {
                    if !details {
                        return (entry.name.clone(), String::new());
                    }
                    let size = if entry.is_dir_like() {
                        match panel.dir_sizes.get(&entry.name) {
                            Some(DirSize::Bytes(bytes)) => bytes.to_string(),
                            _ => DIR_MARKER.to_owned(),
                        }
                    } else {
                        entry.size.to_string()
                    };
                    let date = entry.modified.map_or_else(String::new, |time| {
                        DateTime::<Local>::from(time)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                    });
                    (entry.name.clone(), format!("\t{size}\t{date}"))
                })
                .collect();
            (panel.path.clone(), chosen)
        };
        if chosen.is_empty() {
            return Err(EngineError::EntryNotFound {
                name: String::new(),
            });
        }
        let full = matches!(format, NameFormat::FullPaths | NameFormat::FullDetails);
        let remote = if full {
            self.connection_writer(&folder).await
        } else {
            None
        };
        let shown = shown_path(&folder);
        let lines: Vec<String> = chosen
            .into_iter()
            .map(|(name, details)| {
                let name = match (full, &remote) {
                    (false, _) => name,
                    (true, Some((connection, written))) => {
                        written(connection, &folder.path.join(&name))
                    }
                    (true, None) => shown.join(&name).display().to_string(),
                };
                name + &details
            })
            .collect();
        Ok(Outcome::ClipboardText {
            text: lines.join(LINE_BREAK),
        })
    }

    /// For a folder on a connection, the connection's name and how its locations are written;
    /// `None` for a local or archive folder, or when the connection cannot be read (the path on
    /// the server is written then).
    async fn connection_writer(&self, folder: &VfsPath) -> Option<(String, Written)> {
        let (id, written): (i64, Written) = match folder.backend {
            BackendId::Ftp { connection_id } => (connection_id, ftp_location),
            BackendId::Sftp { connection_id } => (connection_id, sftp_location),
            _ => return None,
        };
        let connection = self.db().ok()?.connection(ConnectionId(id)).await.ok()?;
        Some((connection.name, written))
    }
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
