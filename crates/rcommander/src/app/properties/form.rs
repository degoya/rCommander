//! The attribute fields of the properties dialog, Total Commander's "Change attributes": the
//! flags, the Unix mode and the two times start as the first entry has them, and only what a
//! person changed goes into the [`PropertyChange`].

use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use rc_engine::{ApplyTo, EntryKind, PropertiesReport, PropertyChange};

use crate::i18n::t;

/// How the times are written in the fields, in local time.
pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// A flag of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Readonly,
    Hidden,
    System,
    Archive,
}

impl Flag {
    pub const ALL: [Self; 4] = [Self::Readonly, Self::Hidden, Self::System, Self::Archive];

    fn index(self) -> usize {
        match self {
            Self::Readonly => 0,
            Self::Hidden => 1,
            Self::System => 2,
            Self::Archive => 3,
        }
    }
}

/// A change of one field.
#[derive(Debug, Clone)]
pub enum FormMsg {
    Flag(Flag, bool),
    Mode(String),
    Modified(String),
    Created(String),
    /// Both times to now.
    Now,
    Recursive(bool),
    ApplyTo(ApplyTo),
}

/// The fields and what they started with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeForm {
    /// The flags in the order of [`Flag::ALL`].
    pub flags: [bool; 4],
    original_flags: [bool; 4],
    /// The Unix mode in octal; empty where there is none.
    pub mode: String,
    original_mode: String,
    pub modified: String,
    original_modified: String,
    pub created: String,
    original_created: String,
    /// Whether the entries have a Unix mode.
    pub has_mode: bool,
    /// Whether one of the entries is a folder: recursion is offered.
    pub has_dirs: bool,
    pub recursive: bool,
    pub apply_to: ApplyTo,
}

impl AttributeForm {
    /// The fields as the first entry of `report` has them.
    #[must_use]
    pub fn from_report(report: &PropertiesReport) -> Self {
        let Some(first) = report.items.first() else {
            return Self::default();
        };
        let attributes = first.entry.attributes;
        let windows = attributes.windows.unwrap_or_default();
        let flags = [
            attributes.readonly,
            windows.hidden,
            windows.system,
            windows.archive,
        ];
        let mode = attributes
            .mode
            .map_or_else(String::new, |mode| format!("{mode:o}"));
        let modified = time_text(first.entry.modified);
        let created = time_text(first.entry.created);
        Self {
            flags,
            original_flags: flags,
            has_mode: attributes.mode.is_some(),
            mode: mode.clone(),
            original_mode: mode,
            modified: modified.clone(),
            original_modified: modified,
            created: created.clone(),
            original_created: created,
            has_dirs: report
                .items
                .iter()
                .any(|item| item.entry.kind == EntryKind::Dir),
            recursive: false,
            apply_to: ApplyTo::default(),
        }
    }

    pub fn flag(&self, flag: Flag) -> bool {
        self.flags[flag.index()]
    }

    pub fn update(&mut self, message: FormMsg) {
        match message {
            FormMsg::Flag(flag, on) => self.flags[flag.index()] = on,
            FormMsg::Mode(mode) => self.mode = mode,
            FormMsg::Modified(text) => self.modified = text,
            FormMsg::Created(text) => self.created = text,
            FormMsg::Now => {
                let now = time_text(Some(SystemTime::now()));
                self.modified.clone_from(&now);
                self.created = now;
            }
            FormMsg::Recursive(on) => self.recursive = on,
            FormMsg::ApplyTo(apply_to) => self.apply_to = apply_to,
        }
    }

    /// What a person changed; the message of the first field that does not read.
    pub fn change(&self) -> Result<PropertyChange, String> {
        let flag = |flag: Flag| {
            let at = flag.index();
            (self.flags[at] != self.original_flags[at]).then_some(self.flags[at])
        };
        let mode = if self.mode.trim() == self.original_mode {
            None
        } else {
            Some(
                u32::from_str_radix(self.mode.trim(), 8)
                    .ok()
                    .filter(|mode| *mode <= 0o7777)
                    .ok_or_else(|| t!("props.mode_invalid").to_owned())?,
            )
        };
        let time = |text: &str, original: &str| {
            if text.trim() == original {
                return Ok(None);
            }
            parse_time(text)
                .map(Some)
                .ok_or_else(|| t!("props.time_invalid", format = "YYYY-MM-DD HH:MM:SS"))
        };
        Ok(PropertyChange {
            mode,
            readonly: flag(Flag::Readonly),
            hidden: flag(Flag::Hidden),
            system: flag(Flag::System),
            archive: flag(Flag::Archive),
            modified: time(&self.modified, &self.original_modified)?,
            created: time(&self.created, &self.original_created)?,
        })
    }
}

/// A time in the fields' format; empty when unknown.
fn time_text(time: Option<SystemTime>) -> String {
    time.map_or_else(String::new, |time| {
        DateTime::<Local>::from(time)
            .format(TIME_FORMAT)
            .to_string()
    })
}

/// A time typed in the fields' format, in local time.
fn parse_time(text: &str) -> Option<SystemTime> {
    let naive = NaiveDateTime::parse_from_str(text.trim(), TIME_FORMAT).ok()?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(SystemTime::from)
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
