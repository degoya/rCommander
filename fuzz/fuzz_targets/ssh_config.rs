//! An OpenSSH client configuration as the connection import reads it
//! (`rc_engine::import::ssh_config::parse_str`): the bytes decoded as the import decodes a file,
//! then read. It is an error or the candidates, never a panic, and the import's bounds hold: at
//! most `MAX_HOSTS` hosts and as many skipped names, per host at most `MAX_IDENTITY_FILES` key
//! paths and `MAX_IGNORED_OPTIONS` ignored keywords, no alias twice (in any case) and none that
//! is a pattern.
//!
//! `Include` lines are left out before the text is read: an include is read from disk, and a
//! glob like `/*/*/*` would walk the fuzzing machine's file system. The include's own bounds have
//! their tests in the crate.

#![no_main]

use std::{collections::HashSet, path::PathBuf, sync::OnceLock};

use libfuzzer_sys::fuzz_target;
use rc_engine::import::ssh_config::{
    MAX_HOSTS, MAX_IDENTITY_FILES, MAX_IGNORED_OPTIONS, SshConfigContext, parse_str,
};

/// A home that holds nothing: `~` and `%d` expand to it, nothing is read from it.
fn home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| std::env::temp_dir().join(format!("rc-fuzz-ssh-{}", std::process::id())))
}

/// Whether `line` is an `Include` directive, split as the import's lexer splits a keyword:
/// leading white space off, the keyword up to white space or `=`.
fn is_include(line: &str) -> bool {
    let line = line.trim_start();
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    line[..end].eq_ignore_ascii_case("include")
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let text: String = text
        .lines()
        .filter(|line| !is_include(line))
        .flat_map(|line| [line, "\n"])
        .collect();
    let context = SshConfigContext::new(home(), "fuzz");
    let Ok(import) = parse_str(&text, &context.ssh_dir.join("config"), &context) else {
        return;
    };
    assert!(
        import.hosts.len() <= MAX_HOSTS,
        "{} hosts",
        import.hosts.len()
    );
    assert!(
        import.skipped.len() <= MAX_HOSTS,
        "{} skipped",
        import.skipped.len()
    );
    let mut aliases = HashSet::new();
    for host in &import.hosts {
        let alias = &host.alias;
        assert!(
            aliases.insert(alias.to_ascii_lowercase()),
            "the alias {alias:?} twice"
        );
        assert!(
            !alias.starts_with('!') && !alias.contains(['*', '?']),
            "the pattern {alias:?} is a host"
        );
        assert!(
            host.identity_files.len() <= MAX_IDENTITY_FILES,
            "{alias:?} keeps {} key paths",
            host.identity_files.len()
        );
        assert!(
            host.ignored_options.len() <= MAX_IGNORED_OPTIONS,
            "{alias:?} lists {} ignored keywords",
            host.ignored_options.len()
        );
    }
});
