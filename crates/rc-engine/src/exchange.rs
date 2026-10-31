//! The export and import of settings (with the own colour themes, RC-100-50), connections,
//! trusted host keys and certificates, the hotlist and the tabs (RC-080-19): one `.rcexport`
//! file, unencrypted or sealed with an export password of its own (`rc_vault::seal_export`),
//! never with the vault's key.
//!
//! - `format`: the file - its JSON, the sealed form, the caps a file read from anywhere is held
//!   to before anything of it is used.
//! - `report`: the parts, the rows of an import's preview and what an export wrote.
//! - `error`: the failures of both, each with a stable code.
//!
//! The commands are `config.export` and `config.import`, both `GuiOnly`: an export can carry the
//! secrets and an import writes trust decisions, which are a person's. The work is the engine's
//! (`commands/exchange_*.rs`): an export reads the parts and, only when secrets go with it, the
//! vault; an import reads and checks the whole file - size, format, version, the seal, every
//! field - before it compares one row, and writes nothing on a refusal.

mod error;
mod format;
mod report;
mod sources;
mod themes;

pub use error::ExchangeError;
pub use format::{
    ConnectionsPart, Content, EXTENSION, ExportFile, ExportSecret, ExportedCertificate,
    ExportedConnection, ExportedHostKey, ExportedHotlistItem, ExportedScript, ExportedScriptSecret,
    ExportedSetting, ExportedTab, FORMAT, FORMAT_VERSION, HotlistPart, MAX_FILE_BYTES, MAX_ITEMS,
    MAX_TEXT, SOURCE_PREFIX, SealedFile, TrustPart, hex,
};
pub use report::{ExchangePart, ExchangeReport, ExchangeRow, ExportParts, ExportReport, RowStatus};
pub use sources::ExportedSource;
pub use themes::ExportedTheme;
