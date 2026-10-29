//! The options under a `Host` line, applied to the candidates it names (RC-050-06), with the
//! work held to the text's size (RC-080-47).
//!
//! A line applies to every candidate of its block. Once a block's candidates all hold a
//! keyword's value - a first value, an ignored keyword listed, or [`MAX_IDENTITY_FILES`] key
//! paths - a later line of it in the block changes nothing and is not applied again; a block
//! that listed [`MAX_IGNORED_OPTIONS`] ignored keywords has filled every candidate's list. So
//! `Host` with a thousand names and a megabyte of options costs a few passes, not one per line.

use std::collections::HashSet;

use super::{
    Block, Located, MAX_IDENTITY_FILES, MAX_IGNORED_OPTIONS, Reader, SourceLine, WarningKind,
    lex::Directive,
};

/// The options a candidate takes.
const IMPORTED: &[&str] = &["hostname", "user", "port", "identityfile", "proxyjump"];

/// The candidates under one `Host` line, and what they all hold already.
#[derive(Debug, Clone)]
pub(super) struct HostBlock {
    targets: Vec<usize>,
    /// Keywords in ASCII lower case every candidate holds: a first value taken, or an ignored
    /// keyword listed (or the list full).
    settled: HashSet<String>,
    /// `IdentityFile` lines applied.
    identity_lines: usize,
    /// Ignored keywords applied.
    ignored: usize,
}

impl HostBlock {
    pub(super) fn new(targets: Vec<usize>) -> Self {
        Self {
            targets,
            settled: HashSet::new(),
            identity_lines: 0,
            ignored: 0,
        }
    }
}

impl Reader<'_> {
    pub(super) fn option(&mut self, block: &mut Block, directive: Directive, source: SourceLine) {
        let block = match block {
            Block::Hosts(block) => block,
            Block::Global => {
                let keyword = directive.keyword;
                self.warn(source, WarningKind::OptionOutsideHost { keyword });
                return;
            }
            Block::Match => return,
        };
        let keyword = directive.keyword.to_ascii_lowercase();
        if !IMPORTED.contains(&keyword.as_str()) {
            if block.ignored >= MAX_IGNORED_OPTIONS || !block.settled.insert(keyword) {
                return;
            }
            block.ignored += 1;
            for &index in &block.targets {
                let ignored = &mut self.hosts[index].ignored_options;
                if ignored.len() < MAX_IGNORED_OPTIONS
                    && !ignored
                        .iter()
                        .any(|seen| seen.eq_ignore_ascii_case(&directive.keyword))
                {
                    ignored.push(directive.keyword.clone());
                }
            }
            return;
        }
        let Some(value) = directive.args.into_iter().next() else {
            let keyword = directive.keyword;
            self.warn(source, WarningKind::MissingArgument { keyword });
            return;
        };
        let port = if keyword == "port" {
            match value.parse::<u16>() {
                Ok(port) if port > 0 => Some(port),
                _ => {
                    self.warn(source, WarningKind::InvalidPort { value });
                    return;
                }
            }
        } else {
            None
        };
        if keyword == "identityfile" {
            if block.identity_lines >= MAX_IDENTITY_FILES {
                return;
            }
            block.identity_lines += 1;
        } else if !block.settled.insert(keyword.clone()) {
            return;
        }
        for &index in &block.targets {
            let draft = &mut self.hosts[index];
            let located = || Located {
                value: value.clone(),
                source: source.clone(),
            };
            match keyword.as_str() {
                "hostname" if draft.host_name.is_none() => draft.host_name = Some(located()),
                "user" if draft.user.is_none() => draft.user = Some(value.clone()),
                "port" if draft.port.is_none() => draft.port = port,
                "identityfile" if draft.identity_files.len() < MAX_IDENTITY_FILES => {
                    draft.identity_files.push(located());
                }
                "proxyjump" if draft.proxy_jump.is_none() => {
                    let jump = (!value.eq_ignore_ascii_case("none")).then(|| value.clone());
                    draft.proxy_jump = Some(jump);
                }
                _ => {}
            }
        }
    }
}
