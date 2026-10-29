//! One ssh_config line split into its keyword and arguments, the way OpenSSH's `argv_split`
//! does it: whitespace separates, double or single quotes group, a backslash escapes a quote, a
//! backslash or (outside quotes) a space, and an unquoted `#` at the start of an argument ends
//! the line. Any other backslash stays, so a Windows path needs no escaping.

/// One directive: the keyword as written and its arguments, quotes removed.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Directive {
    pub keyword: String,
    pub args: Vec<String>,
}

/// Why a line is not a directive.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum LineError {
    /// A quote opened and never closed.
    UnterminatedQuote,
    /// The line starts with `=`.
    MissingKeyword,
}

/// The directive on `line`, `None` for a blank or comment line. The keyword ends at whitespace
/// or `=`, so `Port 22`, `Port=22` and `Port = 22` are the same.
pub(super) fn split_line(line: &str) -> Result<Option<Directive>, LineError> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    if end == 0 {
        return Err(LineError::MissingKeyword);
    }
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    Ok(Some(Directive {
        keyword: line[..end].to_owned(),
        args: split_args(rest)?,
    }))
}

fn split_args(text: &str) -> Result<Vec<String>, LineError> {
    let mut args = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        match chars.peek() {
            None | Some('#') => break,
            Some(_) => {}
        }
        let mut arg = String::new();
        let mut quote = None;
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.peek() {
                    Some(&next @ ('\\' | '"' | '\'')) => {
                        arg.push(next);
                        chars.next();
                    }
                    Some(' ') if quote.is_none() => {
                        arg.push(' ');
                        chars.next();
                    }
                    _ => arg.push('\\'),
                },
                '"' | '\'' if quote == Some(c) => quote = None,
                '"' | '\'' if quote.is_none() => quote = Some(c),
                c if c.is_whitespace() && quote.is_none() => break,
                c => arg.push(c),
            }
        }
        if quote.is_some() {
            return Err(LineError::UnterminatedQuote);
        }
        args.push(arg);
    }
    Ok(args)
}

#[cfg(test)]
#[path = "lex_tests.rs"]
mod tests;
