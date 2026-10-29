//! Guards the rule that no program rCommander starts opens a console window on Windows.
//!
//! The GUI binary has no console; a console program it starts without `CREATE_NO_WINDOW` opens a
//! visible window of its own (`git`, `wsl.exe`, `ssh`, `reg.exe`, ...). Each spawn asks through
//! `rc_process::NoConsoleWindow`, and this test scans `crates/*/src` for `Command::new(`: the
//! spawn's own statement, or a later statement of the name it is bound to in the same function
//! (`let mut c = Command::new(x); c.arg(y).no_console_window();`), must use one of [`ACCEPTED`]
//! (RC-080-54: a mention anywhere in the function, a comment included, no longer counts); the
//! call must sit under an attribute that keeps it off Windows ([`OFF_WINDOWS`]); or a comment
//! containing [`ALLOW`] on the line itself or the line above must say why this spawn shows a
//! window on purpose (an external editor, a terminal the user asked for). Test code is not
//! checked (`#[cfg(test)]`, `*_tests.rs`, `tests.rs`). The scan reads the code without its
//! comments and texts (`scan::code_only`), laid out as rustfmt writes it: an item ends at the
//! closing brace at its own indentation.
//!
//! A second rule: only [`CREATION_FLAGS_FILES`] call `creation_flags`, because the call replaces
//! the flags set before - a later one anywhere else would undo the helper unseen.

use std::path::{Path, PathBuf};

use crate::scan::{chain_has, code_only};
use crate::{files_with, relative, workspace_root};

/// What a function that starts a program uses so the program opens no window.
const ACCEPTED: &[&str] = &["no_console_window(", "CREATE_NO_WINDOW"];

/// The marker of a spawn that shows a window on purpose: `// console-window: allow (<reason>)`.
const ALLOW: &str = "console-window: allow";

/// Attributes under which an item is never compiled for Windows, or only for tests.
const OFF_WINDOWS: &[&str] = &[
    "#[cfg(unix)]",
    "#[cfg(not(windows))]",
    "#[cfg(target_os = \"linux\")]",
    "#[cfg(target_os = \"macos\")]",
    "#[cfg(test)]",
];

/// The helper itself.
const CREATION_FLAGS_FILES: &[&str] = &["crates/rc-process/src/lib.rs"];

/// Words that may stand before `fn` in a function's first line, besides a visibility
/// (`pub(...)`, any path in it) and an ABI after `extern` (`"C"`).
const QUALIFIERS: &[&str] = &["pub", "async", "const", "unsafe", "extern", "default"];

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// The last line of the item that starts at `start`: the line that ends it with `;` before any
/// brace, a line that opens and closes it, or else the closing brace at `indent`.
fn item_end(lines: &[&str], start: usize, indent: usize) -> usize {
    let last = lines.len().saturating_sub(1);
    let closing = format!("{}}}", " ".repeat(indent));
    for (index, line) in lines.iter().enumerate().skip(start) {
        let line = line.trim_end();
        if (line.ends_with(';') && !line.contains('{'))
            || (line.ends_with('}') && line.contains('{'))
        {
            return index;
        }
        if line.ends_with('{') {
            return lines
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, line)| line.trim_end() == closing)
                .map_or(last, |(end, _)| end);
        }
    }
    last
}

/// Whether `line` starts a function: `fn` after nothing but [`QUALIFIERS`], a visibility
/// such as `pub(in crate::platform)` and an ABI string.
fn is_fn_header(line: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        if rest.starts_with("fn ") {
            return true;
        }
        if let Some(inside) = rest.strip_prefix("pub(") {
            let Some(close) = inside.find(')') else {
                return false;
            };
            rest = inside[close + 1..].trim_start();
            continue;
        }
        if let Some(abi) = rest.strip_prefix('"') {
            let Some(close) = abi.find('"') else {
                return false;
            };
            rest = abi[close + 1..].trim_start();
            continue;
        }
        let word = rest.split_whitespace().next().unwrap_or_default();
        if word.is_empty() || !QUALIFIERS.contains(&word) {
            return false;
        }
        rest = rest[word.len()..].trim_start();
    }
}

/// Which lines lie in an item under one of [`OFF_WINDOWS`]: the attribute as `raw` has it, the
/// item's extent read from `code`.
fn off_windows_lines(raw: &[&str], code: &[&str]) -> Vec<bool> {
    let mut off = vec![false; code.len()];
    for (index, line) in raw.iter().enumerate() {
        if !OFF_WINDOWS.contains(&line.trim()) {
            continue;
        }
        let item = (index + 1..code.len())
            .find(|&next| {
                let next = code[next].trim_start();
                !next.is_empty() && !next.starts_with("#[")
            })
            .unwrap_or(index);
        let end = item_end(code, item, indent(line));
        off[index..=end].fill(true);
    }
    off
}

/// Whether the spawn on line `index` carries the allow marker, on its line or the one above.
fn allowed(lines: &[&str], index: usize) -> bool {
    lines[index].contains(ALLOW) || (index > 0 && lines[index - 1].contains(ALLOW))
}

/// How many spawns `source` has that are compiled for Windows, and the line numbers of those
/// that are neither guarded nor allowed.
fn unguarded_spawns(source: &str) -> (usize, Vec<usize>) {
    let raw = source.lines().collect::<Vec<_>>();
    let code = code_only(source);
    let lines = code.lines().collect::<Vec<_>>();
    let off = off_windows_lines(&raw, &lines);
    let functions = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_fn_header(line))
        .map(|(start, line)| (start, item_end(&lines, start, indent(line))))
        .collect::<Vec<_>>();
    // The byte offset at which each line ends, newline included.
    let mut line_ends = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        offset += line.len();
        line_ends.push(offset);
    }
    let mut spawns = 0;
    let mut unguarded = Vec::new();
    // Any `...Command::new(`, an alias such as `TokioCommand::new(` too.
    for (at, _) in code.match_indices("Command::new(") {
        let index = code[..at].matches('\n').count();
        if off.get(index).copied().unwrap_or(false) {
            continue;
        }
        spawns += 1;
        if allowed(&raw, index) {
            continue;
        }
        // A binding is followed to the end of the innermost function: the one that starts last.
        let end = functions
            .iter()
            .filter(|(start, end)| (*start..=*end).contains(&index))
            .max()
            .and_then(|(_, end)| line_ends.get(*end).copied())
            .unwrap_or(code.len());
        if !chain_has(&code, at, end, ACCEPTED) {
            unguarded.push(index + 1);
        }
    }
    (spawns, unguarded)
}

#[test]
fn no_spawn_opens_a_console_window() {
    let root = workspace_root();
    let mut sources: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("read crates") {
        files_with(
            &entry.expect("dir entry").path().join("src"),
            &[".rs"],
            &mut sources,
        );
    }
    assert!(!sources.is_empty(), "expected to scan crates/*/src");
    let mut unguarded = Vec::new();
    let mut flags = Vec::new();
    for path in sources.iter().filter(|path| !is_test_file(path)) {
        let relative = relative(&root, path);
        let source = std::fs::read_to_string(path).expect("read source");
        if source.contains("creation_flags(") && !CREATION_FLAGS_FILES.contains(&relative.as_str())
        {
            flags.push(relative.clone());
        }
        let (_, lines) = unguarded_spawns(&source);
        unguarded.extend(lines.into_iter().map(|line| format!("{relative}:{line}")));
    }
    assert!(
        unguarded.is_empty(),
        "a spawn without rc_process::NoConsoleWindow opens a console window on Windows \
         (or mark it `// {ALLOW} (<reason>)`):\n{}",
        unguarded.join("\n")
    );
    assert!(
        flags.is_empty(),
        "creation_flags replaces the console-window flag; use rc_process::NoConsoleWindow:\n{}",
        flags.join("\n")
    );
}

/// The scan itself: a bare spawn is found, at any depth, and a guarded, allowed or Linux-only one
/// is not.
#[test]
fn the_scan_finds_a_bare_spawn() {
    let source = r#"fn bare() {
    let _ = std::process::Command::new("reg.exe").status();
}

fn guarded() {
    let _ = std::process::Command::new("reg.exe").no_console_window().status();
}

#[cfg(target_os = "linux")]
fn linux_only() {
    let _ = std::process::Command::new("systemctl").status();
}

mod platform {
    pub(super) fn nested() -> Result<()> {
        run(Command::new("reg.exe")
            .no_console_window())
    }

    fn one_liner() -> u32 { 1 }

    fn inner_bare() {
        let _ = Command::new("icacls.exe").status();
    }

    fn editor() {
        // console-window: allow (the user's editor shows its own window)
        let _ = Command::new("notepad.exe").spawn();
    }
}
"#;
    assert_eq!(unguarded_spawns(source), (5, vec![2, 23]));
}

/// The gaps of the scan by function (RC-080-54): a `no_console_window(` anywhere in the body -
/// in a comment, or on another spawn - guarded every spawn of it, and a function behind
/// `pub(in ...)` or `extern "C"` was not seen as one. A binding guarded later still passes.
#[test]
fn each_spawn_is_judged_by_its_own_chain() {
    let source = r#"fn commented() {
    // no_console_window() is not called here
    let _ = Command::new("reg.exe").status();
}

fn two_spawns() {
    let _ = Command::new("reg.exe").status();
    let _ = Command::new("wsl.exe").no_console_window().status();
}

fn bound() -> Command {
    let mut command = Command::new("wsl.exe");
    command.arg("-l").no_console_window();
    command
}

fn bound_elsewhere() -> Command {
    let mut command = Command::new("wsl.exe");
    let text = "command.no_console_window()";
    command
}
"#;
    assert_eq!(unguarded_spawns(source), (5, vec![3, 7, 18]));
    for header in [
        "pub(in crate::platform) fn scoped() {",
        "    pub(crate) async fn run() {",
        "extern \"C\" fn callback() {",
        "pub unsafe extern \"system\" fn hook() {",
        "const fn limit() -> u32 {",
    ] {
        assert!(is_fn_header(header), "{header}");
    }
    for line in ["let fn_name = 1;", "// fn commented() {", "pub struct Fn;"] {
        assert!(!is_fn_header(line), "{line}");
    }
}
