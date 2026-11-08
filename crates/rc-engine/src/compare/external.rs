//! The external compare tool's arguments (RC-100-94): Total Commander's `CompareTool=` starts a
//! program with the two files; rCommander also takes its arguments (`compare.external_args`,
//! default `"%1" "%2"`). The text is split as a command line is: words at white space, double
//! quotes keep spaces in one; `%1` is the left file and `%2` the right one wherever they stand,
//! `%%` a percent sign. Every word is one argument of its own - no shell ever reads it, so a
//! name with spaces or quotes stays one argument.

use std::{ffi::OsString, path::Path};

/// The arguments when the setting is empty: both files.
pub const DEFAULT_ARGS: &str = "\"%1\" \"%2\"";

/// A part of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Left,
    Right,
}

/// Why an argument text does not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgsError {
    /// A double quote is opened and never closed.
    OpenQuote,
}

/// The words of `template`, each as its pieces.
fn words(template: &str) -> Result<Vec<Vec<Piece>>, ArgsError> {
    let mut words = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    // A word was started: `""` is an empty argument, not none.
    let mut started = false;
    let mut quoted = false;
    let mut chars = template.chars().peekable();
    let push_text = |word: &mut Vec<Piece>, c: char| match word.last_mut() {
        Some(Piece::Text(text)) => text.push(c),
        _ => word.push(Piece::Text(c.to_string())),
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '%' => {
                started = true;
                match chars.peek() {
                    Some('1') => {
                        chars.next();
                        word.push(Piece::Left);
                    }
                    Some('2') => {
                        chars.next();
                        word.push(Piece::Right);
                    }
                    Some('%') => {
                        chars.next();
                        push_text(&mut word, '%');
                    }
                    _ => push_text(&mut word, '%'),
                }
            }
            c => {
                started = true;
                push_text(&mut word, c);
            }
        }
    }
    if quoted {
        return Err(ArgsError::OpenQuote);
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

/// Checks an argument text as the setting stores it.
///
/// # Errors
///
/// A quote is not closed.
pub fn check(template: &str) -> Result<(), ArgsError> {
    words(template).map(|_| ())
}

/// The arguments `template` gives for `left` and `right`; an empty template is
/// [`DEFAULT_ARGS`].
///
/// # Errors
///
/// A quote is not closed.
pub fn arguments(template: &str, left: &Path, right: &Path) -> Result<Vec<OsString>, ArgsError> {
    let template = if template.trim().is_empty() {
        DEFAULT_ARGS
    } else {
        template
    };
    Ok(words(template)?
        .into_iter()
        .map(|word| {
            let mut argument = OsString::new();
            for piece in word {
                match piece {
                    Piece::Text(text) => argument.push(text),
                    Piece::Left => argument.push(left),
                    Piece::Right => argument.push(right),
                }
            }
            argument
        })
        .collect())
}

#[cfg(test)]
#[path = "external_tests.rs"]
mod tests;
