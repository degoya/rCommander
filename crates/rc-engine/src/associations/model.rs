//! The rules of the internal associations (RC-100-91) and their one check: every rule comes from
//! the window, an import or the database, and is read as untrusted input. The setting keeps them
//! as JSON text - empty for none -, written the one way [`write_rules`] writes them, so the same
//! rules always read as one value.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    mask::split_line,
    user_commands::{CommandKind, CommandRef},
};

/// The most rules.
pub const MAX_RULES: usize = 256;
/// The most named commands of one rule.
pub const MAX_VERBS: usize = 16;
/// The most bytes the rules take as the setting's text.
pub const MAX_BYTES: usize = 64 * 1024;
/// The most characters of a mask, a description, a symbol or a command's name.
pub const MAX_TEXT: usize = 1024;
/// The most characters of a command's name.
const MAX_VERB_NAME: usize = 64;

/// One named command of a rule, for the context menu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssociationVerb {
    /// What the menu shows, unique in its rule regardless of case; never `open`.
    pub name: String,
    pub action: CommandRef,
}

/// One rule: a file type and its commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAssociation {
    /// Total Commander's mask: `*.txt *.log`, `;` or blanks between, masks after `|` excluded.
    pub mask: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The symbol's file (an `.ico`, or a program with its index as `path,3`); empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// What Enter runs; `None` leaves Enter to the next rule or the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<CommandRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verbs: Vec<AssociationVerb>,
}

/// What does not fit in the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleErrorKind {
    /// The setting's text is no list of rules.
    NotJson,
    /// More than [`MAX_RULES`] rules.
    TooMany,
    /// More than [`MAX_BYTES`] as text.
    TooLarge,
    /// An empty mask.
    Mask,
    /// A field with a control character or longer than [`MAX_TEXT`].
    Text,
    /// A command's name that is empty, too long, holds a control character or is `open`.
    VerbName,
    /// Two commands of a rule share a name.
    VerbTwice,
    /// More than [`MAX_VERBS`] named commands.
    TooManyVerbs,
    /// A program without a file parameter (`%1`, `%2`, `%N`, `%S`, `%L` ...).
    NeedsFileParameter,
    /// The command itself does not fit (RC-100-86's check), and why.
    Command(String),
}

impl RuleErrorKind {
    /// The stable code; the window explains it under `associations.error.<code>`.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotJson => "association_not_json",
            Self::TooMany => "association_too_many",
            Self::TooLarge => "association_too_large",
            Self::Mask => "association_mask",
            Self::Text => "association_text",
            Self::VerbName => "association_verb_name",
            Self::VerbTwice => "association_verb_twice",
            Self::TooManyVerbs => "association_too_many_verbs",
            Self::NeedsFileParameter => "association_needs_file_parameter",
            Self::Command(_) => "association_command",
        }
    }
}

/// Why the rules do not fit, and where: the rule (from 0) and the command (`open` or a verb's
/// name as typed). It never repeats a parameter: one may hold a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    pub rule: Option<usize>,
    pub verb: Option<String>,
    pub kind: RuleErrorKind,
}

impl RuleError {
    fn list(kind: RuleErrorKind) -> Self {
        Self {
            rule: None,
            verb: None,
            kind,
        }
    }
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.code())?;
        if let Some(rule) = self.rule {
            write!(formatter, ": rule {}", rule + 1)?;
        }
        if let Some(verb) = &self.verb {
            write!(formatter, ", command {verb:?}")?;
        }
        let why = match &self.kind {
            RuleErrorKind::NotJson => "expected a JSON list of rules".to_owned(),
            RuleErrorKind::TooMany => format!("at most {MAX_RULES} rules"),
            RuleErrorKind::TooLarge => format!("at most {} KiB", MAX_BYTES / 1024),
            RuleErrorKind::Mask => "a mask such as *.txt *.log".to_owned(),
            RuleErrorKind::Text => {
                format!("at most {MAX_TEXT} characters, no control character")
            }
            RuleErrorKind::VerbName => {
                format!("a name of 1 to {MAX_VERB_NAME} characters other than open")
            }
            RuleErrorKind::VerbTwice => "two commands share a name".to_owned(),
            RuleErrorKind::TooManyVerbs => format!("at most {MAX_VERBS} named commands"),
            RuleErrorKind::NeedsFileParameter => {
                "put %1 or \"%1\" (or %2) behind the program so it gets the file".to_owned()
            }
            RuleErrorKind::Command(reason) => reason.clone(),
        };
        write!(formatter, " - {why}")
    }
}

impl std::error::Error for RuleError {}

/// Whether a program's parameters name the file: `%1`, `%2`, `%N`, `%S` (`%P%S`), or a list file
/// `%L`, `%F`, `%UL`, `%UF`, `%WL`, `%WF`; `%%` is a percent sign and names nothing.
#[must_use]
pub fn has_file_parameter(params: &str) -> bool {
    let bytes = params.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            at += 1;
            continue;
        }
        match (bytes.get(at + 1), bytes.get(at + 2)) {
            (Some(b'%'), _) => at += 2,
            (Some(b'1' | b'2' | b'N' | b'S' | b'L' | b'F'), _)
            | (Some(b'U' | b'W'), Some(b'L' | b'F')) => return true,
            _ => at += 1,
        }
    }
    false
}

/// Refuses a field with a control character or longer than [`MAX_TEXT`] characters.
fn fits(text: &str) -> bool {
    !text.chars().any(char::is_control) && text.chars().count() <= MAX_TEXT
}

/// `action` checked for a rule: RC-100-86's check, and a program with a file parameter.
fn checked_action(action: &CommandRef) -> Result<CommandRef, RuleErrorKind> {
    let action = action
        .checked()
        .map_err(|error| RuleErrorKind::Command(error.to_string()))?;
    if action.kind == CommandKind::Program && !has_file_parameter(action.params_to_ask()) {
        return Err(RuleErrorKind::NeedsFileParameter);
    }
    Ok(action)
}

/// `rule` checked: trimmed where blanks mean nothing, an `open` without a command left out.
fn checked_rule(
    rule: &FileAssociation,
) -> Result<FileAssociation, (Option<String>, RuleErrorKind)> {
    let own = |kind| (None, kind);
    if !fits(&rule.mask) || !fits(&rule.description) || !fits(&rule.icon) {
        return Err(own(RuleErrorKind::Text));
    }
    let mask = rule.mask.trim().to_owned();
    let (include, exclude) = split_line(&mask);
    if include.is_empty() && exclude.is_empty() {
        return Err(own(RuleErrorKind::Mask));
    }
    if rule.verbs.len() > MAX_VERBS {
        return Err(own(RuleErrorKind::TooManyVerbs));
    }
    let open = match &rule.open {
        Some(action) if !action.command.trim().is_empty() => {
            Some(checked_action(action).map_err(|kind| (Some(super::OPEN_VERB.to_owned()), kind))?)
        }
        _ => None,
    };
    let mut verbs: Vec<AssociationVerb> = Vec::with_capacity(rule.verbs.len());
    for verb in &rule.verbs {
        let name = verb.name.trim().to_owned();
        let named = |kind| (Some(verb.name.clone()), kind);
        let plain = !name.is_empty()
            && name.chars().count() <= MAX_VERB_NAME
            && !name.chars().any(char::is_control)
            && !name.eq_ignore_ascii_case(super::OPEN_VERB);
        if !plain {
            return Err(named(RuleErrorKind::VerbName));
        }
        if verbs
            .iter()
            .any(|known| known.name.to_lowercase() == name.to_lowercase())
        {
            return Err(named(RuleErrorKind::VerbTwice));
        }
        let action = checked_action(&verb.action).map_err(named)?;
        verbs.push(AssociationVerb { name, action });
    }
    Ok(FileAssociation {
        mask,
        description: rule.description.trim().to_owned(),
        icon: rule.icon.trim().to_owned(),
        open,
        verbs,
    })
}

/// `rules` checked, in their order: at most [`MAX_RULES`], each with a mask, its texts fitting,
/// its commands checked as RC-100-86 checks them and each program given the file; at most
/// [`MAX_BYTES`] as the setting's text.
///
/// # Errors
///
/// The first thing that does not fit, with the rule and the command it is in.
pub fn check_rules(rules: &[FileAssociation]) -> Result<Vec<FileAssociation>, RuleError> {
    if rules.len() > MAX_RULES {
        return Err(RuleError::list(RuleErrorKind::TooMany));
    }
    let checked = rules
        .iter()
        .enumerate()
        .map(|(at, rule)| {
            checked_rule(rule).map_err(|(verb, kind)| RuleError {
                rule: Some(at),
                verb,
                kind,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if write_rules(&checked).len() > MAX_BYTES {
        return Err(RuleError::list(RuleErrorKind::TooLarge));
    }
    Ok(checked)
}

/// The setting's text as rules, checked: empty text is none.
///
/// # Errors
///
/// The text is longer than [`MAX_BYTES`], no JSON list of rules, or a rule does not fit.
pub fn parse_rules(text: &str) -> Result<Vec<FileAssociation>, RuleError> {
    if text.len() > MAX_BYTES {
        return Err(RuleError::list(RuleErrorKind::TooLarge));
    }
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rules: Vec<FileAssociation> =
        serde_json::from_str(text).map_err(|_| RuleError::list(RuleErrorKind::NotJson))?;
    check_rules(&rules)
}

/// The rules as the setting keeps them: compact JSON, empty for none.
#[must_use]
pub fn write_rules(rules: &[FileAssociation]) -> String {
    if rules.is_empty() {
        return String::new();
    }
    serde_json::to_string(rules).unwrap_or_default()
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
