//! `file.set_comment` and the comments the properties dialog shows: a folder's `descript.ion`
//! read, changed and written back through its backend (`comments.rs` holds the format).

use std::sync::Arc;

use rc_vfs::{Vfs, VfsError, VfsPath, WriteMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::panel::resolve_path;
use crate::{
    Engine, EngineError, Outcome, PanelId,
    comments::{DESCRIPTION_FILE, Descriptions, MAX_COMMENT, MAX_FILE, decode, encode},
};

/// The comments of the folder `dir`; none when it has no `descript.ion`.
async fn read_descriptions(vfs: &Arc<dyn Vfs>, dir: &VfsPath) -> Result<Descriptions, EngineError> {
    let file = dir.join(DESCRIPTION_FILE);
    let entry = match vfs.stat(&file.path).await {
        Ok(entry) => entry,
        Err(VfsError::NotFound { .. }) => return Ok(Descriptions::default()),
        Err(error) => return Err(error.into()),
    };
    if entry.size > MAX_FILE {
        return Err(EngineError::PropertyInvalid {
            reason: "the folder's descript.ion is too large to read",
        });
    }
    let mut bytes = Vec::new();
    vfs.open_read(&file.path)
        .await?
        .take(MAX_FILE)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| VfsError::from_io(error, &file.path))?;
    Ok(Descriptions::parse(&decode(&bytes)))
}

impl Engine {
    /// The comment of the entry at `path`, if its folder keeps one; unreadable is none.
    pub(crate) async fn comment_of(&self, vfs: &Arc<dyn Vfs>, path: &VfsPath) -> Option<String> {
        let dir = path.parent()?;
        let name = path.path.file_name()?.to_string_lossy();
        read_descriptions(vfs, &dir).await.ok()?.get(&name)
    }

    /// `file.set_comment`: the comment of `path` (or of the entry under the cursor); an empty
    /// one removes it, and a `descript.ion` left empty goes too.
    pub(crate) async fn set_comment(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        comment: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let Some(comment) = comment else {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment to set, or an empty one to remove",
            });
        };
        if comment.chars().count() > MAX_COMMENT {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment has 4096 characters at most",
            });
        }
        let id = self.resolve(panel);
        let target = {
            let state = self.state();
            let panel = state.panel(id);
            match path {
                Some(path) => resolve_path(&panel.path, path),
                None => panel
                    .cursor_entry()
                    .map(|entry| panel.path.join(&entry.name))
                    .ok_or(EngineError::PropertyInvalid {
                        reason: "a comment needs the entry it is about",
                    })?,
            }
        };
        let (Some(dir), Some(name)) = (target.parent(), target.path.file_name()) else {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment needs the entry it is about",
            });
        };
        let name = name.to_string_lossy().into_owned();
        if name.contains(['"', '\n', '\r']) || name.eq_ignore_ascii_case(DESCRIPTION_FILE) {
            return Err(EngineError::PropertyInvalid {
                reason: "this name cannot carry a comment in descript.ion",
            });
        }
        let vfs = self.vfs(&target)?;
        vfs.stat(&target.path).await?;
        let mut comments = read_descriptions(&vfs, &dir).await?;
        comments.set(&name, Some(comment));
        let file = dir.join(DESCRIPTION_FILE);
        if comments.is_empty() {
            match vfs.remove_file(&file.path).await {
                Ok(()) | Err(VfsError::NotFound { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        } else {
            let mut stream = vfs.open_write(&file.path, WriteMode::Truncate).await?;
            let written = async {
                stream.write_all(&encode(&comments.render())).await?;
                stream.shutdown().await
            };
            written
                .await
                .map_err(|error| VfsError::from_io(error, &file.path))?;
        }
        self.refresh_showing(std::slice::from_ref(&file)).await;
        let comment = comments.get(&name);
        Ok(Outcome::CommentSet {
            path: target,
            comment,
        })
    }
}

#[cfg(test)]
#[path = "comment_tests.rs"]
mod tests;
