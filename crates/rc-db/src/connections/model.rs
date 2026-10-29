//! The connection tree's types, as the repository reads and writes them.
//!
//! No type here holds a secret: a connection names a key *file* at most, and says which secrets
//! are stored ([`SecretFlags`]) without their values.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{SecretFlags, StoredKey, connections::Credential};

/// A connection's id. Never reused once deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionId(pub i64);

/// A shared credential's id (RC-050-12). Never reused once deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialId(pub i64);

/// A folder's id. Never reused once deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderId(pub i64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for CredentialId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for FolderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// An enum stored in a TEXT column under its serde name.
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $text:literal,)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $($(#[$variant_meta])* #[serde(rename = $text)] $variant,)+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &[Self] = &[$(Self::$variant,)+];

            /// The name stored in the database and used by serde.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }

            /// The value stored as `text`.
            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

text_enum! {
    /// The protocol of a connection; for FTP also its encryption (RC-070-01).
    ConnectionKind {
        #[default]
        Ssh = "ssh",
        /// Plain FTP: commands, password and data in clear.
        Ftp = "ftp",
        /// Explicit FTPS: `AUTH TLS` on the FTP port.
        Ftps = "ftps",
        /// Implicit FTPS: TLS from the first byte, usually on port 990.
        FtpsImplicit = "ftps_implicit",
    }
}

impl ConnectionKind {
    /// The port a connection of this kind uses when none is given.
    #[must_use]
    pub fn default_port(self) -> u16 {
        match self {
            Self::Ssh => 22,
            Self::Ftp | Self::Ftps => 21,
            Self::FtpsImplicit => 990,
        }
    }

    /// Whether this is one of the FTP kinds.
    #[must_use]
    pub fn is_ftp(self) -> bool {
        matches!(self, Self::Ftp | Self::Ftps | Self::FtpsImplicit)
    }

    /// Whether a connection of this kind can log in with `method`: FTP with a password or
    /// anonymously, SSH with anything but anonymous.
    #[must_use]
    pub fn accepts(self, method: AuthMethod) -> bool {
        if self.is_ftp() {
            matches!(method, AuthMethod::Password | AuthMethod::Anonymous)
        } else {
            method != AuthMethod::Anonymous
        }
    }
}

text_enum! {
    /// How a connection authenticates. A password, key or passphrase may be stored encrypted
    /// (`crate::SecretKind`) or asked for when connecting.
    AuthMethod {
        /// A password, asked for or stored encrypted.
        #[default]
        Password = "password",
        /// A private key: the file at `key_path`, or a stored key.
        Key = "key",
        /// ssh-agent or Pageant.
        Agent = "agent",
        /// Keyboard-interactive: the server asks, the person answers.
        Interactive = "interactive",
        /// Anonymous FTP: user `anonymous`, no password stored or asked for.
        Anonymous = "anonymous",
    }
}

text_enum! {
    /// What a panel shows when the connection opens.
    DefaultView {
        /// The remote files.
        #[default]
        Files = "files",
        /// A terminal on the remote host.
        Terminal = "terminal",
    }
}

impl DefaultView {
    /// Whether a connection of `kind` can open in this view: FTP has no terminal.
    #[must_use]
    pub fn fits(self, kind: ConnectionKind) -> bool {
        self == Self::Files || !kind.is_ftp()
    }
}

text_enum! {
    /// How an FTP connection opens its data connections.
    FtpDataMode {
        /// The client connects to the server (`EPSV` where offered, `PASV` otherwise).
        #[default]
        Passive = "passive",
        /// The server connects back to the client (`PORT`/`EPRT`).
        Active = "active",
    }
}

text_enum! {
    /// How an FTP connection encodes names on a server that does not offer UTF-8; one that
    /// offers it always gets UTF-8.
    FtpEncoding {
        /// ISO 8859-1, what old Unix and NAS servers mostly send.
        #[default]
        Latin1 = "latin1",
        /// UTF-8 although the server does not announce it.
        Utf8 = "utf8",
    }
}

/// One connection of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    /// The folder it sits in; `None` at the root.
    pub folder_id: Option<FolderId>,
    pub name: String,
    pub kind: ConnectionKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub auth_method: AuthMethod,
    /// A private key file to use: a path, never the key itself.
    pub key_path: Option<String>,
    /// The login script to run after connecting (RC-050-08).
    pub login_script_id: Option<i64>,
    pub default_view: DefaultView,
    /// The directory a session starts in; the server's default when `None`.
    pub start_directory: Option<String>,
    pub notes: Option<String>,
    /// FTP only (`None` for SSH, and the default when not set): passive or active data
    /// connections, and the name encoding without UTF-8.
    pub ftp_data_mode: Option<FtpDataMode>,
    pub ftp_encoding: Option<FtpEncoding>,
    /// Its place among its folder's connections, smallest first.
    pub sort_order: i64,
    /// RFC 3339 in UTC, as SQLite writes it.
    pub created_at: String,
    pub updated_at: String,
    /// Which secrets are stored (`has_password`, `has_key`, `has_passphrase`); never a value.
    #[serde(flatten)]
    pub secrets: SecretFlags,
    /// The private key stored in the vault, by its public half; `None` when there is none
    /// (`key_path` may still name a key file).
    pub key: Option<StoredKey>,
    /// The shared credential it takes its secrets from, and its user when it names none
    /// (RC-050-12); `None` uses its own.
    pub credential_id: Option<CredentialId>,
}

/// One folder of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionFolder {
    pub id: FolderId,
    /// The folder it sits in; `None` at the root.
    pub parent_id: Option<FolderId>,
    pub name: String,
    /// Its place among its parent's folders, smallest first.
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A new connection. Optional text that is empty or blank is stored as none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionDraft {
    pub name: String,
    #[serde(default)]
    pub kind: ConnectionKind,
    pub host: String,
    /// 1 to 65535; the kind's default port when `None`. Wider than `u16` so that 0 or 70000
    /// reach the validation and its `port_invalid` instead of failing to parse.
    #[serde(default)]
    pub port: Option<i64>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub default_view: DefaultView,
    #[serde(default)]
    pub start_directory: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// FTP only; dropped for SSH.
    #[serde(default)]
    pub ftp_data_mode: Option<FtpDataMode>,
    #[serde(default)]
    pub ftp_encoding: Option<FtpEncoding>,
}

impl ConnectionDraft {
    /// A draft with every optional field at its default.
    #[must_use]
    pub fn new(name: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ConnectionKind::default(),
            host: host.into(),
            port: None,
            username: None,
            auth_method: AuthMethod::default(),
            key_path: None,
            default_view: DefaultView::default(),
            start_directory: None,
            notes: None,
            ftp_data_mode: None,
            ftp_encoding: None,
        }
    }
}

/// A change to a connection: `None` keeps a field. For the optional text fields (`username`,
/// `key_path`, `start_directory`, `notes`) an empty string clears the value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ConnectionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<AuthMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_view: Option<DefaultView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ftp_data_mode: Option<FtpDataMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ftp_encoding: Option<FtpEncoding>,
}

/// The whole tree: the root's folders, connections and shared credentials, each folder with its
/// own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionTree {
    pub folders: Vec<FolderNode>,
    pub connections: Vec<Connection>,
    /// By name (RC-050-12).
    #[serde(default)]
    pub credentials: Vec<Credential>,
    /// The connection the window logged in to last, where the manager's cursor starts
    /// (RC-050-15); the engine fills it in, the database's tree leaves it `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<ConnectionId>,
}

/// A folder with what it holds, in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderNode {
    #[serde(flatten)]
    pub folder: ConnectionFolder,
    pub folders: Vec<FolderNode>,
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub credentials: Vec<Credential>,
}

/// What deleting a folder removed, the folder itself included.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderDeletion {
    pub folders: u64,
    pub connections: u64,
}
