//! The refusals and read failures of the imports: an ssh_config, Total Commander's
//! `wincmd.ini` and `wcx_ftp.ini`, an export of Remote Desktop Manager. Split off
//! [`EngineError`](super::EngineError) in RC-110-35; the codes are in `codes.rs`.

/// An import refused its file or could not read it; each variant keeps its own code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportError {
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
}
