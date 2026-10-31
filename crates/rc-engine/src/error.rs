//! The engine's errors, each with a stable code.

use rc_checksum::ChecksumError;
use rc_db::connections::ConnectionError;
use rc_db::login_scripts::LoginScriptError;
use rc_db::{BackupError, DbError, RetentionError};
use rc_ops::OpsError;
use rc_remote::login_script::ScriptInvalid;
use rc_remote::{AgentError, FtpError, SftpError, SshError};
use rc_rename::{RenameError, UndoMismatch};
use rc_term::TermError;
use rc_vault::VaultError;
use rc_vfs::VfsError;

use crate::ConnectionId;

mod codes;

/// What went wrong running a command.
///
/// [`EngineError::code`] is the stable machine-readable form: the GUI looks its message up under
/// that key in the i18n catalogue, the MCP server returns it to agents. A code never changes
/// once released; a file-system failure keeps the code of its [`VfsError`], a refusal of the
/// connection tree that of its [`ConnectionError`], a refused or failed operation that of its
/// [`OpsError`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// The file system refused or failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// The operations queue refused or failed.
    #[error(transparent)]
    Ops(#[from] OpsError),
    /// A name given for a panel entry is not among its entries.
    #[error("no entry named {name:?} in the panel")]
    EntryNotFound { name: String },
    /// A cursor index past the panel's last entry.
    #[error("index {index} is outside the panel's {len} entries")]
    CursorOutOfRange { index: usize, len: usize },
    /// No file system is open for the location's backend.
    #[error("no file system is open for this location")]
    BackendUnavailable,
    /// A command needs a secret and the vault is locked; only the GUI can unlock it.
    #[error("the vault is locked; only the person can unlock it, in the rCommander window")]
    VaultLocked,
    /// A terminal refused or failed; keeps the code of its [`TermError`].
    #[error(transparent)]
    Term(#[from] TermError),
    /// No database is attached to the engine ([`crate::Engine::attach_db`]).
    #[error("the database is not open")]
    DatabaseUnavailable,
    /// The connection tree refused the change or its database failed.
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    /// A [`crate::CommandClass::GuiOnly`] command reached [`crate::Engine::dispatch_mcp`].
    #[error("{command} is a decision for a person in the rCommander window; MCP cannot run it")]
    GuiOnly { command: &'static str },
    /// The database failed.
    #[error(transparent)]
    Database(#[from] DbError),
    /// The SSH server's key is not trusted yet. The fingerprint is public - it is what a person
    /// compares - and only the app can confirm it.
    #[error(
        "the SSH server {host}:{port} is not trusted yet ({algorithm} {fingerprint}); a person \
         has to confirm its key in the rCommander window"
    )]
    HostKeyUnknown {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    /// The SSH server presented another key than the stored one: stopped. Only the app can
    /// replace the stored key, by an action of its own.
    #[error(
        "the SSH server {host}:{port} presented another {algorithm} key ({fingerprint}) than the \
         stored one ({stored_fingerprint}); the connection is stopped, only a person can replace \
         the stored key in the rCommander window"
    )]
    HostKeyChanged {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        stored_fingerprint: String,
    },
    /// The stored key changed since the question was asked; nothing was replaced.
    #[error("the stored host key changed since it was shown")]
    HostKeyStale,
    /// No open host-key question has this id (answered, replaced or never asked).
    #[error("no open host key question {question}")]
    HostKeyQuestionNotFound { question: u64 },
    /// `hostkeys.replace` on a question about an unknown key, which has nothing to replace.
    #[error("host key question {question} is about an unknown key, not a changed one")]
    HostKeyNotChanged { question: u64 },
    /// No stored host key has this id.
    #[error("no stored host key {id}")]
    KnownHostNotFound { id: i64 },
    /// A `known_hosts` file to import cannot be read.
    #[error("cannot read {path}: {source}")]
    KnownHostsUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A backup or a restore failed; keeps the code of its [`BackupError`].
    #[error(transparent)]
    Backup(#[from] BackupError),
    /// An export or an import of settings and connections failed (RC-080-19); keeps the code of
    /// its [`crate::exchange::ExchangeError`].
    #[error(transparent)]
    Exchange(#[from] crate::exchange::ExchangeError),
    /// `diagnostics.set_retention` named a table without retention limits.
    #[error("{table} is not a table with retention limits")]
    RetentionUnknownTable { table: String },
    /// `diagnostics.set_retention` asked for a limit outside the table's floor and ceiling.
    #[error("{key} must be between {min} and {max}, not {value}")]
    RetentionOutOfRange {
        key: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },
    /// No SSH agent answered; keeps the code of its [`AgentError`].
    #[error(transparent)]
    Agent(#[from] AgentError),
    /// The vault refused or failed; keeps the code of its [`VaultError`] (a locked vault is
    /// [`EngineError::VaultLocked`]).
    #[error(transparent)]
    Vault(VaultError),
    /// A new master password shorter than the minimum.
    #[error("the master password needs at least {min} characters")]
    MasterPasswordTooShort { min: usize },
    /// An empty password, key or passphrase to store.
    #[error("there is nothing to store: the secret is empty")]
    SecretEmpty,
    /// Not an OpenSSH, PEM, PKCS#8 or PuTTY private key, or a damaged one.
    #[error("this is not an OpenSSH, PEM, PKCS#8 or PuTTY private key")]
    KeyFormatUnknown,
    /// The key file is encrypted and no passphrase was given.
    #[error("the private key is encrypted; enter its passphrase")]
    KeyPassphraseNeeded,
    /// The passphrase does not open the key file.
    #[error("the passphrase does not open this private key")]
    KeyPassphraseWrong,
    /// A private key is taken into the vault from its file (`secrets.import_key`), checked
    /// with its passphrase, never set as text.
    #[error("a private key is taken into the vault from its file, with secrets.import_key")]
    KeyImportOnly,
    /// A key file to import cannot be read.
    #[error("cannot read {path}: {source}")]
    KeyFileUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A key file to take into the vault is not a local file: a share, a device path, a URL or
    /// a relative path. Only a drive or absolute path, or a WSL distribution's share, is read.
    #[error("{path} is not a key file on this computer; only a local one is taken into the vault")]
    KeyPathRefused { path: String },
    /// A file to import is not `~/.ssh/config` or below `~/.ssh/config.d/` (the user's or a
    /// WSL distribution's), by its canonical path.
    #[error("{path} is not ~/.ssh/config or a file in ~/.ssh/config.d; only those are imported")]
    SshConfigPathRefused { path: String },
    /// The ssh_config, or a file it includes, is or holds a key: nothing was imported.
    #[error("{path} is key material, not an SSH configuration; nothing was imported")]
    SshConfigKeyMaterial { path: String },
    /// The ssh_config to import cannot be read.
    #[error("cannot read {path}: {source}")]
    SshConfigUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A host to import is not in the ssh_config.
    #[error("the ssh config has no host {alias:?}")]
    SshConfigHostUnknown { alias: String },
    /// Over MCP, a Total Commander file to import is not named `wincmd.ini` or `wcx_ftp.ini`,
    /// or the read guard refuses it; the same answer whether it exists or not.
    #[error("{path} is not a wincmd.ini or wcx_ftp.ini that may be read")]
    TcIniPathRefused { path: String },
    /// A Total Commander file to import cannot be read.
    #[error("cannot read {path}: {source}")]
    TcIniUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A Total Commander file to import is larger than the import reads.
    #[error("{path} is larger than {max} bytes; it is not read")]
    TcIniTooLarge { path: String, max: u64 },
    /// Neither `wincmd.ini` nor `wcx_ftp.ini` was found.
    #[error("no wincmd.ini or wcx_ftp.ini of Total Commander was found; name the files")]
    TcIniNotFound,
    /// Over MCP, an export of Remote Desktop Manager to import does not end in `.rdm`, or the
    /// read guard refuses it; the same answer whether it exists or not.
    #[error("{path} is not a .rdm file that may be read")]
    RdmPathRefused { path: String },
    /// The export cannot be read, or is not a regular file.
    #[error("cannot read {path}: {source}")]
    RdmUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The export is larger than the import reads.
    #[error("{path} is larger than {max} bytes; it is not read")]
    RdmTooLarge { path: String, max: u64 },
    /// The export is protected with a master key (or is no XML at all): not read.
    #[error("{path} is encrypted with a master key; export again without one")]
    RdmEncrypted { path: String },
    /// The export is not well-formed XML, or declares a DTD.
    #[error("{path} is not valid XML (line {line}) or declares a DTD")]
    RdmInvalid { path: String, line: u32 },
    /// The file is XML, but no export of Remote Desktop Manager.
    #[error("{path} is not an export of Remote Desktop Manager")]
    RdmNotAnExport { path: String },
    /// A search cannot run as asked: a pattern that does not compile, an empty content text,
    /// a cursor of another search, too many start folders.
    #[error("invalid search: {reason}")]
    InvalidSearch { reason: String },
    /// The caller cancelled a long-running comparison.
    #[error("cancelled")]
    Cancelled,
    /// `compare.files` without paths found no two files: neither two selected files in the
    /// active panel nor a file under each panel's cursor.
    #[error("select two files, or put each panel's cursor on a file, to compare them")]
    CompareNeedsFiles,
    /// A synchronization would delete and the deletes were not confirmed; nothing was done.
    #[error("the synchronization deletes {count} entries; confirm the deletes to run it")]
    SyncDeletesUnconfirmed { count: usize },
    /// A synchronization row does not exist or cannot take the action.
    #[error("row {row} of the synchronization cannot take the action {action:?}")]
    SyncActionInvalid {
        row: usize,
        action: crate::sync::SyncAction,
    },
    /// `sync.apply` names a path the comparison has no row for.
    #[error("the synchronization has no row {path}")]
    SyncRowNotFound { path: String },
    /// An MCP dispatch would read a file whose contents never leave through MCP (a key, a
    /// credential file, rCommander's own data; `read_guard.rs`); nothing of it was read.
    #[error(
        "{path} is {reason}: its contents are never shown through MCP. A person can open it in \
         rCommander."
    )]
    SensitivePath { path: String, reason: &'static str },
    /// An MCP command would change where a connection with stored secrets connects (host,
    /// port, kind or user): the secrets would go to the new endpoint. Only a person changes it,
    /// in the window.
    #[error(
        "connection {id} has stored secrets: its host, port, kind and user change only in \
         rCommander's window"
    )]
    EndpointChangeNeedsGui { id: i64 },
    /// Hashing, a checksum file, split or combine failed; keeps the code of its
    /// [`ChecksumError`].
    #[error(transparent)]
    Checksum(#[from] ChecksumError),
    /// `file.open`: the system could not start or open the file.
    #[error("the system could not open {name:?}")]
    OpenFailed { name: String },
    /// `panel.change_drive` named no entry of the drive bar.
    #[error("there is no drive {drive:?} in the drive bar")]
    DriveNotFound { drive: String },
    /// The drive has no medium, cannot be reached or did not answer in time; the panel stays.
    #[error("drive {drive} is not ready")]
    DriveNotReady { drive: String },
    /// No setting has this key.
    #[error("{key} is not a setting")]
    SettingUnknown { key: String },
    /// Only a person in the rCommander window changes this setting (MCP and vault keys).
    #[error("{key} can only be changed in the rCommander window")]
    SettingProtected { key: String },
    /// The value does not fit the setting.
    #[error("{key}: {reason}")]
    SettingInvalid { key: String, reason: String },
    /// A multi-rename rule that does not work, or a plan with conflicts (nothing was renamed);
    /// keeps the code of its [`RenameError`].
    #[error(transparent)]
    Rename(#[from] RenameError),
    /// A multi-rename or its undo is still queued or running.
    #[error("a multi-rename is still running; wait until it has finished")]
    RenameBusy,
    /// The rule gives every entry the name it has.
    #[error("the rule changes no name; nothing was renamed")]
    RenameNoChanges,
    /// The undo log holds no run with a step carried out.
    #[error("there is no multi-rename to undo")]
    RenameNothingToUndo,
    /// The latest run is no longer whole in the undo log (cut by its retention limit) or cannot
    /// be read; it is not undone in part.
    #[error("the latest multi-rename is no longer complete in the undo log and cannot be undone")]
    RenameUndoIncomplete,
    /// Entries of the latest run were renamed, removed or taken since; nothing was undone.
    #[error(
        "{} entries no longer fit the undo, nothing was renamed: {}",
        .mismatches.len(),
        describe_mismatches(.mismatches)
    )]
    RenameUndoMismatch { mismatches: Vec<UndoMismatch> },
    /// A multi-rename preset name that cannot be used, or one preset too many.
    #[error("the preset is refused: {reason}")]
    RenamePresetInvalid { reason: String },
    /// No multi-rename preset has this name.
    #[error("there is no preset named {name}")]
    RenamePresetNotFound { name: String },
    /// The login scripts refused the change or their database failed; keeps the code of its
    /// [`LoginScriptError`].
    #[error(transparent)]
    LoginScript(#[from] LoginScriptError),
    /// A login script breaks a bound or does not read; keeps the code of its [`ScriptInvalid`].
    #[error(transparent)]
    LoginScriptInvalid(#[from] ScriptInvalid),
    /// An SSH session failed; keeps the code of its [`SshError`] (a locked vault, a refused host
    /// key, a key that does not open and a cancel are the engine's own variants,
    /// `sessions/sources.rs`).
    #[error(transparent)]
    Ssh(SshError),
    /// An SSH command on a connection of another kind.
    #[error("connection {id} is not an SSH connection")]
    ConnectionNotSsh { id: crate::ConnectionId },
    /// An `ssh.exec` request out of bounds: an empty or overlong command, a NUL, a limit.
    #[error("invalid command request: {reason}")]
    ExecInvalid { reason: String },
    /// SSH session settings that cannot be stored.
    #[error("invalid session settings: {reason}")]
    SessionPrefsInvalid { reason: String },
    /// No FTP session came about (unreachable, login refused, untrusted certificate); keeps the
    /// code of its [`FtpError`].
    #[error(transparent)]
    Ftp(#[from] FtpError),
    /// The connection logs in with a password and none is stored; only the person stores it,
    /// in rCommander.
    #[error(
        "no password is stored for connection {connection}; the person stores it in rCommander"
    )]
    PasswordNotStored { connection: ConnectionId },
    /// An `ftp://` location names no FTP connection, or a connection that is not one.
    #[error("there is no FTP connection named {connection:?}")]
    FtpConnectionNotFound { connection: String },
    /// An `ftp://` location's name fits FTP connections in several folders.
    #[error("{count} FTP connections are named {connection:?}; rename one to reach it by name")]
    FtpConnectionAmbiguous { connection: String, count: usize },
    /// No MCP question with this id waits (answered, timed out, given up or never asked).
    #[error("no open MCP question {question}")]
    McpQuestionNotFound { question: u64 },
    /// "Allow for this session" answers only a question that offered it (RC-010-27).
    #[error("MCP question {question} cannot be allowed for the session; it did not offer that")]
    McpAnswerNotOffered { question: u64 },
    /// A tab, folder-history or hotlist command was refused; keeps the code of its
    /// [`crate::TabsError`].
    #[error(transparent)]
    Tabs(crate::TabsError),
    /// A command line that does not run: blank, too long or with a control character. The line
    /// itself is never repeated - it may hold a password.
    #[error("the command line cannot run: {reason}")]
    CommandLineInvalid { reason: &'static str },
    /// A command line outside a local folder: inside an archive or on a server.
    #[error("the command line runs only in a local folder")]
    CommandLineLocalOnly,
    /// The system's shell did not start.
    #[error("the command line's shell did not start: {source}")]
    CommandLineFailed {
        #[source]
        source: std::io::Error,
    },
    /// A quick filter that is too long or holds a control character.
    #[error("invalid filter: {reason}")]
    FilterInvalid { reason: &'static str },
    /// A change inside an archive: archives are read-only in a panel; unpack to change one.
    #[error("archives are read-only here; unpack the archive to change its contents")]
    ArchiveReadOnly,
    /// The file is no archive rCommander reads.
    #[error("{name} is not an archive rCommander reads")]
    NotAnArchive { name: String },
    /// A properties command (RC-020-06) misses what it needs or got what it cannot take: a
    /// link without its place, a comment that is too long.
    #[error("{reason}")]
    PropertyInvalid { reason: &'static str },
    /// No SFTP session came about on an SSH connection (no subsystem, a timeout, a lost
    /// channel); keeps the code of its [`SftpError`].
    #[error(transparent)]
    Sftp(#[from] SftpError),
    /// An `sftp://` location names no SSH connection.
    #[error("there is no SSH connection named {connection:?}")]
    SftpConnectionNotFound { connection: String },
    /// An `sftp://` location's name fits SSH connections in several folders.
    #[error("{count} SSH connections are named {connection:?}; rename one to reach it by name")]
    SftpConnectionAmbiguous { connection: String, count: usize },
    /// An FTPS certificate refused, or a decision about one that failed; keeps the code of its
    /// [`crate::CertificateError`].
    #[error(transparent)]
    Certificate(crate::CertificateError),
    /// A network drive refused or failed (RC-010-16); keeps the code of its
    /// [`crate::NetworkDriveError`].
    #[error(transparent)]
    NetworkDrive(crate::NetworkDriveError),
    /// No saved transfer list has this id (RC-010-15).
    #[error("no saved queue list {saved}")]
    SavedQueueNotFound { saved: i64 },
    /// The list holds no job that can be saved: copies and moves are.
    #[error("the queue list holds no copy or move to save")]
    QueueListEmpty,
    /// A `plugins.*` command failed (RC-100-09); keeps the code of its [`crate::PluginAdminError`].
    #[error(transparent)]
    Plugin(crate::PluginAdminError),
    /// A secret source failed or a reference to one was refused (RC-100-34); keeps the code of
    /// its [`crate::SecretSourceError`].
    #[error(transparent)]
    SecretSource(crate::SecretSourceError),
    /// An update command was refused or failed (RC-100-05, RC-100-06); keeps the code of its
    /// [`crate::UpdateRefusal`].
    #[error(transparent)]
    Update(crate::UpdateRefusal),
    /// No colour theme has this id, or none this system offers (RC-100-50).
    #[error("there is no theme {id:?}")]
    ThemeUnknown { id: String },
    /// The theme is of the other mode than the one it was chosen or saved for.
    #[error("theme {id:?} is not a {mode} theme")]
    ThemeWrongMode { id: String, mode: &'static str },
    /// A built-in theme is neither changed nor deleted; Duplicate makes an own one of it.
    #[error("theme {id:?} is built in; duplicate it to change it")]
    ThemeBuiltin { id: String },
    /// A mode draws with the theme; another is chosen before it is deleted.
    #[error("theme {id:?} is chosen; choose another before deleting it")]
    ThemeInUse { id: String },
    /// Another own theme has this name (case does not count).
    #[error("another theme is named {name:?}")]
    ThemeNameTaken { name: String },
    /// A theme to save that does not fit: no name, an unknown token, a colour no `#rrggbb`.
    #[error("the theme does not fit: {reason}")]
    ThemeInvalid { reason: String },
    /// An MCP call the person allowed found a panel in another folder than the question showed
    /// (RC-080-34): the names it carries would mean other entries there.
    #[error(
        "the {panel} panel shows another folder than when the person was asked; nothing was done. \
         Call again, and the person is asked about the folder it shows now"
    )]
    PanelChanged { panel: &'static str },
}

/// The first few mismatches as `from -> to (problem)`.
fn describe_mismatches(mismatches: &[UndoMismatch]) -> String {
    const SHOWN: usize = 5;
    let mut text: Vec<String> = mismatches
        .iter()
        .take(SHOWN)
        .map(|mismatch| {
            let problem = match mismatch.problem {
                rc_rename::UndoProblem::SourceMissing => "gone or renamed since",
                rc_rename::UndoProblem::TargetOccupied => "old name taken",
            };
            format!("{} -> {} ({problem})", mismatch.step.from, mismatch.step.to)
        })
        .collect();
    if mismatches.len() > SHOWN {
        text.push(format!("and {} more", mismatches.len() - SHOWN));
    }
    text.join(", ")
}

impl From<RetentionError> for EngineError {
    fn from(error: RetentionError) -> Self {
        match error {
            RetentionError::UnknownTable { table } => Self::RetentionUnknownTable { table },
            RetentionError::OutOfRange {
                key,
                value,
                min,
                max,
            } => Self::RetentionOutOfRange {
                key,
                value,
                min,
                max,
            },
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
