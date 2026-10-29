//! A small reader for Total Commander's INI files (`wincmd.ini`, `wcx_ftp.ini`), RC-050-09.
//!
//! The files are hostile input: the reader takes at most [`MAX_FILE_BYTES`] of a regular file,
//! drops values longer than [`MAX_VALUE_CHARS`], and only splits text - nothing of a file is ever
//! run or followed. It knows sections (`[name]`), `key=value` lines and `;` comments; everything
//! else is ignored. Names compare ignoring ASCII case, as Windows' own INI functions do; the first
//! of two equal keys counts, and a section that comes again adds its keys to the first. Sections
//! and keys are found by their lower-case names (RC-080-47), so a file of many sections costs
//! one pass, not one per section.
//!
//! The text is decoded as the viewer decodes a file: UTF-16 LE with its BOM (Total Commander's
//! Unicode files), UTF-8 with or without BOM, or else ANSI (Windows-1252); the viewer's UTF-16
//! BE and UTF-16 without a BOM are read too. `wcx_ftp.ini` holds obfuscated passwords, so the
//! bytes, the text and every value are zeroized when dropped.

use std::{collections::HashMap, io, path::Path};

use zeroize::Zeroizing;

use super::bounded_read::{BoundedReadError, read_bounded};

/// The largest file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The longest value kept, in characters; a longer one is dropped and counted.
pub const MAX_VALUE_CHARS: usize = 4096;

/// Why a file was not read.
#[derive(Debug, thiserror::Error)]
pub enum IniError {
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Not a regular file: a folder, a device or a pipe is never opened.
    #[error("not a regular file")]
    NotAFile,
    #[error("larger than {MAX_FILE_BYTES} bytes")]
    TooLarge,
}

impl From<BoundedReadError> for IniError {
    fn from(error: BoundedReadError) -> Self {
        match error {
            BoundedReadError::Io(error) => Self::Io(error),
            BoundedReadError::NotAFile => Self::NotAFile,
            BoundedReadError::TooLarge { .. } => Self::TooLarge,
        }
    }
}

/// One section: its name as written and its keys in file order.
#[derive(Debug, Default)]
pub struct Section {
    pub name: String,
    entries: Vec<(String, Zeroizing<String>)>,
    /// The index in `entries` of each key's first value, by the key in ASCII lower case.
    first: HashMap<String, usize>,
}

impl Section {
    /// The first value of `key`, ignoring ASCII case.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.first
            .get(&key.to_ascii_lowercase())
            .map(|&at| self.entries[at].1.as_str())
    }

    fn push(&mut self, key: &str, value: &str) {
        self.first
            .entry(key.to_ascii_lowercase())
            .or_insert(self.entries.len());
        self.entries
            .push((key.to_owned(), Zeroizing::new(value.to_owned())));
    }

    /// Every key and value, in file order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// A parsed file.
#[derive(Debug, Default)]
pub struct Ini {
    sections: Vec<Section>,
    /// The index in `sections` of each section, by its name in ASCII lower case.
    by_name: HashMap<String, usize>,
    /// How many values were dropped for their length.
    pub dropped: usize,
}

impl Ini {
    /// The section `name`, ignoring ASCII case.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .map(|&at| &self.sections[at])
    }

    /// The index of the section `name`, added when it is new.
    fn section_at(&mut self, name: &str) -> usize {
        let next = self.sections.len();
        let at = *self
            .by_name
            .entry(name.to_ascii_lowercase())
            .or_insert(next);
        if at == next {
            self.sections.push(Section {
                name: name.to_owned(),
                ..Section::default()
            });
        }
        at
    }

    /// The value of `key` in `section`.
    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }
}

/// The file at `path`, when it is a regular file of at most [`MAX_FILE_BYTES`] (`bounded_read`).
///
/// # Errors
///
/// The file cannot be read, is no regular file, or is too large.
pub fn read_file(path: &Path) -> Result<Ini, IniError> {
    let bytes = read_bounded(path, MAX_FILE_BYTES)?;
    Ok(parse(&decode(&bytes)))
}

/// `bytes` as text, decoded as the viewer decodes a file (`viewer::decode_text`).
#[must_use]
pub fn decode(bytes: &[u8]) -> Zeroizing<String> {
    Zeroizing::new(crate::viewer::decode_text(bytes).1)
}

/// The sections and keys of `text`.
#[must_use]
pub fn parse(text: &str) -> Ini {
    let mut ini = Ini::default();
    // The index of the section the lines belong to; none before the first.
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some((name, _)) = rest.split_once(']') else {
                current = None;
                continue;
            };
            current = Some(ini.section_at(name.trim()));
            continue;
        }
        let (Some(at), Some((key, value))) = (current, line.split_once('=')) else {
            continue;
        };
        let value = value.trim();
        if value.chars().count() > MAX_VALUE_CHARS {
            ini.dropped += 1;
            continue;
        }
        ini.sections[at].push(key.trim(), value);
    }
    ini
}

#[cfg(test)]
#[path = "tc_ini_tests.rs"]
mod tests;
