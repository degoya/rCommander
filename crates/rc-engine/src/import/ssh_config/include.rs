//! `Include`: the files it names, read in place under the including block (RC-050-06), and
//! what holds the reading to a bounded amount of work (RC-080-47).
//!
//! Each file is read at most once: one on the stack being read again is a cycle
//! ([`WarningKind::IncludeCycle`]), one read before is skipped ([`WarningKind::IncludeRepeated`]),
//! so `Include f f f` nested eight deep reads `f` once. At most [`MAX_INCLUDES`] files are named
//! in all, counted before anything is opened; the rest are skipped with one
//! [`WarningKind::TooManyIncludes`].

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    Block, MAX_INCLUDE_DEPTH, MAX_INCLUDES, Reader, SourceLine, WarningKind,
    expand::{expand_glob, expand_tilde},
    guard::{holds_key, is_key_file},
    scope::read_text,
};

impl Reader<'_> {
    pub(super) fn include(
        &mut self,
        patterns: &[String],
        block: &Block,
        source: &SourceLine,
        depth: usize,
    ) {
        if matches!(block, Block::Match) {
            self.warn(source.clone(), WarningKind::IncludeInMatch);
            return;
        }
        if patterns.is_empty() {
            let keyword = "Include".to_owned();
            self.warn(source.clone(), WarningKind::MissingArgument { keyword });
        }
        for pattern in patterns {
            let expanded = expand_tilde(pattern, &self.context.home);
            for file in expand_glob(&self.context.ssh_dir.join(&expanded)) {
                if self.includes >= MAX_INCLUDES {
                    if !self.includes_capped {
                        self.includes_capped = true;
                        self.warn(source.clone(), WarningKind::TooManyIncludes);
                    }
                    return;
                }
                self.includes += 1;
                self.include_file(&file, block.clone(), source, depth + 1);
            }
        }
    }

    fn include_file(&mut self, file: &Path, block: Block, source: &SourceLine, depth: usize) {
        if self.key_material.is_some() {
            return;
        }
        let file_buf = file.to_path_buf();
        if depth > MAX_INCLUDE_DEPTH {
            self.warn(
                source.clone(),
                WarningKind::IncludeTooDeep { file: file_buf },
            );
            return;
        }
        // A key is never opened: its name is checked before, and as the canonical path it
        // resolves to, so that neither a link nor a glob reaches one. Under MCP the file must
        // lie inside `.ssh` as it really is, and pass the read guard.
        let canonical = match fs::canonicalize(file) {
            Ok(canonical) => canonical,
            Err(error) => return self.unreadable(source, file_buf, &error),
        };
        if self
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.admits(&canonical))
        {
            self.warn(
                source.clone(),
                WarningKind::IncludeRefused { file: file_buf },
            );
            return;
        }
        if is_key_file(file) || is_key_file(&canonical) {
            self.key_material = Some(file_buf);
            return;
        }
        // A file on the stack or read before was judged when it was read; it is not opened again.
        if self.reading.contains(&canonical) {
            self.warn(source.clone(), WarningKind::IncludeCycle { file: file_buf });
            return;
        }
        if self.read_files.contains(&canonical) {
            self.warn(
                source.clone(),
                WarningKind::IncludeRepeated { file: file_buf },
            );
            return;
        }
        let text = match read_text(&canonical) {
            Ok(text) if holds_key(&text) => {
                self.key_material = Some(file_buf);
                return;
            }
            Ok(text) => text,
            Err(error) => return self.unreadable(source, file_buf, &error),
        };
        self.read_files.insert(canonical.clone());
        self.reading.push(canonical);
        self.read(file, &text, block, depth);
        self.reading.pop();
    }

    /// The warning for an included `file` that cannot be found or read.
    fn unreadable(&mut self, source: &SourceLine, file: PathBuf, error: &io::Error) {
        let kind = if error.kind() == io::ErrorKind::NotFound {
            WarningKind::IncludeNotFound { file }
        } else {
            WarningKind::IncludeUnreadable {
                file,
                error: error.kind(),
            }
        };
        self.warn(source.clone(), kind);
    }
}
