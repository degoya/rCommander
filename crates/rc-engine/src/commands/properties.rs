//! `file.properties`, `file.set_attributes`, `file.create_link` and `file.dir_size` (RC-020-06):
//! the properties dialog, Total Commander's "Change attributes", Ctrl+Shift+F5 and Ctrl+L.
//!
//! Like the `ops.*` commands they take the named paths, else the panel's selection, else the
//! entry under its cursor. A change refreshes the panels that show the folder it touched.

use std::path::PathBuf;

use rc_ops::{ApplyTo, AttributeReport, DirUsage};
use rc_vfs::{EntryKind, LinkKind, PropertyCapabilities, PropertyChange, VfsPath};

use super::{ops::sources_of, panel::resolve_path};
use crate::{Engine, EngineError, ItemProperties, Outcome, PanelId, PathUsage, PropertiesReport};

impl Engine {
    /// The entries a properties command means.
    fn chosen(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
    ) -> Result<Vec<VfsPath>, EngineError> {
        let id = self.resolve(panel);
        sources_of(self.state().panel(id), paths)
    }

    /// What the properties dialog can change at `path`; nothing where no backend is open.
    #[must_use]
    pub fn property_capabilities(&self, path: &VfsPath) -> PropertyCapabilities {
        self.vfs(path).map_or(PropertyCapabilities::NONE, |vfs| {
            vfs.property_capabilities()
        })
    }

    /// `file.properties`: each entry with its details and comment; a folder's contents counted
    /// with `count`; sums when there are several.
    pub(crate) async fn properties(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        count: bool,
    ) -> Result<Outcome, EngineError> {
        let chosen = self.chosen(panel, paths)?;
        let mut items = Vec::with_capacity(chosen.len());
        for path in &chosen {
            let vfs = self.vfs(path)?;
            let entry = vfs.stat(&path.path).await?;
            let details = vfs.details(&path.path).await.unwrap_or_default();
            let usage = if count && entry.kind == EntryKind::Dir {
                Some(rc_ops::dir_size(&*vfs, &path.path).await?)
            } else {
                None
            };
            let comment = self.comment_of(&vfs, path).await;
            items.push(ItemProperties {
                path: path.clone(),
                entry,
                details,
                usage,
                comment,
            });
        }
        let total = (items.len() > 1).then(|| sum(&items));
        let capabilities = chosen.first().map_or(PropertyCapabilities::NONE, |path| {
            self.property_capabilities(path)
        });
        Ok(Outcome::Properties(PropertiesReport {
            items,
            total,
            capabilities,
        }))
    }

    /// `file.set_attributes`: `change` on each entry and, with `recursive`, below it. An entry
    /// that fails is reported and the others go on; when none could be changed, its error.
    pub(crate) async fn set_file_attributes(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        change: PropertyChange,
        recursive: bool,
        apply_to: ApplyTo,
    ) -> Result<Outcome, EngineError> {
        if change.is_empty() {
            return Err(EngineError::PropertyInvalid {
                reason: "there is no attribute or time to change",
            });
        }
        let chosen = self.chosen(panel, paths)?;
        let mut report = AttributeReport::default();
        let mut last_error = None;
        for path in &chosen {
            let vfs = self.vfs(path)?;
            if let Err(error) = rc_ops::set_properties_tree(
                &*vfs,
                &path.path,
                change,
                recursive,
                apply_to,
                &mut report,
            )
            .await
            {
                report.record(&path.path, &error);
                last_error = Some(error);
            }
        }
        self.refresh_showing(&chosen).await;
        match last_error {
            Some(error) if report.changed == 0 => Err(error.into()),
            _ => Ok(Outcome::AttributesSet(report)),
        }
    }

    /// `file.create_link`: a link at `link` (relative to the panel's folder) to `target`, or to
    /// the entry under the cursor. Without a kind, a shortcut where the backend makes them, else
    /// a symbolic link; a shortcut's name gets `.lnk` when it lacks it.
    pub(crate) async fn create_link(
        &self,
        panel: Option<PanelId>,
        target: Option<&str>,
        link: Option<&str>,
        kind: Option<LinkKind>,
    ) -> Result<Outcome, EngineError> {
        let Some(link) = link.filter(|link| !link.trim().is_empty()) else {
            return Err(EngineError::PropertyInvalid {
                reason: "a link needs the place it goes",
            });
        };
        let id = self.resolve(panel);
        let (target, mut link) = {
            let state = self.state();
            let panel = state.panel(id);
            let target = match target {
                Some(target) => resolve_path(&panel.path, target),
                None => panel
                    .cursor_entry()
                    .map(|entry| panel.path.join(&entry.name))
                    .ok_or(EngineError::PropertyInvalid {
                        reason: "a link needs the entry it leads to",
                    })?,
            };
            (target, resolve_path(&panel.path, link))
        };
        if link.backend != target.backend {
            return Err(EngineError::PropertyInvalid {
                reason: "a link and its target are on one file system",
            });
        }
        let vfs = self.vfs(&link)?;
        let kind = kind.unwrap_or(if vfs.property_capabilities().links.shortcut {
            LinkKind::Shortcut
        } else {
            LinkKind::Symlink
        });
        if kind == LinkKind::Shortcut && !has_lnk_extension(&link.path) {
            let mut name = link.path.clone().into_os_string();
            name.push(".lnk");
            link.path = PathBuf::from(name);
        }
        vfs.create_link(&link.path, &target.path, kind).await?;
        self.refresh_showing(std::slice::from_ref(&link)).await;
        Ok(Outcome::LinkCreated {
            link,
            target,
            link_kind: kind,
        })
    }

    /// `file.dir_size`: the occupied space of each entry and the sum, a hard link shared by
    /// them counted once. Dropping the future stops the count.
    pub(crate) async fn dir_size(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
    ) -> Result<Outcome, EngineError> {
        let chosen = self.chosen(panel, paths)?;
        let Some(first) = chosen.first() else {
            return Err(rc_ops::OpsError::NoSources.into());
        };
        let vfs = self.vfs(first)?;
        let starts: Vec<&std::path::Path> = chosen.iter().map(|path| path.path.as_path()).collect();
        let (each, total) = rc_ops::dir_sizes(&*vfs, &starts).await?;
        let items = chosen
            .into_iter()
            .zip(each)
            .map(|(path, usage)| PathUsage { path, usage })
            .collect();
        Ok(Outcome::DirSizes { items, total })
    }

    /// Reads again the panels that show the folder of one of `paths`.
    pub(crate) async fn refresh_showing(&self, paths: &[VfsPath]) {
        let parents: Vec<VfsPath> = paths.iter().filter_map(VfsPath::parent).collect();
        for id in PanelId::BOTH {
            let shown = self.state().panel(id).path.clone();
            if parents.contains(&shown) || paths.contains(&shown) {
                // A panel that cannot be read again keeps its old listing.
                if let Err(error) = self.refresh(Some(id)).await {
                    tracing::debug!(code = error.code(), "a panel was not read again");
                }
            }
        }
    }
}

/// Whether a path's name ends in `.lnk`, in any case.
fn has_lnk_extension(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
}

/// The sums of several entries: a file adds itself, a folder itself and its counted contents.
fn sum(items: &[ItemProperties]) -> DirUsage {
    let mut total = DirUsage {
        allocated: Some(0),
        ..DirUsage::default()
    };
    for item in items {
        if item.entry.kind == EntryKind::Dir {
            total.dirs += 1;
            match &item.usage {
                Some(usage) => total.add(usage),
                None => total.allocated = None,
            }
        } else {
            total.files += 1;
            if item.entry.kind == EntryKind::File {
                total.bytes += item.entry.size;
            }
            total.allocated = total
                .allocated
                .zip(item.details.allocated)
                .map(|(sum, more)| sum + more);
        }
    }
    total
}

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;
