//! What the find-files dialog's fields hold, and the search they make: masks, start folders,
//! depth and text on the first tab, size, date, attributes and kind on the second, as Total
//! Commander lays them out. Every field is text as typed; [`Form::query`] reads it and says
//! which field it could not read.

use chrono::{Local, NaiveDate, TimeZone};
use rc_engine::{
    SearchQuery,
    search::{ContentQuery, Filters, KindFilter, TextEncoding},
};

use crate::format::parse_size;

/// Seconds of a day, for "not older than n days".
const DAY_SECS: u64 = 24 * 60 * 60;

/// A text field of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// "Search for": masks, or a regular expression.
    Name,
    /// "Search in": start folders separated by `;`.
    Start,
    /// Folder levels below a start folder; empty for all.
    Depth,
    /// "Find text"; empty for none.
    Text,
    MinSize,
    MaxSize,
    /// Not older than this many days.
    Days,
    /// Modified on or after this day (`YYYY-MM-DD`).
    After,
    /// Modified on or before this day.
    Before,
}

/// A switch of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    NameRegex,
    NameCase,
    TextCase,
    TextWords,
    TextRegex,
    NotContaining,
    SkipBinary,
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
    FollowLinks,
    SkipHidden,
}

/// An attribute condition: either way, set, or not set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tri {
    #[default]
    Any,
    Set,
    Unset,
}

impl Tri {
    pub const ALL: [Self; 3] = [Self::Any, Self::Set, Self::Unset];

    fn wanted(self) -> Option<bool> {
        match self {
            Self::Any => None,
            Self::Set => Some(true),
            Self::Unset => Some(false),
        }
    }
}

/// An attribute with a [`Tri`] condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    Hidden,
    Readonly,
    System,
    Archive,
}

/// The dialog's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub name: String,
    pub name_regex: bool,
    pub name_case: bool,
    pub start: String,
    pub depth: String,
    pub text: String,
    pub text_case: bool,
    pub text_words: bool,
    pub text_regex: bool,
    pub not_containing: bool,
    pub skip_binary: bool,
    pub utf8: bool,
    pub utf16_le: bool,
    pub utf16_be: bool,
    pub windows1252: bool,
    pub min_size: String,
    pub max_size: String,
    pub days: String,
    pub after: String,
    pub before: String,
    pub kind: KindFilter,
    pub hidden: Tri,
    pub readonly: Tri,
    pub system: Tri,
    pub archive: Tri,
    pub follow_links: bool,
    pub skip_hidden: bool,
}

impl Form {
    /// A form that searches `start` for every name, as Alt+F7 opens it.
    pub fn starting_in(start: String) -> Self {
        Self {
            name: "*.*".to_owned(),
            start,
            ..Self::default()
        }
    }

    pub fn text_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Start => &mut self.start,
            Field::Depth => &mut self.depth,
            Field::Text => &mut self.text,
            Field::MinSize => &mut self.min_size,
            Field::MaxSize => &mut self.max_size,
            Field::Days => &mut self.days,
            Field::After => &mut self.after,
            Field::Before => &mut self.before,
        }
    }

    pub fn flag_mut(&mut self, flag: Flag) -> &mut bool {
        match flag {
            Flag::NameRegex => &mut self.name_regex,
            Flag::NameCase => &mut self.name_case,
            Flag::TextCase => &mut self.text_case,
            Flag::TextWords => &mut self.text_words,
            Flag::TextRegex => &mut self.text_regex,
            Flag::NotContaining => &mut self.not_containing,
            Flag::SkipBinary => &mut self.skip_binary,
            Flag::Utf8 => &mut self.utf8,
            Flag::Utf16Le => &mut self.utf16_le,
            Flag::Utf16Be => &mut self.utf16_be,
            Flag::Windows1252 => &mut self.windows1252,
            Flag::FollowLinks => &mut self.follow_links,
            Flag::SkipHidden => &mut self.skip_hidden,
        }
    }

    pub fn tri_mut(&mut self, attribute: Attribute) -> &mut Tri {
        match attribute {
            Attribute::Hidden => &mut self.hidden,
            Attribute::Readonly => &mut self.readonly,
            Attribute::System => &mut self.system,
            Attribute::Archive => &mut self.archive,
        }
    }

    /// The start folders as typed, each trimmed; empty ones dropped.
    pub fn start_folders(&self) -> Vec<String> {
        self.start
            .split(';')
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// The search the fields describe, or the first field that does not read.
    ///
    /// # Errors
    ///
    /// The field whose text is no size, number or date.
    pub fn query(&self) -> Result<SearchQuery, Field> {
        let content = (!self.text.is_empty()).then(|| ContentQuery {
            text: self.text.clone(),
            regex: self.text_regex,
            case_sensitive: self.text_case,
            whole_words: self.text_words,
            encodings: self.encodings(),
            not_containing: self.not_containing,
            skip_binary: self.skip_binary,
        });
        let filters = Filters {
            kind: self.kind,
            min_size: optional(&self.min_size, parse_size).ok_or(Field::MinSize)?,
            max_size: optional(&self.max_size, parse_size).ok_or(Field::MaxSize)?,
            modified_after: optional(&self.after, |day| day_start(day, 0)).ok_or(Field::After)?,
            // The last second of the day; a day that ends at the epoch is no date to search.
            modified_before: optional(&self.before, |day| day_start(day, 1)?.checked_sub(1))
                .ok_or(Field::Before)?,
            max_age_secs: optional(&self.days, |days| {
                days.parse::<u64>().ok()?.checked_mul(DAY_SECS)
            })
            .ok_or(Field::Days)?,
            hidden: self.hidden.wanted(),
            readonly: self.readonly.wanted(),
            system: self.system.wanted(),
            archive: self.archive.wanted(),
            mode_bits: None,
        };
        Ok(SearchQuery {
            name: self.name.trim().to_owned(),
            name_regex: self.name_regex,
            case_sensitive: self.name_case,
            content,
            filters,
            max_depth: optional(&self.depth, |depth| depth.parse().ok()).ok_or(Field::Depth)?,
            follow_symlinks: self.follow_links,
            skip_hidden: self.skip_hidden,
        })
    }

    /// The encodings ticked; none is "automatic".
    fn encodings(&self) -> Vec<TextEncoding> {
        [
            (self.utf8, TextEncoding::Utf8),
            (self.utf16_le, TextEncoding::Utf16Le),
            (self.utf16_be, TextEncoding::Utf16Be),
            (self.windows1252, TextEncoding::Windows1252),
        ]
        .into_iter()
        .filter_map(|(ticked, encoding)| ticked.then_some(encoding))
        .collect()
    }
}

/// `None` for an empty field, `Some(None)` for one that does not read, else its value.
fn optional<T>(field: &str, parse: impl FnOnce(&str) -> Option<T>) -> Option<Option<T>> {
    let field = field.trim();
    if field.is_empty() {
        Some(None)
    } else {
        parse(field).map(Some)
    }
}

/// The Unix time of local midnight starting `day` (`YYYY-MM-DD`), `days_later` days on.
fn day_start(day: &str, days_later: u64) -> Option<u64> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    let date = date.checked_add_days(chrono::Days::new(days_later))?;
    let midnight = date.and_hms_opt(0, 0, 0)?;
    let local = Local.from_local_datetime(&midnight).earliest()?;
    u64::try_from(local.timestamp()).ok()
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
