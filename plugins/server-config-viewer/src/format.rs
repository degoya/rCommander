//! Which grammar a file is read in: by its name first - `nginx.conf`, `.htaccess`,
//! `sshd_config`, `crontab`, `hosts` - and for any other `.conf` by its first line that is no
//! comment, which is also how `probe` declines an INI-like `.conf`.
//!
//! The manifest claims every `*.conf` by a name pattern, asked after the config viewer's claim of
//! the extension `conf` (RC-100-80: associations, exact names, extensions, patterns, magic). The
//! config viewer's `probe` declines a `.conf` whose first line that is neither blank nor a
//! comment opens a block, ends a directive with `;` or is a tag; this plugin is asked next, and
//! [`sniff`] takes at least those - its acceptance is a superset of the config viewer's refusal,
//! so no server file falls through to the built-in viewer. A line starting with an Apache
//! directive such as `LoadModule` is taken too, for when it reaches this plugin. Anything else -
//! a section `[name]`, `key = value`, nothing to tell - is INI's and declined.

/// The grammar of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// nginx: statements ending in `;`, blocks in `{}`, `$variables`.
    Nginx,
    /// Apache httpd and `.htaccess`: a directive per line, sections `<Tag ...>`, `%{VAR}`.
    Apache,
    /// OpenSSH's `ssh_config` and `sshd_config`: `Keyword value`, `Host` and `Match` blocks.
    Ssh,
    /// A crontab: five time fields and a command, `@reboot`, `NAME=value`.
    Crontab,
    /// `hosts`: an address and its names.
    Hosts,
}

/// Apache's directives `sniff` knows a file by, as Apache writes them.
#[rustfmt::skip]
const APACHE_DIRECTIVES: &[&[u8]] = &[
    b"AddHandler", b"AddType", b"Alias", b"AllowOverride", b"CustomLog", b"DirectoryIndex",
    b"DocumentRoot", b"ErrorLog", b"Header", b"Include", b"IncludeOptional", b"KeepAlive",
    b"Listen", b"LoadModule", b"LogFormat", b"LogLevel", b"Options", b"ProxyPass",
    b"ProxyPassReverse", b"Redirect", b"RedirectMatch", b"Require", b"RewriteCond",
    b"RewriteEngine", b"RewriteRule", b"ScriptAlias", b"ServerAdmin", b"ServerAlias",
    b"ServerName", b"ServerRoot", b"ServerTokens", b"SetEnv", b"SSLCertificateFile",
    b"SSLEngine", b"WSGIScriptAlias",
];

impl Format {
    /// The grammar the name `name` (the file's name, never its folder) stands for, in any case;
    /// `None` for a `.conf` and every other name, which [`sniff`] decides.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        match name.as_str() {
            "nginx.conf" => Some(Self::Nginx),
            "httpd.conf" | "apache2.conf" | ".htaccess" => Some(Self::Apache),
            "ssh_config" | "sshd_config" => Some(Self::Ssh),
            "crontab" => Some(Self::Crontab),
            "hosts" => Some(Self::Hosts),
            _ if name.ends_with(".nginx") => Some(Self::Nginx),
            _ if name.ends_with(".htaccess") => Some(Self::Apache),
            _ => None,
        }
    }
}

/// Whether `name` ends in `.conf`, in any case.
#[must_use]
pub fn is_conf(name: &str) -> bool {
    name.len() > 5
        && name
            .get(name.len() - 5..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(".conf"))
}

/// The grammar the first line of `head` that is neither blank nor a comment (`#`, `;`) shows:
/// nginx, Apache, or - INI, or nothing to tell - `None`.
#[must_use]
pub fn sniff(head: &[u8]) -> Option<Format> {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let line = head
        .split(|byte| *byte == b'\n')
        .map(trim)
        .find(|line| !line.is_empty() && !line.starts_with(b"#") && !line.starts_with(b";"))?;
    line_format(line)
}

fn trim(line: &[u8]) -> &[u8] {
    let blank = |byte: &u8| byte.is_ascii_whitespace();
    let start = line
        .iter()
        .position(|byte| !blank(byte))
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|byte| !blank(byte))
        .map_or(start, |end| end + 1);
    line.get(start..end).unwrap_or_default()
}

/// The grammar one trimmed line shows: a tag (`<VirtualHost`, `</Directory>`) or a line starting
/// with an Apache directive is Apache's; a line opening a block (`server {`, `if ($a = b) {`) or
/// ending a directive with `;` and holding no `=` (`worker_processes auto;`) nginx's.
fn line_format(line: &[u8]) -> Option<Format> {
    let tag = line.first() == Some(&b'<')
        && line
            .get(1)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'/');
    let word_end = line
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(line.len());
    let (word, rest) = line.split_at(word_end);
    let directive =
        APACHE_DIRECTIVES.contains(&word) && rest.first().is_some_and(u8::is_ascii_whitespace);
    if tag || directive {
        return Some(Format::Apache);
    }
    let statement = line.ends_with(b";") && !line.contains(&b'=');
    (line.ends_with(b"{") || statement).then_some(Format::Nginx)
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
