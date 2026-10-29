//! Finding credentials in strings, ported from rDownloader's `rd-core/src/redact.rs`.
//!
//! Two users: every line the error log stores (`error_log::layer`) and every MCP answer
//! (`rc-mcp`'s mask). It lives here, the lowest crate both reach, so the database never has to
//! trust a caller to have masked a message before it is written.
//!
//! Two invariants hold, as there: a string with nothing to hide comes back byte for byte, and
//! redacting twice equals redacting once. Names stay, only values are replaced, so an answer
//! still says *that* an address carried a token.
//!
//! What it finds: the user info and secret query values of an address
//! (`ftp://user:pass@host`, `https://host/?token=…`), credential header lines
//! (`Authorization: …`), a `Bearer`/`Basic`/`Digest` value anywhere, and a secret's value after
//! its name in every form a file, a terminal or a server writes it (RC-080-42): `password=x`,
//! `password = x`, `password: x` (YAML), `"password": "x"` (JSON), `DB_PASSWORD=x` (a name with
//! a secret's part, as `rc-mcp`'s argument gate reads names). The colon forms take only names
//! that are a credential's beyond doubt - `key: F5` and `session: 3` stay - and never a
//! structured value (`[`, `{`) or `true`, `false`, `null`; documentation such as
//! `password: see vault` is masked too, which is the price of the rule.

use url::Url;

/// What every redacted value becomes. Free of characters `form_urlencoded` would change, so a
/// second pass writes the same string.
pub const PLACEHOLDER: &str = "[redacted]";

/// Parameter and field names whose value is a credential, matched case-insensitively on the
/// whole name. Checksum names (`md5`, `hash`) are absent on purpose: a file manager reports
/// checksums.
const SECRET_NAMES: &[&str] = &[
    "access_key",
    "access_token",
    "api-key",
    "api_key",
    "apikey",
    "auth",
    "auth_key",
    "auth_token",
    "authorization",
    "aws_secret_access_key",
    "aws_session_token",
    "client_secret",
    "github_token",
    "key",
    "passphrase",
    "passwd",
    "password",
    "private_key",
    "private_token",
    "pwd",
    "refresh_token",
    "secret",
    "secret_key",
    "session",
    "sid",
    "sig",
    "signature",
    "token",
    "x-amz-credential",
    "x-amz-security-token",
    "x-amz-signature",
    "x-api-key",
];

/// Parts of a name that make it a secret's wherever they stand in it (`DB_PASSWORD`,
/// `client_secret_2`), as `rc-mcp`'s argument gate reads names.
const SECRET_PARTS: &[&str] = &["passphrase", "passwd", "password", "private_key", "secret"];

/// Words of a name, between `_`, `-` and `.`, that make it a secret's (`MY_API_TOKEN`,
/// `mysql.pwd`); as words only, so `max_tokens` stays.
const SECRET_WORDS: &[&str] = &["apikey", "pwd", "token"];

/// Names of [`SECRET_NAMES`] too common in prose and answers for the colon forms: `key: F5`,
/// `session: 3`, `signature: good` stay.
const COLON_AMBIGUOUS: &[&str] = &["auth", "key", "session", "sid", "sig", "signature"];

/// Header names whose value is always a credential.
const CREDENTIAL_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "proxy-authorization",
    "set-cookie",
];

/// Authentication schemes whose parameter is a credential.
const CREDENTIAL_SCHEMES: &[&str] = &["bearer ", "basic ", "digest "];

/// Address schemes searched for inside prose.
const SCHEMES: &[&str] = &[
    "https://",
    "http://",
    "ftps://",
    "ftp://",
    "sftp://",
    "ssh://",
    "smb://",
    "webdav://",
    "davs://",
    "socks5://",
];

/// Whether `name` is the name of a credential.
pub fn is_secret_name(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    SECRET_NAMES.contains(&name.as_str())
}

/// Whether `name` holds a secret's part or word ([`SECRET_PARTS`], [`SECRET_WORDS`]).
fn has_secret_part(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_PARTS.iter().any(|part| name.contains(part))
        || name
            .split(['_', '-', '.'])
            .any(|word| SECRET_WORDS.contains(&word))
}

/// Whether the value after `name` and `separator` is a secret: `=` after any secret's name,
/// `:` only after one that is beyond doubt.
fn names_a_secret(name: &str, separator: char) -> bool {
    if has_secret_part(name) {
        return true;
    }
    is_secret_name(name)
        && (separator == '=' || !COLON_AMBIGUOUS.contains(&name.to_ascii_lowercase().as_str()))
}

/// The address with its user info and secret query values replaced; unchanged otherwise.
pub fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if !url.username().is_empty() || url.password().is_some() {
        let _ = redacted.set_password(None);
        // Bracket-free, so a second pass does not percent-encode it again.
        let _ = redacted.set_username("redacted");
    }
    let mut found = false;
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(name, value)| {
            if is_secret_name(&name) {
                found = true;
                (name.into_owned(), PLACEHOLDER.to_owned())
            } else {
                (name.into_owned(), value.into_owned())
            }
        })
        .collect();
    if found {
        redacted.query_pairs_mut().clear().extend_pairs(pairs);
    }
    redacted.into()
}

/// Free text with every credential it carries replaced, line by line.
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, terminator) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        out.push_str(&redact_line(body));
        out.push_str(terminator);
    }
    out
}

fn redact_line(line: &str) -> String {
    if let Some(position) = line.find(':') {
        let name = line[..position].trim();
        if CREDENTIAL_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            return format!("{}: {PLACEHOLDER}", line[..position].trim_end());
        }
    }
    redact_inline(line)
}

/// Addresses, auth schemes and `secret=value` inside one line. Scans by byte index; every
/// marker is ASCII, so the slices always fall on character boundaries.
fn redact_inline(line: &str) -> String {
    let lowered = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        if SCHEMES
            .iter()
            .any(|scheme| lowered[index..].starts_with(scheme))
        {
            let end = token_end(line, index);
            let token = &line[index..end];
            match Url::parse(token) {
                Ok(url) if redact_url(&url) != url.as_str() => out.push_str(&redact_url(&url)),
                _ => out.push_str(token),
            }
            index = end;
            continue;
        }
        if is_word_start(line, index) {
            if let Some(scheme) = CREDENTIAL_SCHEMES
                .iter()
                .find(|scheme| lowered[index..].starts_with(**scheme))
            {
                let value_start = index + scheme.len();
                let value_end = token_end(line, value_start);
                if value_end > value_start {
                    out.push_str(&line[index..value_start]);
                    out.push_str(PLACEHOLDER);
                    index = value_end;
                    continue;
                }
            }
            if let Some(next) = redact_assignment(line, index, &mut out) {
                index = next;
                continue;
            }
        }
        let next = line[index..]
            .chars()
            .next()
            .map_or(line.len(), |character| index + character.len_utf8());
        out.push_str(&line[index..next]);
        index = next;
    }
    out
}

/// A secret's value after its name at `start`, in any of the forms the module names: writes the
/// name, the separator and [`PLACEHOLDER`] (inside the value's quotes when it has them) and
/// returns where the value ended; `None` when there is no such value at `start`.
fn redact_assignment(line: &str, start: usize, out: &mut String) -> Option<usize> {
    let name_len = line[start..]
        .find(|character: char| !(character.is_ascii_alphanumeric() || "_-.".contains(character)))
        .unwrap_or(line.len() - start);
    if name_len == 0 {
        return None;
    }
    let name = &line[start..start + name_len];
    let mut at = start + name_len;
    // A quoted name (`"password": ...`) closes with the quote it opened with.
    if let Some(quote) = line[..start]
        .chars()
        .next_back()
        .filter(|c| "\"'".contains(*c))
        && line[at..].starts_with(quote)
    {
        at += quote.len_utf8();
    }
    at += blanks(&line[at..]);
    let separator = line[at..].chars().next().filter(|c| "=:".contains(*c))?;
    if !names_a_secret(name, separator) {
        return None;
    }
    let value_start = at + 1 + blanks(&line[at + 1..]);
    let rest = &line[value_start..];
    let first = rest.chars().next()?;
    // `Authorization: Bearer x`: the scheme's rule takes the credential, not its scheme word.
    let lowered = rest.get(..8).unwrap_or(rest).to_ascii_lowercase();
    if CREDENTIAL_SCHEMES
        .iter()
        .any(|scheme| lowered.starts_with(scheme))
    {
        return None;
    }
    if first == '"' || first == '\'' {
        let inner = value_start + 1;
        let close = closing_quote(&line[inner..], first).map_or(line.len(), |end| inner + end);
        if close == inner {
            return None;
        }
        out.push_str(&line[start..inner]);
        out.push_str(PLACEHOLDER);
        return Some(close);
    }
    if separator == ':' {
        let word = rest
            .split(|c: char| c.is_whitespace() || ",;}".contains(c))
            .next()
            .unwrap_or_default();
        if first == '[' || first == '{' || ["true", "false", "null"].contains(&word) {
            return None;
        }
    }
    let stops: &[char] = if separator == ':' {
        &['&', ';', ',', '"', '\'', '<', '>', '}']
    } else {
        &['&', ';', ',', '"', '\'', '<', '>']
    };
    let value_end = rest
        .find(|character: char| character.is_whitespace() || stops.contains(&character))
        .map_or(line.len(), |offset| value_start + offset);
    if value_end == value_start {
        return None;
    }
    out.push_str(&line[start..value_start]);
    out.push_str(PLACEHOLDER);
    Some(value_end)
}

/// The length of the spaces and tabs `text` starts with.
fn blanks(text: &str) -> usize {
    text.len() - text.trim_start_matches([' ', '\t']).len()
}

/// Where the `quote` that closes a quoted value lies in `text` (after its opening quote); a
/// quote after a backslash does not close it.
fn closing_quote(text: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (offset, character) in text.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            _ if character == quote => return Some(offset),
            _ => {}
        }
    }
    None
}

/// Whether `index` starts a word, so `member` does not look like `basic `.
fn is_word_start(line: &str, index: usize) -> bool {
    line[..index]
        .chars()
        .next_back()
        .is_none_or(|previous| !previous.is_alphanumeric() && previous != '-' && previous != '_')
}

/// End of an address-like token: stops at whitespace or a delimiter, then gives back trailing
/// sentence punctuation (never the bracket of a placeholder written earlier).
fn token_end(line: &str, start: usize) -> usize {
    let mut end = line[start..]
        .find(|character: char| character.is_whitespace() || "\"'<>`\\|".contains(character))
        .map_or(line.len(), |offset| start + offset);
    while end > start && !line[start..end].ends_with(PLACEHOLDER) {
        match line[start..end].chars().next_back() {
            Some(last) if ".,;:!?)]}".contains(last) => end -= last.len_utf8(),
            _ => break,
        }
    }
    end
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
