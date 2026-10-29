//! A connection's secret taken from a source, as an export carries it (RC-100-34): the reference,
//! never a value.

use serde::{Deserialize, Serialize};

use crate::SecretKind;

/// A secret of a connection taken from a source; a source missing here is named at connect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedSource {
    pub kind: SecretKind,
    pub source: String,
    pub entry_id: String,
    pub entry_label: String,
}

impl From<rc_db::SecretSourceRef> for ExportedSource {
    fn from(reference: rc_db::SecretSourceRef) -> Self {
        Self {
            kind: reference.kind,
            source: reference.source,
            entry_id: reference.entry_id,
            entry_label: reference.entry_label,
        }
    }
}
