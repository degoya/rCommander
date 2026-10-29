//! The properties tools (RC-020-06): `files_properties`, `files_set_attributes`,
//! `files_create_link`, `files_dir_size` and `files_set_comment` - thin wrappers over
//! `file.properties`, `file.set_attributes`, `file.create_link`, `file.dir_size` and
//! `file.set_comment`. (`files_stat`, one entry also on an FTP connection, is `remote.rs`'s.)
//!
//! Metadata is no file content, so reading it needs no guard. Changing attributes could open a
//! key file to everyone (a mode of 0644) and a link is a second way into its target - a hard
//! link even one the guard cannot follow - so both refuse sensitive paths (`guard.rs`), as the
//! target and as the link's place. The write guard (`guard_write.rs`, RC-080-09) refuses what
//! they change or create in a protected place: attributes there (with `recursive`, anywhere
//! below), a link there or leading there, a comment's `descript.ion` there.

use std::time::SystemTime;

use rc_engine::{ApplyTo, Command, LinkKind, PanelId, PropertyChange};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
    guard::{refuse_sensitive, refuse_sensitive_tree},
};

use super::panels::PanelArg;

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct StatParams {
    /// The entries: absolute, or relative to the panel's directory. Empty: the panel's
    /// selection, or the entry under its cursor.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Count each folder's contents too (bytes, files, folders); slow on big trees.
    #[serde(default)]
    pub count: bool,
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// Which entries below a folder a recursive change reaches.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApplyToArg {
    Files,
    Dirs,
    #[default]
    Both,
}

impl From<ApplyToArg> for ApplyTo {
    fn from(apply: ApplyToArg) -> Self {
        Self {
            files: !matches!(apply, ApplyToArg::Dirs),
            dirs: !matches!(apply, ApplyToArg::Files),
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct SetAttributesParams {
    /// The entries: absolute, or relative to the panel's directory.
    pub paths: Vec<String>,
    /// Read-only on (true) or off (false); omitted stays.
    #[serde(default)]
    pub readonly: Option<bool>,
    /// Windows only: hidden, system, archive.
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub system: Option<bool>,
    #[serde(default)]
    pub archive: Option<bool>,
    /// Unix only: the permission bits in octal, such as "644" or "0755".
    #[serde(default)]
    pub mode: Option<String>,
    /// The modification time: RFC 3339 (2026-10-17T12:00:00Z) or "now".
    #[serde(default)]
    pub modified: Option<String>,
    /// The creation time (Windows): RFC 3339 or "now".
    #[serde(default)]
    pub created: Option<String>,
    /// Also everything below a folder that apply_to names.
    #[serde(default)]
    pub recursive: bool,
    /// files, dirs or both (default) below a folder; the named entries always change.
    #[serde(default)]
    pub apply_to: ApplyToArg,
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// A kind of link, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LinkKindArg {
    Shortcut,
    Symlink,
    Hardlink,
    Junction,
}

impl From<LinkKindArg> for LinkKind {
    fn from(kind: LinkKindArg) -> Self {
        match kind {
            LinkKindArg::Shortcut => Self::Shortcut,
            LinkKindArg::Symlink => Self::Symlink,
            LinkKindArg::Hardlink => Self::Hardlink,
            LinkKindArg::Junction => Self::Junction,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct CreateLinkParams {
    /// What the link leads to: absolute, or relative to the panel's directory.
    pub target: String,
    /// Where the link goes: absolute, or relative to the panel's directory. Never replaced.
    pub link: String,
    /// shortcut (a .lnk, Windows), symlink, hardlink (files on one volume) or junction
    /// (folders, Windows); a shortcut where possible, else a symlink, when omitted.
    #[serde(default)]
    pub kind: Option<LinkKindArg>,
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DirSizeParams {
    /// Files and folders: absolute, or relative to the panel's directory. Empty: the panel's
    /// selection, or the entry under its cursor.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct SetCommentParams {
    /// The entry: absolute, or relative to the panel's directory.
    pub path: String,
    /// The comment (4096 characters at most, line breaks kept); empty removes it.
    pub comment: String,
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// An argument the tool cannot take, as an answer.
fn invalid(message: &str) -> CallToolResult {
    error_result("property_invalid", message)
}

/// A time as RFC 3339 or "now".
pub(crate) fn parse_time(text: &str) -> Option<SystemTime> {
    if text.trim().eq_ignore_ascii_case("now") {
        return Some(SystemTime::now());
    }
    chrono::DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(SystemTime::from)
}

/// Octal permission bits, at most 7777.
pub(crate) fn parse_mode(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 8)
        .ok()
        .filter(|mode| *mode <= 0o7777)
}

/// The change the parameters ask for, or the answer that refuses them.
fn change_of(params: &SetAttributesParams) -> Result<PropertyChange, CallToolResult> {
    let time = |text: &Option<String>, what: &str| match text {
        Some(text) => parse_time(text)
            .map(Some)
            .ok_or_else(|| invalid(&format!("{what} is RFC 3339 (2026-10-17T12:00:00Z) or now"))),
        None => Ok(None),
    };
    let mode = match &params.mode {
        Some(text) => {
            Some(parse_mode(text).ok_or_else(|| invalid("mode is octal, 0 to 7777, such as 644"))?)
        }
        None => None,
    };
    Ok(PropertyChange {
        mode,
        readonly: params.readonly,
        hidden: params.hidden,
        system: params.system,
        archive: params.archive,
        modified: time(&params.modified, "modified")?,
        created: time(&params.created, "created")?,
    })
}

impl RcMcpServer {
    /// The refusal for the first of `targets` (relative to `panel`) the write guard protects;
    /// with `tree`, also for anything below a local folder.
    async fn refuse_protected<'a>(
        &self,
        panel: Option<PanelId>,
        targets: impl IntoIterator<Item = &'a str>,
        tree: bool,
    ) -> Option<CallToolResult> {
        let guard = self.write_guard();
        for target in targets {
            let at = self.context.engine.resolve_target(panel, target);
            let refusal = if tree {
                guard.refuse_copy(&at, &at).await
            } else {
                guard.refuse(&at).await
            };
            if refusal.is_some() {
                return refusal;
            }
        }
        None
    }

    /// The refusal for the first of `targets` that is sensitive, itself or (`tree`) inside.
    async fn refuse_sensitive_paths<'a>(
        &self,
        panel: Option<PanelId>,
        targets: impl IntoIterator<Item = &'a str>,
        tree: bool,
    ) -> Option<CallToolResult> {
        for target in targets {
            let refusal = if tree {
                refuse_sensitive_tree(&self.context.engine, panel, target).await
            } else {
                refuse_sensitive(&self.context.engine, panel, target).await
            };
            if refusal.is_some() {
                return refusal;
            }
        }
        None
    }
}

#[tool_router(router = properties_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Properties of files and folders: name, kind (file, dir, symlink with target and target_kind), size, modified, created, attributes (readonly, Unix mode, Windows hidden/system/archive), details (accessed, allocated bytes on disk, links, uid/gid), the descript.ion comment, and with count=true each folder's contents (bytes, files, dirs, hard_links counted once, unreadable). Several entries get a total. capabilities says what can be changed there (links, windows_flags, set_created). Reads only metadata, never contents."
    )]
    pub(crate) async fn files_properties(
        &self,
        Parameters(params): Parameters<StatParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::GetProperties {
            panel: params.panel.map(Into::into),
            paths: params.paths,
            count: params.count,
        })
        .await
    }

    #[tool(
        description = "Change attributes and times as Total Commander's Change attributes does: readonly, hidden, system, archive (Windows), mode (Unix, octal), modified and created (Windows) as RFC 3339 or now; omitted fields stay. recursive also changes everything below a folder, apply_to picks files, dirs or both there. Answers changed, failed and the failures with codes; unsupported for what the platform cannot set (nothing changes then). Keys and credential files are refused with sensitive_path; an entry in rCommander's data folder, an SSH folder or an autostart folder, a shell's start-up file - with recursive, anywhere below - with protected_target."
    )]
    pub(crate) async fn files_set_attributes(
        &self,
        Parameters(params): Parameters<SetAttributesParams>,
    ) -> ToolResult {
        self.context
            .engine
            .pin_mcp_panels(async {
                // One panel for the guard and the command, its folders pinned (RC-080-42).
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| self.context.engine.active_panel(), Into::into),
                );
                let change = match change_of(&params) {
                    Ok(change) => change,
                    Err(refusal) => return Ok(refusal),
                };
                if let Some(refusal) = self
                    .refuse_sensitive_paths(panel, params.paths.iter().map(String::as_str), true)
                    .await
                {
                    return Ok(refusal);
                }
                let paths = params.paths.iter().map(String::as_str);
                if let Some(refusal) = self.refuse_protected(panel, paths, params.recursive).await {
                    return Ok(refusal);
                }
                self.dispatch_mcp(Command::SetFileAttributes {
                    panel,
                    paths: params.paths,
                    change,
                    recursive: params.recursive,
                    apply_to: params.apply_to.into(),
                })
                .await
            })
            .await
    }

    #[tool(
        description = "Create a link at link leading to target: shortcut (a Windows .lnk; the name gets .lnk), symlink (on Windows only with Developer Mode or administrator rights, else symlink_privilege - a junction needs neither), hardlink (a file on the same volume) or junction (a local folder, Windows). Never replaces an existing entry (already_exists); unsupported where the file system has no such link. Answers the link, target and kind. Sensitive files and folders are refused with sensitive_path, as target and as the link's place; a link in or to rCommander's data folder, an SSH folder or an autostart folder with protected_target."
    )]
    pub(crate) async fn files_create_link(
        &self,
        Parameters(params): Parameters<CreateLinkParams>,
    ) -> ToolResult {
        self.context
            .engine
            .pin_mcp_panels(async {
                // One panel for the guard and the command, its folders pinned (RC-080-42).
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| self.context.engine.active_panel(), Into::into),
                );
                if let Some(refusal) = self
                    .refuse_sensitive_paths(
                        panel,
                        [params.target.as_str(), params.link.as_str()],
                        true,
                    )
                    .await
                {
                    return Ok(refusal);
                }
                // A link into a protected place is a second way to write there.
                let ends = [params.target.as_str(), params.link.as_str()];
                if let Some(refusal) = self.refuse_protected(panel, ends, false).await {
                    return Ok(refusal);
                }
                self.dispatch_mcp(Command::CreateLink {
                    panel,
                    target: Some(params.target),
                    link: Some(params.link),
                    kind: params.kind.map(Into::into),
                })
                .await
            })
            .await
    }

    #[tool(
        description = "Occupied space of files and folders, streamed through the tree without following links: per path bytes, files, dirs, hard_links (names of a file counted once), unreadable folders and allocated bytes on disk where known; and the total, a hard link shared between paths counted once. Cancelling the call stops the count. Reads only metadata."
    )]
    pub(crate) async fn files_dir_size(
        &self,
        Parameters(params): Parameters<DirSizeParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::DirSize {
            panel: params.panel.map(Into::into),
            paths: params.paths,
        })
        .await
    }

    #[tool(
        description = "Set the comment of a file or folder in its folder's descript.ion, as Total Commander's Ctrl+Z does (the file is kept compatible with Total Commander); an empty comment removes it, and an empty descript.ion is deleted. Answers the path and the comment now stored. files_properties reads comments. An entry in rCommander's data folder, an SSH folder or an autostart folder is refused with protected_target."
    )]
    pub(crate) async fn files_set_comment(
        &self,
        Parameters(params): Parameters<SetCommentParams>,
    ) -> ToolResult {
        self.context
            .engine
            .pin_mcp_panels(async {
                // One panel for the guard and the command, its folders pinned (RC-080-42).
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| self.context.engine.active_panel(), Into::into),
                );
                // The comment is written into the entry's folder: `descript.ion` there is judged
                // too.
                let at = self.context.engine.resolve_target(panel, &params.path);
                let list = at.parent().map(|folder| folder.join("descript.ion"));
                if let Some(refusal) = self
                    .write_guard()
                    .refuse_any([Some(at), list].into_iter().flatten())
                    .await
                {
                    return Ok(refusal);
                }
                self.dispatch_mcp(Command::SetComment {
                    panel,
                    path: Some(params.path),
                    comment: Some(params.comment),
                })
                .await
            })
            .await
    }
}

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;
