//! rCommander's remote side. The SSH host-key trust store: what a server presented, the verdict
//! on it against `rcommander.db` (trusted, unknown, changed), trusting and replacing a key, and
//! the import of OpenSSH's `known_hosts`; the SSH agents ([`agent`]: `ssh-agent`, the Windows
//! OpenSSH agent service, Pageant), whose keys it lists and whose signer the login uses; and the
//! private keys a person takes into the vault ([`private_key`]: checked, described, opened again
//! for a session), with the one table of what marks key material ([`key_markers`]). [`ftp`] is
//! FTP and FTPS as a file system ([`FtpVfs`]), [`sftp`] SFTP over the channels of an SSH session
//! ([`SftpVfs`]). [`login_script`] is the
//! model and the engine of login scripts, played against any byte stream. [`ssh`] is the SSH
//! session (RC-060-01): it checks every server key through a [`ssh::HostKeyGate`] - the engine's
//! is [`trust::verify`] with a question for a person - before it logs in, and carries the
//! channels SFTP, the terminal and commands run on. `testkit` (tests, feature `testkit`) is an
//! in-process SSH server and its fixtures.

pub mod agent;
pub mod ftp;
mod hashed;
pub mod hostkey;
pub mod key_markers;
pub mod known_hosts_import;
mod listed_name;
pub mod login_script;
pub mod private_key;
pub mod sftp;
pub mod ssh;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
mod tree;
pub mod trust;
mod wire;

pub use agent::{
    AGENT_FAILED, AGENT_NOT_FOUND, AGENT_TIMEOUT, AgentError, AgentIdentity, AgentSource,
    AgentSourceKind, WindowsAgents,
};
pub use ftp::{FtpError, FtpVfs};
pub use hostkey::{
    HOST_KEY_CHANGED, HOST_KEY_INVALID, HOST_KEY_UNKNOWN, HOST_KEY_UNSUPPORTED, KeyError,
    OfferedKey, Verdict, describe,
};
pub use private_key::{KeyFormat, KeyInfo, MAX_KEY_BYTES, PrivateKeyError};
pub use sftp::{SftpError, SftpVfs};
pub use ssh::{SshError, SshSession};
pub use trust::TrustError;
