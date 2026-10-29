//! What the lints read of a Rust source (RC-080-54): the code without its comments and the
//! insides of its literals, a call as a call of its own, and the statement a call stands in -
//! followed through the binding it is stored in. No parser: the sources are as rustfmt writes
//! them, and each helper's test shows the shapes it understands.

/// `source` with every comment and the inside of every string and character literal blanked to
/// spaces, newlines kept: a call named in a comment or a text counts for nothing, and every
/// line keeps its number.
pub(crate) fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        if c == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                out.push(' ');
                at += 1;
            }
        } else if c == '/' && next == Some('*') {
            at = block_comment(&chars, at, &mut out);
        } else if c == '"' {
            out.push('"');
            at = string(&chars, at + 1, &mut out);
        } else if c == 'r' && raw_string_starts(&chars, at) {
            at = raw_string(&chars, at, &mut out);
        } else if c == '\'' {
            at = quote(&chars, at, &mut out);
        } else {
            out.push(c);
            at += 1;
        }
    }
    out
}

fn blank(c: char) -> char {
    if c == '\n' { '\n' } else { ' ' }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A nested block comment from `at`; where the code goes on.
fn block_comment(chars: &[char], mut at: usize, out: &mut String) -> usize {
    let mut depth = 0_usize;
    while at < chars.len() {
        let next = chars.get(at + 1).copied();
        if chars[at] == '/' && next == Some('*') {
            depth += 1;
            out.push_str("  ");
            at += 2;
        } else if chars[at] == '*' && next == Some('/') {
            depth = depth.saturating_sub(1);
            out.push_str("  ");
            at += 2;
            if depth == 0 {
                break;
            }
        } else {
            out.push(blank(chars[at]));
            at += 1;
        }
    }
    at
}

/// The inside of a `"..."` from `at`, just past its opening quote, and its closing quote.
fn string(chars: &[char], mut at: usize, out: &mut String) -> usize {
    while at < chars.len() {
        match chars[at] {
            '\\' => {
                out.push(' ');
                if let Some(&escaped) = chars.get(at + 1) {
                    out.push(blank(escaped));
                }
                at += 2;
            }
            '"' => {
                out.push('"');
                return at + 1;
            }
            other => {
                out.push(blank(other));
                at += 1;
            }
        }
    }
    at
}

/// Whether the `r` at `at` opens a raw string (`r"`, `r#"`, `br"`), not ends a name.
fn raw_string_starts(chars: &[char], at: usize) -> bool {
    let before = at.checked_sub(1).map(|before| chars[before]);
    let prefix_ok = match before {
        None => true,
        Some('b') => at < 2 || !is_ident(chars[at - 2]),
        Some(before) => !is_ident(before),
    };
    let mut quote = at + 1;
    while chars.get(quote) == Some(&'#') {
        quote += 1;
    }
    prefix_ok && chars.get(quote) == Some(&'"')
}

/// A raw string from its `r`; where the code goes on.
fn raw_string(chars: &[char], at: usize, out: &mut String) -> usize {
    let mut at = at + 1;
    let mut hashes = 0;
    while chars.get(at) == Some(&'#') {
        hashes += 1;
        at += 1;
    }
    out.push('r');
    out.extend(std::iter::repeat_n('#', hashes));
    out.push('"');
    at += 1;
    while at < chars.len() {
        let closes =
            chars[at] == '"' && (1..=hashes).all(|offset| chars.get(at + offset) == Some(&'#'));
        if closes {
            out.push('"');
            out.extend(std::iter::repeat_n('#', hashes));
            return at + 1 + hashes;
        }
        out.push(blank(chars[at]));
        at += 1;
    }
    at
}

/// A character literal (`'x'`, `'\n'`, `'"'`) blanked, or a lifetime left as it is.
fn quote(chars: &[char], at: usize, out: &mut String) -> usize {
    out.push('\'');
    if chars.get(at + 1) == Some(&'\\') {
        let mut end = at + 3;
        while end < chars.len() && chars[end] != '\'' {
            end += 1;
        }
        out.extend(std::iter::repeat_n(' ', end.min(chars.len()) - (at + 1)));
        if end < chars.len() {
            out.push('\'');
        }
        return end + 1;
    }
    if chars.get(at + 2) == Some(&'\'') {
        out.push(' ');
        out.push('\'');
        return at + 3;
    }
    at + 1
}

/// Where `call` appears in `code` as a call of its own - not as the end of a longer name
/// (`a_checkbox(`); a path before it (`widget::checkbox(`) counts.
pub(crate) fn calls(code: &str, call: &str) -> Vec<usize> {
    code.match_indices(call)
        .map(|(at, _)| at)
        .filter(|at| {
            code[..*at]
                .chars()
                .next_back()
                .is_none_or(|before| !is_ident(before))
        })
        .collect()
}

/// The statement the expression at `start` stands in, from `start` to the `;` or `,` that ends
/// it at its own depth, the brace that opens a block after it, or the bracket that closes
/// round it: `run(Command::new(x).arg(y))` ends before the last `)`.
pub(crate) fn statement_from(code: &str, start: usize) -> &str {
    let mut depth = 0_usize;
    for (offset, c) in code[start..].char_indices() {
        match c {
            '(' | '[' => depth += 1,
            '{' if depth == 0 => return &code[start..start + offset],
            '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return &code[start..start + offset],
            ')' | ']' | '}' => depth -= 1,
            ';' | ',' if depth == 0 => return &code[start..start + offset],
            _ => {}
        }
    }
    &code[start..]
}

/// The name the expression at `at` is stored in when its line is `let [mut] name[: T] = ...`.
pub(crate) fn bound_name(code: &str, at: usize) -> Option<&str> {
    let line_start = code[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let before = code[line_start..at].trim();
    let binding = before.strip_prefix("let ")?.strip_suffix('=')?.trim();
    let binding = binding.strip_prefix("mut ").unwrap_or(binding).trim_start();
    let name = binding.split(':').next()?.trim();
    (!name.is_empty() && name.chars().all(is_ident)).then_some(name)
}

/// Whether the expression at `at` - its own statement, or a later statement of the name it is
/// bound to, before `end` - contains one of `needles`: `Command::new(x).no_console_window()`
/// and `let mut c = Command::new(x); c.arg(y).no_console_window();` both do.
pub(crate) fn chain_has(code: &str, at: usize, end: usize, needles: &[&str]) -> bool {
    let has = |statement: &str| needles.iter().any(|needle| statement.contains(needle));
    let own = statement_from(code, at);
    if has(own) {
        return true;
    }
    let Some(name) = bound_name(code, at) else {
        return false;
    };
    let from = (at + own.len()).min(end);
    calls(&code[..end], name)
        .into_iter()
        .filter(|use_at| *use_at >= from)
        .filter(|use_at| {
            let after = &code[use_at + name.len()..end];
            after.trim_start().starts_with('.')
                && !code[..*use_at].ends_with('.')
                && !after.starts_with(is_ident)
        })
        .any(|use_at| has(statement_from(code, use_at)))
}

/// The 1-based line of the byte offset `at`.
pub(crate) fn line_of(code: &str, at: usize) -> usize {
    code[..at].matches('\n').count() + 1
}

#[test]
fn comments_and_literals_are_blanked_and_lines_kept() {
    let source = "let a = \"x(); // no\"; // f(\n/* g( /* h( */ */ b('(', r#\"i(\"#, 'a);\nc";
    let code = code_only(source);
    assert_eq!(code.lines().count(), source.lines().count());
    assert!(!code.contains("f(") && !code.contains("g(") && !code.contains("h("));
    assert!(!code.contains("x(") && !code.contains("i("), "{code}");
    assert!(
        code.contains("b(") && code.contains("'a)"),
        "a lifetime stays: {code}"
    );
    assert!(code.ends_with("\nc"));
    assert_eq!(code_only("\"sftp://box\" x()"), "\"          \" x()");
    assert_eq!(code_only("for r in rows"), "for r in rows");
}

#[test]
fn a_statement_ends_at_its_own_depth() {
    let code = "run(Command::new(x)\n    .no_console_window()), other";
    let at = code.find("Command").expect("there");
    assert_eq!(
        statement_from(code, at),
        "Command::new(x)\n    .no_console_window()"
    );
    let code = "let c = Command::new(x).output() { body(); }";
    let at = code.find("Command").expect("there");
    assert_eq!(statement_from(code, at), "Command::new(x).output() ");
    assert_eq!(calls("a_b(); b(); x::b()", "b("), vec![7, 15]);
}

#[test]
fn a_binding_is_followed_to_its_later_statements() {
    let code = "let mut command = Command::new(x);\ncommand.arg(y).no_console_window();\n";
    let at = code.find("Command").expect("there");
    assert_eq!(bound_name(code, at), Some("command"));
    assert!(chain_has(code, at, code.len(), &[".no_console_window("]));
    let other = "let mut command = Command::new(x);\nother.no_console_window();\n";
    let at = other.find("Command").expect("there");
    assert!(!chain_has(other, at, other.len(), &[".no_console_window("]));
    let typed = "let sort: Element = mouse_area(x);";
    assert_eq!(
        bound_name(typed, typed.find("mouse").expect("there")),
        Some("sort")
    );
    assert_eq!(line_of("a\nb\nc", 4), 3);
}
