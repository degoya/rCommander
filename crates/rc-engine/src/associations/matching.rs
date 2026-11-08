//! Which rule and which command a file gets (RC-100-91), pure and shared by the engine and the
//! window's context menu. A rule applies to a file whose name its mask matches; to an archive -
//! a file Enter opens as a folder by its extension - only when one of its masks names that
//! extension (`*.zip`, not `*` or `*.*`), as Total Commander 11.51 does. Enter takes the `open`
//! of the first rule that has one; the menu offers the first matching rule's commands, and with
//! "all matching types" the others' too, each name once.

use crate::{
    mask::{Mask, split_line},
    user_commands::CommandRef,
};

use super::FileAssociation;

/// The name of a rule's Enter command, in the menu and for `file.run_association`.
pub const OPEN_VERB: &str = "open";

/// Whether Enter opens the file `name` as an archive (by its extension), as `file.open` decides.
#[must_use]
pub fn is_archive_name(name: &str) -> bool {
    crate::archives::opens_on_enter(name)
}

/// Whether one of the masks of `mask` (before its `|`) names the extension of `name` itself: it
/// ends in `.` and that extension, without a wildcard in it.
#[must_use]
pub fn names_extension(mask: &str, name: &str) -> bool {
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if extension.is_empty() {
        return false;
    }
    let extension = extension.to_lowercase();
    split_line(mask).0.iter().any(|token| {
        token
            .text
            .rsplit_once('.')
            .is_some_and(|(_, named)| named.to_lowercase() == extension)
    })
}

/// Whether `rule` applies to the file `name`, an archive by its extension when `archive`.
#[must_use]
pub fn applies(rule: &FileAssociation, name: &str, archive: bool) -> bool {
    Mask::parse(&rule.mask).matches(name, false) && (!archive || names_extension(&rule.mask, name))
}

/// What Enter runs for the file `name`: the `open` of the first rule that applies and has one.
#[must_use]
pub fn open_for<'a>(
    rules: &'a [FileAssociation],
    name: &str,
    archive: bool,
) -> Option<&'a CommandRef> {
    rules
        .iter()
        .filter(|rule| applies(rule, name, archive))
        .find_map(|rule| rule.open.as_ref())
}

/// The commands the context menu offers for the file `name`: [`OPEN_VERB`] when Enter has one,
/// then the named commands of the first rule that applies, and with `all` those of the other
/// rules that apply, each name once (case does not count).
#[must_use]
pub fn commands_for(
    rules: &[FileAssociation],
    name: &str,
    archive: bool,
    all: bool,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if open_for(rules, name, archive).is_some() {
        names.push(OPEN_VERB.to_owned());
    }
    let matching = rules.iter().filter(|rule| applies(rule, name, archive));
    let taken = if all { usize::MAX } else { 1 };
    for rule in matching.take(taken) {
        for verb in &rule.verbs {
            let known = names
                .iter()
                .any(|known| known.to_lowercase() == verb.name.to_lowercase());
            if !known {
                names.push(verb.name.clone());
            }
        }
    }
    names
}

/// The command `verb` offers for the file `name` ([`commands_for`]): [`OPEN_VERB`] is Enter's,
/// any other the first named so among the rules the menu reads.
#[must_use]
pub fn command_for<'a>(
    rules: &'a [FileAssociation],
    name: &str,
    archive: bool,
    all: bool,
    verb: &str,
) -> Option<&'a CommandRef> {
    if verb.eq_ignore_ascii_case(OPEN_VERB) {
        return open_for(rules, name, archive);
    }
    let taken = if all { usize::MAX } else { 1 };
    let wanted = verb.trim().to_lowercase();
    rules
        .iter()
        .filter(|rule| applies(rule, name, archive))
        .take(taken)
        .flat_map(|rule| &rule.verbs)
        .find(|candidate| candidate.name.to_lowercase() == wanted)
        .map(|candidate| &candidate.action)
}

/// A program's parameters with Total Commander's `%1` and `%2` as RC-100-86's placeholders: `%1`
/// (also written `"%1"`) is the file, `"%P%N"`; `%2` (also `"%2"`) every marked file with its
/// folder, `%P%S`, which quotes a name with a blank itself. `%%` stays a percent sign.
#[must_use]
pub fn file_params(params: &str) -> String {
    let mut out = String::with_capacity(params.len() + 8);
    let mut rest = params;
    while let Some(at) = rest.find(['%', '"']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let (replaced, used) = if tail.starts_with("%%") {
            ("%%", 2)
        } else if tail.starts_with("\"%1\"") {
            ("\"%P%N\"", 4)
        } else if tail.starts_with("\"%2\"") {
            ("%P%S", 4)
        } else if tail.starts_with("%1") {
            ("\"%P%N\"", 2)
        } else if tail.starts_with("%2") {
            ("%P%S", 2)
        } else {
            (&tail[..1], 1)
        };
        out.push_str(replaced);
        rest = &tail[used..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "matching_tests.rs"]
mod tests;
