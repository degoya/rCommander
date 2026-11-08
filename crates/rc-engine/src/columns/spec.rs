//! A column set's model (RC-100-90): its columns ([`Column`]: field, title, width, alignment) and
//! the folder masks of the automatic switch, as versioned JSON ([`ColumnSetSpec`]), and the
//! checks every spec passes on its way in - from the window, from MCP, and back from the
//! database.
//!
//! The limits: 1 to [`MAX_COLUMNS`] columns, a width within [`MIN_WIDTH`] and [`MAX_WIDTH`] (the
//! bounds of `columns.*_width`), a title of at most [`MAX_TITLE_CHARS`], 0 to [`MAX_MASKS`]
//! masks of at most [`MAX_MASK_CHARS`] each, [`MAX_SPEC_BYTES`] of JSON, a name of 1 to
//! [`MAX_NAME_CHARS`] characters without control characters. Another [`SPEC_VERSION`] is
//! refused before its fields are read.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ColumnSetError;
use crate::SortKey;

/// A column set's id in the database.
pub type ColumnSetId = i64;

/// The version of the spec this build writes and reads.
pub const SPEC_VERSION: u32 = 1;
/// The most columns a set has.
pub const MAX_COLUMNS: usize = 16;
/// The most folder masks a set has.
pub const MAX_MASKS: usize = 32;
/// The most column sets there are (Total Commander keeps 29).
pub const MAX_SETS: usize = 64;
/// The longest spec, as JSON.
pub const MAX_SPEC_BYTES: usize = 16384;
/// The longest name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The longest column title, in characters.
pub const MAX_TITLE_CHARS: usize = 64;
/// The longest folder mask, in characters.
pub const MAX_MASK_CHARS: usize = 1024;
/// The narrowest column, as the narrowest `columns.*_width`.
pub const MIN_WIDTH: u16 = 24;
/// The widest column, as the widest `columns.*_width`.
pub const MAX_WIDTH: u16 = 640;

/// What a column shows of an entry. A field the backend has no value for stays empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// The name: without its extension when the set also shows `ext`, as the full view splits
    /// them, else whole.
    Name,
    /// The extension.
    Ext,
    /// The size of a file, or a counted folder's.
    Size,
    /// The packed size inside an archive.
    Packed,
    /// The last modification.
    Modified,
    /// The creation.
    Created,
    /// The last access.
    Accessed,
    /// The Windows attribute flags (`rahs`) on Windows, else the rights as `rights` shows them.
    Attributes,
    /// The Unix rights, `rwxr-xr-x`.
    Rights,
    /// The owning user.
    Owner,
    /// The owning group.
    Group,
    /// The comment of `descript.ion`.
    Comment,
    /// The git status as a word, where `panels.git_status` lets git run (RC-100-97).
    Git,
}

impl Field {
    /// Every field, in the order the editor offers them.
    pub const ALL: [Self; 13] = [
        Self::Name,
        Self::Ext,
        Self::Size,
        Self::Packed,
        Self::Modified,
        Self::Created,
        Self::Accessed,
        Self::Attributes,
        Self::Rights,
        Self::Owner,
        Self::Group,
        Self::Comment,
        Self::Git,
    ];

    /// The field's name as the spec writes it, and as `column.field.<name>` translates it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Ext => "ext",
            Self::Size => "size",
            Self::Packed => "packed",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Accessed => "accessed",
            Self::Attributes => "attributes",
            Self::Rights => "rights",
            Self::Owner => "owner",
            Self::Group => "group",
            Self::Comment => "comment",
            Self::Git => "git",
        }
    }

    /// What a click on the column's title sorts by; attributes, rights, owner, group, the
    /// packed size and the git status do not sort, as in the full view.
    #[must_use]
    pub fn sort_key(self) -> Option<SortKey> {
        match self {
            Self::Name => Some(SortKey::Name),
            Self::Ext => Some(SortKey::Ext),
            Self::Size => Some(SortKey::Size),
            Self::Modified => Some(SortKey::Date),
            Self::Created => Some(SortKey::Created),
            Self::Accessed => Some(SortKey::Accessed),
            Self::Comment => Some(SortKey::Comment),
            Self::Packed
            | Self::Attributes
            | Self::Rights
            | Self::Owner
            | Self::Group
            | Self::Git => None,
        }
    }

    /// The width a new column of this field starts with.
    #[must_use]
    pub fn default_width(self) -> u16 {
        match self {
            Self::Name | Self::Comment => 240,
            Self::Ext => 56,
            Self::Size | Self::Packed => 96,
            Self::Modified | Self::Created | Self::Accessed => 128,
            Self::Attributes => 64,
            Self::Rights | Self::Git => 84,
            Self::Owner | Self::Group => 120,
        }
    }

    /// The alignment a new column of this field starts with: numbers to the right.
    #[must_use]
    pub fn default_align(self) -> Align {
        match self {
            Self::Size | Self::Packed => Align::Right,
            _ => Align::Left,
        }
    }
}

/// Where a column's text stands in its cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Right,
}

/// One column of a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Column {
    pub field: Field,
    /// The title in the header; the translated field name when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// In pixels at the default list font size, as `columns.*_width`.
    pub width: u16,
    #[serde(default)]
    pub align: Align,
}

impl Column {
    /// A column of `field` with its default width and alignment and no title of its own.
    #[must_use]
    pub fn of(field: Field) -> Self {
        Self {
            field,
            title: None,
            width: field.default_width(),
            align: field.default_align(),
        }
    }
}

/// A column set's columns and folder masks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ColumnSetSpec {
    /// [`SPEC_VERSION`]; another is refused (`column_set_version`).
    pub version: u32,
    /// 1 to [`MAX_COLUMNS`], left to right.
    pub columns: Vec<Column>,
    /// Folder masks in Total Commander's syntax (`D:\Photos\*`, `sftp://*`, `*.zip\*`), matched
    /// against the path as the path line shows it; a panel entering a matching folder shows the
    /// set (`columns.auto_switch`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<String>,
}

/// A column set as `column_sets.list` and `column_sets.get` answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSet {
    pub id: ColumnSetId,
    pub name: String,
    pub spec: ColumnSetSpec,
}

/// What the `column_sets.*` commands answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum ColumnSetsOutcome {
    /// `column_sets.list`: every set in the person's order.
    List { sets: Vec<ColumnSet> },
    /// `column_sets.get`.
    Set(Box<ColumnSet>),
    /// `column_sets.create`: the set saved.
    Saved { id: ColumnSetId },
    /// `column_sets.update`, `.rename`, `.move`, `.delete`.
    Done {},
}

impl ColumnSetSpec {
    /// A spec of `fields`, each column at its defaults, without masks.
    #[must_use]
    pub fn of(fields: &[Field]) -> Self {
        Self {
            version: SPEC_VERSION,
            columns: fields.iter().copied().map(Column::of).collect(),
            masks: Vec::new(),
        }
    }

    /// The comments view (Ctrl+Shift+F2, Total Commander's `cm_SrcComments`): name, extension
    /// and comment.
    #[must_use]
    pub fn comments() -> Self {
        Self::of(&[Field::Name, Field::Ext, Field::Comment])
    }

    /// Whether a column shows `field`.
    #[must_use]
    pub fn shows(&self, field: Field) -> bool {
        self.columns.iter().any(|column| column.field == field)
    }

    /// The spec in `text`, as the database keeps it: checked like any input.
    ///
    /// # Errors
    ///
    /// Too large, another version, a field that does not read, or a limit broken.
    pub fn parse(text: &str) -> Result<Self, ColumnSetError> {
        if text.len() > MAX_SPEC_BYTES {
            return Err(ColumnSetError::TooLarge {
                bytes: text.len(),
                max: MAX_SPEC_BYTES,
            });
        }
        let value: Value = serde_json::from_str(text).map_err(|error| ColumnSetError::Invalid {
            reason: error.to_string(),
        })?;
        Self::from_value(value)
    }

    /// The spec in `value`, as MCP hands it over: the version first, then the fields, then the
    /// limits.
    ///
    /// # Errors
    ///
    /// As [`ColumnSetSpec::parse`].
    pub fn from_value(value: Value) -> Result<Self, ColumnSetError> {
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| ColumnSetError::Invalid {
                reason: "a column set needs a version".to_owned(),
            })?;
        if version != u64::from(SPEC_VERSION) {
            return Err(ColumnSetError::Version { found: version });
        }
        let spec: Self =
            serde_json::from_value(value).map_err(|error| ColumnSetError::Invalid {
                reason: error.to_string(),
            })?;
        spec.check()?;
        Ok(spec)
    }

    /// The spec as the database keeps it, checked.
    ///
    /// # Errors
    ///
    /// A limit broken, or the JSON larger than [`MAX_SPEC_BYTES`].
    pub fn to_json(&self) -> Result<String, ColumnSetError> {
        self.check()?;
        let text = serde_json::to_string(self).map_err(|error| ColumnSetError::Invalid {
            reason: error.to_string(),
        })?;
        if text.len() > MAX_SPEC_BYTES {
            return Err(ColumnSetError::TooLarge {
                bytes: text.len(),
                max: MAX_SPEC_BYTES,
            });
        }
        Ok(text)
    }

    /// Every limit of the spec.
    ///
    /// # Errors
    ///
    /// The first limit broken, named.
    pub fn check(&self) -> Result<(), ColumnSetError> {
        let invalid = |reason: &str| ColumnSetError::Invalid {
            reason: reason.to_owned(),
        };
        if self.version != SPEC_VERSION {
            return Err(ColumnSetError::Version {
                found: u64::from(self.version),
            });
        }
        if self.columns.is_empty() {
            return Err(invalid("a column set needs a column"));
        }
        if self.columns.len() > MAX_COLUMNS {
            return Err(invalid("a column set has 16 columns at most"));
        }
        for column in &self.columns {
            if !(MIN_WIDTH..=MAX_WIDTH).contains(&column.width) {
                return Err(invalid("a column is 24 to 640 pixels wide"));
            }
            if let Some(title) = &column.title
                && (title.chars().count() > MAX_TITLE_CHARS || title.chars().any(char::is_control))
            {
                return Err(invalid(
                    "a title has 64 characters at most and no control characters",
                ));
            }
        }
        if self.masks.len() > MAX_MASKS {
            return Err(invalid("a column set has 32 folder masks at most"));
        }
        for mask in &self.masks {
            if mask.trim().is_empty()
                || mask.chars().count() > MAX_MASK_CHARS
                || mask.chars().any(char::is_control)
            {
                return Err(invalid(
                    "a folder mask is 1 to 1024 characters without control characters",
                ));
            }
        }
        Ok(())
    }
}

/// `name` as a column set's name: trimmed, 1 to [`MAX_NAME_CHARS`] characters, no control
/// characters.
///
/// # Errors
///
/// [`ColumnSetError::Name`] with the reason.
pub fn clean_name(name: &str) -> Result<String, ColumnSetError> {
    let refused = |reason: &str| ColumnSetError::Name {
        reason: reason.to_owned(),
    };
    if name.chars().any(char::is_control) {
        return Err(refused("a name has no control characters"));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(refused("a column set needs a name"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(refused("the name is too long"));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
