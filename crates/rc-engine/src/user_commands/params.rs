//! Total Commander's placeholders in a program's parameters (RC-100-86, TC help "Buttonbar"):
//! `%P` the source folder with its separator, `%N` the name under the cursor, `%T` the target
//! folder, `%M` the name under the cursor there, `%O` the name without its extension, `%E` the
//! extension without the dot, `%S` the marked names of the source (the name under the cursor when
//! nothing is marked, nothing with `%Y`), `%R` those of the target, `%P%S` and `%T%R` each name
//! with its folder, `%%` a percent sign, `%L %F %UL %UF %WL %WF` the list files
//! (`list_files.rs`). On Windows `%S`, `%P%S` and `%R` put each name holding a blank in double
//! quotes ("Do NOT put quotes around %P%S yourself!"), which `%Q` switches off. The 8.3 variants
//! (`%n %p %t ...`) are not offered: a lower-case letter stays as written, as any `%` that names
//! nothing does.
//!
//! Windows programs read their own command line, so there the parameters are one text with the
//! values put in ([`expand_windows`]). Elsewhere the parameters are split into words first and
//! the values put into the words ([`expand_posix`]) - a name with a blank, a quote or a `;` stays
//! one argument and never becomes shell syntax; a word with `%S` becomes one word per name.

use rc_rename::split_extension;

use super::{UserCommandError, list_files::ListKind};

/// Which panel a placeholder reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    /// The panel the command runs for: `%P %N %O %E %S`.
    Source,
    /// The other one: `%T %M %R`.
    Target,
}

/// What the placeholders read from the two panels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParamContext {
    /// The source panel's folder; `None` when it shows no local folder (an archive, a server).
    pub source_dir: Option<String>,
    /// The name under the source panel's cursor; `None` in an empty folder.
    pub source_cursor: Option<String>,
    /// The marked names of the source panel in panel order; empty when none is marked.
    pub source_marked: Vec<String>,
    pub target_dir: Option<String>,
    pub target_cursor: Option<String>,
    pub target_marked: Vec<String>,
    /// The list files written for this start, each with its path.
    pub lists: Vec<(ListKind, String)>,
    /// The separator a folder ends with: `\` on Windows, `/` elsewhere.
    pub separator: char,
}

impl ParamContext {
    /// The folder of `side`, ending in the separator.
    fn dir(&self, side: PanelSide) -> Result<String, UserCommandError> {
        let dir = match side {
            PanelSide::Source => &self.source_dir,
            PanelSide::Target => &self.target_dir,
        };
        let dir = dir.as_ref().ok_or(UserCommandError::LocalOnly {
            panel: side_name(side),
        })?;
        if dir.ends_with(['\\', '/']) {
            Ok(dir.clone())
        } else {
            Ok(format!("{dir}{}", self.separator))
        }
    }

    /// Refuses a name placeholder of `side` when it shows no local folder: a name on a server
    /// or in an archive means nothing to a local program, and a server's name may hold a `"`.
    fn require_local(&self, side: PanelSide) -> Result<(), UserCommandError> {
        self.dir(side).map(|_| ())
    }

    /// The name under the cursor of `side`; empty in an empty folder.
    ///
    /// # Errors
    ///
    /// [`UserCommandError::LocalOnly`] for a side without a local folder.
    fn cursor(&self, side: PanelSide) -> Result<String, UserCommandError> {
        self.require_local(side)?;
        Ok(match side {
            PanelSide::Source => self.source_cursor.clone(),
            PanelSide::Target => self.target_cursor.clone(),
        }
        .unwrap_or_default())
    }

    /// The names `%S` (or `%R`) stands for: the marked ones, else the one under the cursor -
    /// none with `%Y`.
    pub fn names(&self, side: PanelSide, empty_when_unmarked: bool) -> Vec<String> {
        let (marked, cursor) = match side {
            PanelSide::Source => (&self.source_marked, &self.source_cursor),
            PanelSide::Target => (&self.target_marked, &self.target_cursor),
        };
        if !marked.is_empty() {
            return marked.clone();
        }
        if empty_when_unmarked {
            return Vec::new();
        }
        cursor.iter().cloned().collect()
    }
}

/// `source` or `target`, as an error names the panel.
fn side_name(side: PanelSide) -> &'static str {
    match side {
        PanelSide::Source => "source",
        PanelSide::Target => "target",
    }
}

/// One placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Var {
    Dir(PanelSide),
    Cursor(PanelSide),
    BaseName,
    Extension,
    Names(PanelSide),
    /// `%P%S`, `%T%R`: each name with its folder.
    Paths(PanelSide),
    List(ListKind),
}

/// A piece of the parameters: text as written, or a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Var(Var),
}

/// The switches the parameters set anywhere in them.
#[derive(Debug, Clone, Copy, Default)]
struct Switches {
    /// `%Q`: no quotes around names with blanks.
    no_quotes: bool,
    /// `%Y`: `%S`, `%R` and the lists are empty when nothing is marked.
    empty_when_unmarked: bool,
}

/// `text` as pieces, and the switches it sets.
fn scan(text: &str) -> (Vec<Piece>, Switches) {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut switches = Switches::default();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        literal.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let (var, used) = match after.as_bytes() {
            [b'%', ..] => {
                literal.push('%');
                rest = &after[1..];
                continue;
            }
            [b'Q', ..] => {
                switches.no_quotes = true;
                rest = &after[1..];
                continue;
            }
            [b'Y', ..] => {
                switches.empty_when_unmarked = true;
                rest = &after[1..];
                continue;
            }
            [b'U', b'L', ..] => (Some(Var::List(ListKind::PathsUtf8)), 2),
            [b'U', b'F', ..] => (Some(Var::List(ListKind::NamesUtf8)), 2),
            [b'W', b'L', ..] => (Some(Var::List(ListKind::PathsUtf16)), 2),
            [b'W', b'F', ..] => (Some(Var::List(ListKind::NamesUtf16)), 2),
            [b'P', b'%', b'S', ..] => (Some(Var::Paths(PanelSide::Source)), 3),
            [b'T', b'%', b'R', ..] => (Some(Var::Paths(PanelSide::Target)), 3),
            [b'P', ..] => (Some(Var::Dir(PanelSide::Source)), 1),
            [b'N', ..] => (Some(Var::Cursor(PanelSide::Source)), 1),
            [b'T', ..] => (Some(Var::Dir(PanelSide::Target)), 1),
            [b'M', ..] => (Some(Var::Cursor(PanelSide::Target)), 1),
            [b'O', ..] => (Some(Var::BaseName), 1),
            [b'E', ..] => (Some(Var::Extension), 1),
            [b'S', ..] => (Some(Var::Names(PanelSide::Source)), 1),
            [b'R', ..] => (Some(Var::Names(PanelSide::Target)), 1),
            [b'L', ..] => (Some(Var::List(ListKind::PathsAnsi)), 1),
            [b'F', ..] => (Some(Var::List(ListKind::NamesAnsi)), 1),
            _ => (None, 0),
        };
        match var {
            Some(var) => {
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                pieces.push(Piece::Var(var));
                rest = &after[used..];
            }
            None => {
                // A `%` that names nothing stays as written.
                literal.push('%');
                rest = after;
            }
        }
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    (pieces, switches)
}

/// Whether `params` hold `%Y`: `%S`, `%R` and the lists stay empty when nothing is marked.
#[must_use]
pub fn marks_only(params: &str) -> bool {
    scan(params).1.empty_when_unmarked
}

/// The list files `params` asks for, each once, in the order they appear.
#[must_use]
pub fn lists_in(params: &str) -> Vec<ListKind> {
    let mut kinds = Vec::new();
    for piece in scan(params).0 {
        if let Piece::Var(Var::List(kind)) = piece
            && !kinds.contains(&kind)
        {
            kinds.push(kind);
        }
    }
    kinds
}

/// The parameters with their placeholders filled in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expanded {
    /// Windows: the one text of the parameters. Elsewhere: the words, one argument each.
    pub args: Vec<String>,
    /// Every value put in (names, folders, list files), for the batch-file check.
    pub values: Vec<String>,
}

/// The value of a placeholder that stands for one text.
fn single(var: Var, context: &ParamContext) -> Result<String, UserCommandError> {
    match var {
        Var::Dir(side) => context.dir(side),
        Var::Cursor(side) => context.cursor(side),
        Var::BaseName => Ok(split_extension(&context.cursor(PanelSide::Source)?)
            .0
            .to_owned()),
        Var::Extension => Ok(split_extension(&context.cursor(PanelSide::Source)?)
            .1
            .to_owned()),
        Var::List(kind) => context
            .lists
            .iter()
            .find(|(written, _)| *written == kind)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| UserCommandError::Invalid {
                reason: "a list file was not written".to_owned(),
            }),
        Var::Names(_) | Var::Paths(_) => Err(UserCommandError::Invalid {
            reason: "a list of names is no single value".to_owned(),
        }),
    }
}

/// The names of `%S`/`%R` (`Names`) or `%P%S`/`%T%R` (`Paths`).
fn many(
    var: Var,
    context: &ParamContext,
    switches: Switches,
) -> Result<Vec<String>, UserCommandError> {
    match var {
        Var::Names(side) => {
            context.require_local(side)?;
            Ok(context.names(side, switches.empty_when_unmarked))
        }
        Var::Paths(side) => {
            let dir = context.dir(side)?;
            Ok(context
                .names(side, switches.empty_when_unmarked)
                .into_iter()
                .map(|name| format!("{dir}{name}"))
                .collect())
        }
        _ => Ok(vec![single(var, context)?]),
    }
}

/// Refuses a value put into a Windows command line that holds a double quote.
fn no_quote(value: &str) -> Result<(), UserCommandError> {
    if value.contains('"') {
        return Err(UserCommandError::Invalid {
            reason: "a name holds a double quote".to_owned(),
        });
    }
    Ok(())
}

/// `params` as one text for a Windows program: each value put in, names with a blank in double
/// quotes unless `%Q`. A local Windows name holds no double quote; a value that does (from
/// anywhere) would end the quotes around it and add arguments of its own, so it refuses the
/// start.
///
/// # Errors
///
/// A placeholder of a panel without a local folder ([`UserCommandError::LocalOnly`]), or a value
/// with a double quote ([`UserCommandError::Invalid`]).
pub fn expand_windows(params: &str, context: &ParamContext) -> Result<Expanded, UserCommandError> {
    let (pieces, switches) = scan(params);
    let mut text = String::new();
    let mut values = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Text(literal) => text.push_str(&literal),
            Piece::Var(var @ (Var::Names(_) | Var::Paths(_))) => {
                let names = many(var, context, switches)?;
                names.iter().try_for_each(|name| no_quote(name))?;
                let quoted: Vec<String> = names
                    .iter()
                    .map(|name| {
                        if !switches.no_quotes && name.contains(char::is_whitespace) {
                            format!("\"{name}\"")
                        } else {
                            name.clone()
                        }
                    })
                    .collect();
                text.push_str(&quoted.join(" "));
                values.extend(names);
            }
            Piece::Var(var) => {
                let value = single(var, context)?;
                no_quote(&value)?;
                text.push_str(&value);
                values.push(value);
            }
        }
    }
    let args = if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![text]
    };
    Ok(Expanded { args, values })
}

/// `params` as the words of a program elsewhere: split first (blanks between words, `'...'` and
/// `"..."` keep blanks, a backslash keeps the next character), then each value put into its word.
/// A word with `%S`, `%R`, `%P%S` or `%T%R` becomes one word per name, none when there is none.
///
/// # Errors
///
/// A placeholder of a panel without a local folder, or an unclosed quote
/// ([`UserCommandError::Invalid`]).
pub fn expand_posix(params: &str, context: &ParamContext) -> Result<Expanded, UserCommandError> {
    let switches = scan(params).1;
    let mut args = Vec::new();
    let mut values = Vec::new();
    for word in words(params)? {
        let pieces = scan(&word).0;
        let list = pieces.iter().find_map(|piece| match piece {
            Piece::Var(var @ (Var::Names(_) | Var::Paths(_))) => Some(*var),
            _ => None,
        });
        let names = match list {
            Some(var) => many(var, context, switches)?,
            None => vec![String::new()],
        };
        for name in names {
            let mut arg = String::new();
            for piece in &pieces {
                match piece {
                    Piece::Text(literal) => arg.push_str(literal),
                    Piece::Var(var) if Some(*var) == list => {
                        arg.push_str(&name);
                        values.push(name.clone());
                    }
                    Piece::Var(var @ (Var::Names(_) | Var::Paths(_))) => {
                        let others = many(*var, context, switches)?;
                        arg.push_str(&others.join(" "));
                        values.extend(others);
                    }
                    Piece::Var(var) => {
                        let value = single(*var, context)?;
                        arg.push_str(&value);
                        values.push(value);
                    }
                }
            }
            args.push(arg);
        }
    }
    Ok(Expanded { args, values })
}

/// `text` split into words as a shell would split them, without any of its other syntax.
fn words(text: &str) -> Result<Vec<String>, UserCommandError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err(unclosed()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(unclosed()),
                        },
                        Some(inner) => word.push(inner),
                        None => return Err(unclosed()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

fn unclosed() -> UserCommandError {
    UserCommandError::Invalid {
        reason: "the parameters hold a quote that is not closed".to_owned(),
    }
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
