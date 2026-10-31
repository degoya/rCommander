//! Validation at the repository's boundary: every write passes through here first, whoever
//! calls (the GUI, MCP, a later import). The tables' CHECK constraints are the last line behind
//! it, not the first.
//!
//! Lengths are capped here too (RC-080-47), in characters: a name, a host and a user name at
//! [`MAX_TEXT_CHARS`] (`text_too_long`), a key path and a start directory at [`MAX_PATH_CHARS`]
//! (`path_too_long`). An imported file can no longer put a megabyte into a row.

use crate::connections::{
    AuthMethod, Connection, ConnectionDraft, ConnectionError, ConnectionKind, ConnectionPatch,
    DefaultView, FtpDataMode, FtpEncoding,
};

/// The longest name, host or user name, in characters; a DNS name has at most 253.
pub const MAX_TEXT_CHARS: usize = 255;

/// The longest key path or start directory, in characters.
pub const MAX_PATH_CHARS: usize = 4096;

/// The longest theme id, as the `themes` table allows it (RC-100-51).
pub const MAX_THEME_ID_CHARS: usize = 64;

/// Refuses `text` of `field` when it is longer than [`MAX_TEXT_CHARS`].
fn short(field: &'static str, text: &str) -> Result<(), ConnectionError> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(ConnectionError::TextTooLong {
            field,
            max: MAX_TEXT_CHARS,
        });
    }
    Ok(())
}

/// Refuses the path `text` of `field` when it is longer than [`MAX_PATH_CHARS`].
fn path(field: &'static str, text: &str) -> Result<(), ConnectionError> {
    if text.chars().count() > MAX_PATH_CHARS {
        return Err(ConnectionError::PathTooLong {
            field,
            max: MAX_PATH_CHARS,
        });
    }
    Ok(())
}

/// A name: trimmed, not empty, at most [`MAX_TEXT_CHARS`], no control characters.
pub(crate) fn name(raw: &str) -> Result<String, ConnectionError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ConnectionError::NameEmpty);
    }
    short("name", name)?;
    plain("name", name)?;
    Ok(name.to_owned())
}

/// Refuses `text` when it holds a control character: a name, a user or a folder is shown, sent
/// to a server or typed into a shell (`cd`) as it is.
fn plain(field: &'static str, text: &str) -> Result<(), ConnectionError> {
    if text.chars().any(char::is_control) {
        return Err(ConnectionError::ControlCharacter { field });
    }
    Ok(())
}

/// Optional text of `field` ([`optional`]) without control characters.
fn optional_plain(
    field: &'static str,
    raw: Option<&str>,
) -> Result<Option<String>, ConnectionError> {
    let text = optional(raw);
    if let Some(text) = &text {
        plain(field, text)?;
    }
    Ok(text)
}

/// A user name: optional, trimmed, at most [`MAX_TEXT_CHARS`], without control characters.
pub(crate) fn user(raw: Option<&str>) -> Result<Option<String>, ConnectionError> {
    let user = optional_plain("user name", raw)?;
    if let Some(user) = &user {
        short("user name", user)?;
    }
    Ok(user)
}

/// A host name or address: trimmed, not empty, at most [`MAX_TEXT_CHARS`], no whitespace or
/// control characters inside.
pub(crate) fn host(raw: &str) -> Result<String, ConnectionError> {
    let host = raw.trim();
    if host.is_empty() {
        return Err(ConnectionError::HostEmpty);
    }
    short("host", host)?;
    if host
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(ConnectionError::HostInvalid);
    }
    Ok(host.to_owned())
}

/// A port: 1 to 65535.
pub(crate) fn port(raw: i64) -> Result<u16, ConnectionError> {
    u16::try_from(raw)
        .ok()
        .filter(|port| *port != 0)
        .ok_or(ConnectionError::PortInvalid { port: raw })
}

/// A theme's id (RC-100-51): optional, trimmed, at most [`MAX_THEME_ID_CHARS`], no control
/// characters. Whether it names a theme of its mode is the engine's to judge.
pub(crate) fn theme(raw: Option<&str>) -> Result<Option<String>, ConnectionError> {
    let id = optional_plain("theme", raw)?;
    if id
        .as_ref()
        .is_some_and(|id| id.chars().count() > MAX_THEME_ID_CHARS)
    {
        return Err(ConnectionError::TextTooLong {
            field: "theme",
            max: MAX_THEME_ID_CHARS,
        });
    }
    Ok(id)
}

/// Optional text: trimmed, and none when nothing is left.
pub(crate) fn optional(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The editable columns of a connection, validated, as they are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fields {
    pub(crate) name: String,
    pub(crate) kind: ConnectionKind,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: Option<String>,
    pub(crate) auth_method: AuthMethod,
    pub(crate) key_path: Option<String>,
    pub(crate) default_view: DefaultView,
    pub(crate) start_directory: Option<String>,
    pub(crate) notes: Option<String>,
    pub(crate) ftp_data_mode: Option<FtpDataMode>,
    pub(crate) ftp_encoding: Option<FtpEncoding>,
    pub(crate) theme_light: Option<String>,
    pub(crate) theme_dark: Option<String>,
}

impl Fields {
    /// The columns of a new connection.
    pub(crate) fn from_draft(draft: &ConnectionDraft) -> Result<Self, ConnectionError> {
        Self {
            name: name(&draft.name)?,
            kind: draft.kind,
            host: host(&draft.host)?,
            port: match draft.port {
                Some(raw) => port(raw)?,
                None => draft.kind.default_port(),
            },
            username: optional_plain("user name", draft.username.as_deref())?,
            auth_method: draft.auth_method,
            key_path: optional(draft.key_path.as_deref()),
            default_view: draft.default_view,
            start_directory: optional_plain("start directory", draft.start_directory.as_deref())?,
            notes: optional(draft.notes.as_deref()),
            ftp_data_mode: draft.ftp_data_mode,
            ftp_encoding: draft.ftp_encoding,
            theme_light: theme(draft.theme_light.as_deref())?,
            theme_dark: theme(draft.theme_dark.as_deref())?,
        }
        .consistent()
    }

    /// The columns of `current` with `patch` applied.
    pub(crate) fn patched(
        current: &Connection,
        patch: &ConnectionPatch,
    ) -> Result<Self, ConnectionError> {
        let text = |patched: &Option<String>, current: &Option<String>| match patched {
            Some(value) => optional(Some(value.as_str())),
            None => current.clone(),
        };
        // A stored value is checked again: one written before the check is refused at its next
        // change, not typed into a shell.
        let plain_text = |field, patched: &Option<String>, current: &Option<String>| {
            let value = text(patched, current);
            if let Some(value) = &value {
                plain(field, value)?;
            }
            Ok::<_, ConnectionError>(value)
        };
        Self {
            name: name(patch.name.as_deref().unwrap_or(&current.name))?,
            kind: patch.kind.unwrap_or(current.kind),
            host: host(patch.host.as_deref().unwrap_or(&current.host))?,
            port: match patch.port {
                Some(raw) => port(raw)?,
                None => current.port,
            },
            username: plain_text("user name", &patch.username, &current.username)?,
            auth_method: patch.auth_method.unwrap_or(current.auth_method),
            key_path: text(&patch.key_path, &current.key_path),
            default_view: patch.default_view.unwrap_or(current.default_view),
            start_directory: plain_text(
                "start directory",
                &patch.start_directory,
                &current.start_directory,
            )?,
            notes: text(&patch.notes, &current.notes),
            ftp_data_mode: patch.ftp_data_mode.or(current.ftp_data_mode),
            ftp_encoding: patch.ftp_encoding.or(current.ftp_encoding),
            theme_light: theme(text(&patch.theme_light, &current.theme_light).as_deref())?,
            theme_dark: theme(text(&patch.theme_dark, &current.theme_dark).as_deref())?,
        }
        .consistent()
    }

    /// These fields, if the kind can log in with the auth method (FTP with a password: as a
    /// named user) and open in the default view, and the user name and paths are not too long
    /// (the name and host are checked as they are read).
    /// The FTP options are dropped for a kind that is not FTP.
    fn consistent(mut self) -> Result<Self, ConnectionError> {
        if let Some(user) = &self.username {
            short("user name", user)?;
        }
        if let Some(key_path) = &self.key_path {
            path("key path", key_path)?;
        }
        if let Some(start) = &self.start_directory {
            path("start directory", start)?;
        }
        if !self.kind.accepts(self.auth_method) {
            return Err(ConnectionError::AuthUnsupported {
                kind: self.kind,
                auth_method: self.auth_method,
            });
        }
        if self.kind.is_ftp() && self.auth_method == AuthMethod::Password && self.username.is_none()
        {
            return Err(ConnectionError::UsernameEmpty);
        }
        if !self.default_view.fits(self.kind) {
            return Err(ConnectionError::ViewUnsupported { kind: self.kind });
        }
        if !self.kind.is_ftp() {
            self.ftp_data_mode = None;
            self.ftp_encoding = None;
        }
        Ok(self)
    }
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
