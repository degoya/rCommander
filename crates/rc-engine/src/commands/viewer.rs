//! The `files.read_*` commands: a capped window of a file as text or hex rows, read the way the
//! viewer reads it - never the whole file, private keys masked.

use std::path::PathBuf;

use rc_vfs::{BackendId, VfsPath};

use super::panel::resolve_path;
use crate::{
    Engine, EngineError, Outcome, PanelId, read_guard,
    viewer::{
        BYTES_PER_ROW, DETECT_BYTES, Detected, FileWindow, HexWindow, ReadHexRequest,
        ReadTextRequest, TextWindow, decode_window, detect, detect_dos, dos_extensions,
        hex::rows as hex_rows,
        read::{HEX_DEFAULT_BYTES, HEX_MAX_BYTES, TEXT_DEFAULT_BYTES, TEXT_MAX_BYTES},
        secret::{
            KEY_CONTEXT, KEY_FILE_MAX, PLACEHOLDER, has_private_key, inside_key_block,
            mask_private_keys,
        },
        sniff::looks_binary,
    },
};

impl Engine {
    /// `files.read_text`.
    pub(crate) async fn read_text(&self, request: ReadTextRequest) -> Result<Outcome, EngineError> {
        let (path, window) = self.open_window(request.panel, &request.path).await?;
        let size = window.size();
        let head = window.read_uncached(0, DETECT_BYTES).await?;
        let dos = request.encoding.is_none() && self.is_dos_file(&path).await;
        let detected = match request.encoding {
            Some(encoding) => Detected::chosen(encoding, &head),
            None if dos => detect_dos(&head),
            None => detect(&head),
        };
        let limit = request
            .max_bytes
            .unwrap_or(TEXT_DEFAULT_BYTES)
            .clamp(1, TEXT_MAX_BYTES);
        let offset = request.offset.min(size);
        let bytes = window.read_uncached(offset, to_len(limit)).await?;
        let at_eof = offset + bytes.len() as u64 >= size;
        let decoded = decode_window(&bytes, offset, detected, at_eof);
        let (text, masked) = if size <= KEY_FILE_MAX && has_private_key(&head) {
            (PLACEHOLDER.to_owned(), true)
        } else if let Some(masked) = mask_private_keys(&decoded.text) {
            (masked, true)
        } else if inside_key(&window, decoded.start, decoded.end, detected).await? {
            (PLACEHOLDER.to_owned(), true)
        } else {
            (decoded.text, false)
        };
        Ok(Outcome::FileText(TextWindow {
            path,
            size,
            encoding: detected.encoding,
            bom: detected.bom_len > 0,
            start: decoded.start,
            end: decoded.end,
            truncated: decoded.end < size,
            binary: looks_binary(&head, detected),
            masked,
            text,
        }))
    }

    /// `files.read_hex`: rows from `offset` itself, as `xxd -s` prints them.
    pub(crate) async fn read_hex(&self, request: ReadHexRequest) -> Result<Outcome, EngineError> {
        let (path, window) = self.open_window(request.panel, &request.path).await?;
        let size = window.size();
        let limit = request
            .length
            .unwrap_or(HEX_DEFAULT_BYTES)
            .clamp(1, HEX_MAX_BYTES);
        let offset = request.offset.min(size);
        let bytes = window.read_uncached(offset, to_len(limit)).await?;
        let end = offset + bytes.len() as u64;
        let key_file = size <= KEY_FILE_MAX
            && has_private_key(&window.read_uncached(0, to_len(KEY_FILE_MAX)).await?);
        let mut masked = key_file || has_private_key(&bytes);
        if !masked {
            let head = window.read_uncached(0, DETECT_BYTES).await?;
            masked = inside_key(&window, offset, end, detect(&head)).await?;
        }
        let rows = if masked {
            Vec::new()
        } else {
            hex_rows(&bytes, offset, BYTES_PER_ROW)
                .iter()
                .map(|row| row.to_xxd(BYTES_PER_ROW))
                .collect()
        };
        Ok(Outcome::FileHex(HexWindow {
            path,
            size,
            start: offset,
            end,
            truncated: end < size,
            masked,
            rows,
        }))
    }

    /// `target` resolved against `panel`'s directory (the active panel's by default), as the
    /// file commands resolve it: for a guard that judges the path before a command reads it.
    /// Inside [`Engine::pin_mcp_panels`] against the pinned folder (RC-080-42).
    #[must_use]
    pub fn resolve_target(&self, panel: Option<PanelId>, target: &str) -> VfsPath {
        let id = self.resolve(panel);
        resolve_path(&self.mcp_folder(id), target)
    }

    /// `target` located as the file commands locate it: resolved against `panel`'s directory,
    /// or a server's location (`sftp://<connection>/<path>`, `ftp://...`) on that connection's
    /// backend. For a guard that judges the place before a command writes there.
    ///
    /// # Errors
    ///
    /// The connection is not found or ambiguous (`sftp_connection_not_found`,
    /// `ftp_connection_not_found`, ...), or the database fails; a location naming only the
    /// connection fails as its start directory does. Inside [`Engine::pin_mcp_panels`] against
    /// the pinned folder (RC-080-42).
    pub async fn locate_target(
        &self,
        panel: Option<PanelId>,
        target: &str,
    ) -> Result<VfsPath, EngineError> {
        self.locate_from(panel, target).await
    }

    /// Where `path` really leads, links and junctions followed - on the local backend; `None`
    /// on another one or when nothing is there.
    pub async fn link_target(&self, path: &VfsPath) -> Option<PathBuf> {
        match path.backend {
            BackendId::Local => tokio::fs::canonicalize(&path.path).await.ok(),
            // `BackendId` is `non_exhaustive`: the remote backends arrive with their jobs.
            _ => None,
        }
    }

    /// Whether `path` is DOS text, CP437 unless its bytes say otherwise: its extension is one of
    /// `viewer.dos_extensions` (RC-020-11), as the viewer has it.
    async fn is_dos_file(&self, path: &VfsPath) -> bool {
        crate::viewer::is_dos_file(&path.path, &dos_extensions(self.db().ok()).await)
    }

    /// The file `target` names, opened for reading: resolved against a panel's directory, or a
    /// server's location as `ops.copy` takes it (RC-060-05), its backend opened. Under MCP the
    /// path read is judged by the read guard, also on a server.
    async fn open_window(
        &self,
        panel: Option<PanelId>,
        target: &str,
    ) -> Result<(VfsPath, FileWindow), EngineError> {
        let path = self.locate_from(panel, target).await?;
        if let Some(guard) = read_guard::current(&path.backend) {
            guard.check(&path.path).await?;
        }
        let vfs = self.open_vfs(&path, self.announces_locked_vault()).await?;
        let window = FileWindow::open(vfs, path.path.clone()).await?;
        Ok((path, window))
    }
}

/// Whether the bytes `start..end` of `window`, which carry no key marker, lie inside a private
/// key block: judged by the [`KEY_CONTEXT`] bytes on either side, decoded as the window is.
async fn inside_key(
    window: &FileWindow,
    start: u64,
    end: u64,
    detected: Detected,
) -> Result<bool, EngineError> {
    let from = start.saturating_sub(KEY_CONTEXT);
    let before = window.read_uncached(from, to_len(start - from)).await?;
    let after = window.read_uncached(end, to_len(KEY_CONTEXT)).await?;
    let at_eof = end + after.len() as u64 >= window.size();
    Ok(inside_key_block(
        &decode_window(&before, from, detected, false).text,
        &decode_window(&after, end, detected, at_eof).text,
    ))
}

/// A capped length as a buffer size.
fn to_len(bytes: u64) -> usize {
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
