//! Why an SSH, SFTP or FTP session request was refused: no fitting connection, or a request
//! out of bounds. Split off [`EngineError`](super::EngineError) in RC-110-35; the codes are in
//! `codes.rs`.

/// A session request refused; each variant keeps its own code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// An SSH command on a connection of another kind.
    #[error("connection {id} is not an SSH connection")]
    ConnectionNotSsh { id: crate::ConnectionId },
    /// An `ssh.exec` request out of bounds: an empty or overlong command, a NUL, a limit.
    #[error("invalid command request: {reason}")]
    ExecInvalid { reason: String },
    /// SSH session settings that cannot be stored.
    #[error("invalid session settings: {reason}")]
    SessionPrefsInvalid { reason: String },
    /// An `ftp://` location names no FTP connection, or a connection that is not one.
    #[error("there is no FTP connection named {connection:?}")]
    FtpConnectionNotFound { connection: String },
    /// An `ftp://` location's name fits FTP connections in several folders.
    #[error("{count} FTP connections are named {connection:?}; rename one to reach it by name")]
    FtpConnectionAmbiguous { connection: String, count: usize },
    /// An `sftp://` location names no SSH connection.
    #[error("there is no SSH connection named {connection:?}")]
    SftpConnectionNotFound { connection: String },
    /// An `sftp://` location's name fits SSH connections in several folders.
    #[error("{count} SSH connections are named {connection:?}; rename one to reach it by name")]
    SftpConnectionAmbiguous { connection: String, count: usize },
}
