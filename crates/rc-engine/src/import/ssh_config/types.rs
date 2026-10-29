//! The plain types an ssh_config import produces.

use std::io;
use std::path::PathBuf;

use super::lex::LineError;

/// The local side the expansions need: `~`, the `.ssh` directory relative includes resolve
/// against, and the local user name for `%u` (and `%r` when the host names no user).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshConfigContext {
    pub home: PathBuf,
    pub ssh_dir: PathBuf,
    pub local_user: String,
}

impl SshConfigContext {
    /// The context of a user whose `.ssh` directory sits in `home`.
    #[must_use]
    pub fn new(home: impl Into<PathBuf>, local_user: impl Into<String>) -> Self {
        let home = home.into();
        Self {
            ssh_dir: home.join(".ssh"),
            home,
            local_user: local_user.into(),
        }
    }

    /// The user's own configuration, `<ssh_dir>/config`.
    #[cfg(test)]
    #[must_use]
    pub fn user_config(&self) -> PathBuf {
        self.ssh_dir.join("config")
    }
}

/// Where a line came from (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLine {
    pub file: PathBuf,
    pub line: usize,
}

/// What reading a configuration found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SshConfigImport {
    /// The candidates, in the order their alias first appears.
    pub hosts: Vec<ImportedHost>,
    /// `Host` names that are no connection, each once.
    pub skipped: Vec<SkippedHost>,
    pub warnings: Vec<ImportWarning>,
}

/// One connection candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedHost {
    /// The name on the `Host` line, spelled as it first appears.
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// Key files, by path only, in the order given.
    pub identity_files: Vec<PathBuf>,
    /// The jump host(s) as written; rCommander does not connect through them yet.
    pub proxy_jump: Option<String>,
    /// Keywords set for this host that the import does not take, as first written.
    pub ignored_options: Vec<String>,
    /// The `Host` line that first names the alias.
    pub source: SourceLine,
}

impl ImportedHost {
    /// The address to connect to: `HostName`, or the alias when there is none.
    #[must_use]
    pub fn address(&self) -> &str {
        self.host_name.as_deref().unwrap_or(&self.alias)
    }
}

/// A `Host` name that is no connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedHost {
    pub name: String,
    pub reason: SkipReason,
    pub source: SourceLine,
}

/// Why a `Host` name is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// It holds `*` or `?` and stands for many hosts.
    Pattern,
    /// It starts with `!` and excludes hosts.
    Negated,
}

impl SkipReason {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &["pattern", "negated"];

    /// The stable code, for the GUI's i18n key and the MCP answer.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Negated => "negated",
        }
    }
}

/// Something the import did not take, with the line it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    pub source: SourceLine,
    pub kind: WarningKind,
}

/// What an [`ImportWarning`] is about; [`WarningKind::code`] is its stable key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WarningKind {
    /// An `Include` names a file that is already being read.
    IncludeCycle { file: PathBuf },
    /// An `Include` would nest deeper than [`MAX_INCLUDE_DEPTH`](super::MAX_INCLUDE_DEPTH).
    IncludeTooDeep { file: PathBuf },
    /// An `Include` without a wildcard names a file that does not exist.
    IncludeNotFound { file: PathBuf },
    /// An included file exists but cannot be read.
    IncludeUnreadable { file: PathBuf, error: io::ErrorKind },
    /// An `Include` inside a `Match` block, skipped with the block.
    IncludeInMatch,
    /// Under MCP, an `Include` that leads outside `.ssh` or to a file the read guard refuses.
    IncludeRefused { file: PathBuf },
    /// A `Match` block, skipped up to the next `Host` or `Match`.
    MatchSkipped,
    /// An option before the first `Host`, which would apply to every host.
    OptionOutsideHost { keyword: String },
    /// A keyword without a value.
    MissingArgument { keyword: String },
    /// A `Port` that is no number from 1 to 65535.
    InvalidPort { value: String },
    /// A `%` token the import does not expand, left as written.
    UnknownToken { token: String },
    /// A quote that never closes; the line is skipped.
    UnterminatedQuote,
    /// A line starting with `=`; it is skipped.
    MissingKeyword,
    /// An `Include` names a file read before; each file is read once.
    IncludeRepeated { file: PathBuf },
    /// More than [`MAX_INCLUDES`](super::MAX_INCLUDES) files are included; this `Include` and
    /// every later one are skipped.
    TooManyIncludes,
    /// More than [`MAX_HOSTS`](super::MAX_HOSTS) hosts or pattern names; this one and every
    /// later new one are not taken.
    TooManyHosts,
}

impl WarningKind {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "include_cycle",
        "include_too_deep",
        "include_not_found",
        "include_unreadable",
        "include_in_match",
        "include_refused",
        "match_skipped",
        "option_outside_host",
        "missing_argument",
        "invalid_port",
        "unknown_token",
        "unterminated_quote",
        "missing_keyword",
        "include_repeated",
        "too_many_includes",
        "too_many_hosts",
    ];

    /// The stable code, for the GUI's i18n key and the MCP answer.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::IncludeCycle { .. } => "include_cycle",
            Self::IncludeTooDeep { .. } => "include_too_deep",
            Self::IncludeNotFound { .. } => "include_not_found",
            Self::IncludeUnreadable { .. } => "include_unreadable",
            Self::IncludeInMatch => "include_in_match",
            Self::IncludeRefused { .. } => "include_refused",
            Self::MatchSkipped => "match_skipped",
            Self::OptionOutsideHost { .. } => "option_outside_host",
            Self::MissingArgument { .. } => "missing_argument",
            Self::InvalidPort { .. } => "invalid_port",
            Self::UnknownToken { .. } => "unknown_token",
            Self::UnterminatedQuote => "unterminated_quote",
            Self::MissingKeyword => "missing_keyword",
            Self::IncludeRepeated { .. } => "include_repeated",
            Self::TooManyIncludes => "too_many_includes",
            Self::TooManyHosts => "too_many_hosts",
        }
    }
}

impl From<LineError> for WarningKind {
    fn from(error: LineError) -> Self {
        match error {
            LineError::UnterminatedQuote => Self::UnterminatedQuote,
            LineError::MissingKeyword => Self::MissingKeyword,
        }
    }
}
