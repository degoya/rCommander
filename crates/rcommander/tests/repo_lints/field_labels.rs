//! Guards the rule that a dialog's text field has a visible label (RC-110-78, WCAG 3.3.2): a
//! placeholder is no label - it is gone with the first letter typed, and iced draws it pale - so
//! the field's name stands beside or over it. A view under `crates/rcommander/src/app/` (the
//! code without its comments and texts) gives each `text_input(` a label the lint can see:
//!
//! - its caption's access key: the field, or the name it is bound to, inside
//!   `focus::Stop::field(` (`theme::layout::captioned`, `keyed`);
//! - a label call - [`LABELS`] - on its line or the [`LOOKBACK`] lines above it, or above a later
//!   use of the name it is bound to in the same function: `row![caption(..), field]`,
//!   `labeled(t!(..), name)`;
//! - or the marker [`MARKER`] with where its label is, on its line or one of the two above:
//!   `// field-label: the form's label column beside it`. A helper that makes the fields of its
//!   callers says `// field-label: each call ...`; then each call of it in its file's folder is
//!   held to the same rule.
//!
//! A field with a placeholder draws it readable: `.style(theme::text_field)` on the field
//! (4.5:1 on the field, where iced's own grey fell to 2.95:1). Static reading cannot see a
//! layout, so the lint is a floor, not a proof: the dialogs are looked at on Windows.

use std::path::Path;

use crate::scan::{bound_name, calls, chain_has, code_only, line_of, statement_from};
use crate::{files_with, relative, workspace_root};

/// The views checked.
const APP: &str = "crates/rcommander/src/app";

/// Views whose fields are no dialog's, each with its reason.
const EXEMPT: &[(&str, &str)] = &[
    (
        "crates/rcommander/src/app/viewer/draw.rs",
        "the Lister's search and go-to bars: no dialog, the bar's own keys",
    ),
    (
        "crates/rcommander/src/app/palette/view.rs",
        "the command palette: its one field is the whole overlay",
    ),
];

/// Where a field says its label is: `// field-label: <where>`.
const MARKER: &str = "field-label:";
/// The marker of a helper whose callers name its fields.
const EACH_CALL: &str = "field-label: each call";
/// The calls that draw a label beside or over a field.
const LABELS: &[&str] = &[
    "label(",
    "caption(",
    "captioned(",
    "captioned_beside(",
    "labeled(",
    "labelled(",
    "line(",
];
/// How many lines above a field its label may stand.
const LOOKBACK: usize = 3;
/// The stop that keys a field to its caption.
const STOP: &str = "Stop::field(";

/// Whether `line` starts a function: `fn`, `pub fn`, `pub(super) const fn`, ...
fn starts_a_function(line: &str) -> bool {
    let blank = |rest: &str| rest.starts_with([' ', '\t']);
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        let after = match after.strip_prefix('(') {
            Some(inner) => match inner.split_once(')') {
                Some((_, after)) => after,
                None => return false,
            },
            None => after,
        };
        if !blank(after) {
            return false;
        }
        rest = after.trim_start();
    }
    for word in ["const", "async"] {
        if let Some(after) = rest.strip_prefix(word)
            && blank(after)
        {
            rest = after.trim_start();
        }
    }
    rest.strip_prefix("fn").is_some_and(blank)
}

/// Where the function `at` stands in ends: the start of the next line that starts one.
fn function_end(code: &str, at: usize) -> usize {
    let mut start = code[at..]
        .find('\n')
        .map_or(code.len(), |newline| at + newline + 1);
    while start < code.len() {
        let end = code[start..]
            .find('\n')
            .map_or(code.len(), |newline| start + newline);
        if starts_a_function(&code[start..end]) {
            return start;
        }
        start = end + 1;
    }
    code.len()
}

/// The name of the last function declared before `at`.
fn enclosing_fn(code: &str, at: usize) -> Option<&str> {
    let mut before = &code[..at];
    while let Some(found) = before.rfind("fn") {
        let after = &code[found + 2..at];
        let bounded = code[..found]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if bounded && after.starts_with([' ', '\t']) {
            let name = after.trim_start();
            let end = name
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(name.len());
            return (end > 0).then_some(&name[..end]);
        }
        before = &code[..found];
    }
    None
}

/// The calls of `call` that make a field: not its declaration, no method or another type's
/// function of the same name (`x.field(`, `Stop::field(`), but `super::field(`.
fn sites(code: &str, call: &str) -> Vec<usize> {
    calls(code, call)
        .into_iter()
        .filter(|at| {
            let before = &code[..*at];
            !before.trim_end().ends_with("fn")
                && !before.ends_with('.')
                && (!before.ends_with("::") || before.ends_with("super::"))
        })
        .collect()
}

/// The marker on the 1-based `line` of `source` or one of the two above, from its name on.
fn marked(source: &str, line: usize) -> Option<&str> {
    let lines: Vec<&str> = source.lines().collect();
    (line.saturating_sub(3)..line)
        .rev()
        .filter_map(|index| lines.get(index).copied())
        .find_map(|text| text.find(MARKER).map(|at| &text[at..]))
}

/// Whether a label call stands on the 1-based `line` of `code` or the [`LOOKBACK`] above it.
fn label_near(code: &str, line: usize) -> bool {
    let lines: Vec<&str> = code.lines().collect();
    let from = line.saturating_sub(LOOKBACK + 1);
    let window = lines[from..line.min(lines.len())].join("\n");
    LABELS.iter().any(|label| !calls(&window, label).is_empty())
}

/// The uses of `name` in `code[start..end]`, as a name of its own.
fn uses(code: &str, name: &str, start: usize, end: usize) -> Vec<usize> {
    calls(&code[..end], name)
        .into_iter()
        .filter(|at| *at >= start)
        .filter(|at| {
            !code[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
        })
        .collect()
}

/// Whether the field at `at` answers to a caption's access key: it stands in a
/// `Stop::field(`, opens one's arguments, or the name it is bound to is one's first.
fn keyed(code: &str, at: usize, end: usize) -> bool {
    let own = statement_from(code, at);
    if own.contains(STOP) {
        return true;
    }
    if code[..at]
        .rfind(STOP)
        .is_some_and(|opened| code[opened + STOP.len()..at].trim().is_empty())
    {
        return true;
    }
    let Some(name) = bound_name(code, at) else {
        return false;
    };
    let after = (at + own.len()).min(end);
    code[after..end].match_indices(STOP).any(|(found, _)| {
        let argument = code[after + found + STOP.len()..end].trim_start();
        argument
            .strip_prefix(name)
            .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// Whether the field at `at` has a label the lint can see.
fn labelled(code: &str, at: usize) -> bool {
    let end = function_end(code, at);
    if keyed(code, at, end) || label_near(code, line_of(code, at)) {
        return true;
    }
    let Some(name) = bound_name(code, at) else {
        return false;
    };
    let after = (at + statement_from(code, at).len()).min(end);
    uses(code, name, after, end)
        .into_iter()
        .any(|used| label_near(code, line_of(code, used)))
}

/// The folder of the file at `path`: where the helpers it declares are called.
fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// `path:line` of every field of `files` (path and source) without a label it can see.
fn unlabelled(files: &[(String, String)]) -> Vec<String> {
    let codes: Vec<String> = files.iter().map(|(_, source)| code_only(source)).collect();
    let mut work = vec![("text_input(".to_owned(), APP.to_owned())];
    let mut seen = Vec::new();
    let mut wrong = Vec::new();
    while let Some((call, scope)) = work.pop() {
        if seen.contains(&(call.clone(), scope.clone())) {
            continue;
        }
        seen.push((call.clone(), scope.clone()));
        for ((path, source), code) in files.iter().zip(&codes) {
            if !path.starts_with(&format!("{scope}/")) {
                continue;
            }
            for at in sites(code, &call) {
                let line = line_of(code, at);
                match marked(source, line) {
                    Some(mark) if mark.starts_with(EACH_CALL) => {
                        if let Some(name) = enclosing_fn(code, at) {
                            work.push((format!("{name}("), folder(path).to_owned()));
                        }
                    }
                    Some(_) => {}
                    None if labelled(code, at) => {}
                    None => wrong.push(format!("{path}:{line} {call}")),
                }
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

/// `path:line` of every field of `source` with a placeholder not drawn by `theme::text_field`.
fn pale(path: &str, source: &str) -> Vec<String> {
    let code = code_only(source);
    sites(&code, "text_input(")
        .into_iter()
        .filter(|at| {
            !code[at + "text_input(".len()..]
                .trim_start()
                .starts_with("\"\"")
        })
        .filter(|at| !chain_has(&code, *at, function_end(&code, *at), &[".style("]))
        .map(|at| format!("{path}:{}", line_of(&code, at)))
        .collect()
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
}

/// The views of the window, path and source, tests left out.
fn views() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut paths = Vec::new();
    files_with(&root.join(APP), &[".rs"], &mut paths);
    assert!(!paths.is_empty(), "no views found");
    paths
        .iter()
        .filter(|path| !is_test_file(path))
        .map(|path| {
            let source = std::fs::read_to_string(path).expect("a readable source");
            (relative(&root, path), source)
        })
        .collect()
}

#[test]
fn every_dialog_field_has_a_visible_label() {
    let files: Vec<(String, String)> = views()
        .into_iter()
        .filter(|(path, _)| !EXEMPT.iter().any(|(exempt, _)| exempt == path))
        .collect();
    let wrong = unlabelled(&files);
    assert!(
        wrong.is_empty(),
        "text fields whose only label is the placeholder - put a caption beside or over each \
         (theme::layout::captioned, captioned_beside), or say where its label is \
         `// {MARKER} <where>`:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn every_placeholder_is_drawn_readable() {
    let wrong: Vec<String> = views()
        .iter()
        .flat_map(|(path, source)| pale(path, source))
        .collect();
    assert!(
        wrong.is_empty(),
        "placeholders in iced's pale grey - add .style(theme::text_field):\n{}",
        wrong.join("\n")
    );
}

fn view(source: &str) -> Vec<(String, String)> {
    vec![(format!("{APP}/x/view.rs"), source.to_owned())]
}

/// The audit's finding (2026-11-25): a field named only by its placeholder.
#[test]
fn a_placeholder_alone_is_no_label() {
    let found = unlabelled(&view(
        "fn prompt() {\n    column![\n        hint(help),\n        text_input(\"Master password\", v),\n    ]\n}\n",
    ));
    assert_eq!(found, [format!("{APP}/x/view.rs:4 text_input(")]);
}

#[test]
fn a_caption_a_stop_or_a_marker_labels_a_field() {
    for source in [
        "fn a() { captioned(t!(\"k\"), text_input(\"\", v)) }",
        "fn b() {\n    row![\n        caption(l, None),\n        text_input(\"\", v),\n    ]\n}",
        "fn c() {\n    let f = text_input(\"\", v);\n    keyed(Stop::field(f, l), l)\n}",
        "fn d() {\n    let f = text_input(\"\", v);\n    x();\n    x();\n    x();\n    x();\n    \
         rows.push(labeled(t!(\"k\"), f))\n}",
        "fn e() {\n    // field-label: the question over it\n    text_input(\"\", v)\n}",
    ] {
        assert!(unlabelled(&view(source)).is_empty(), "{source}");
    }
}

/// A label in the next function does not label a field of this one.
#[test]
fn a_label_counts_in_its_own_function_only() {
    let source = "fn a() {\n    let f = text_input(\"\", v);\n    f\n}\n\nfn b() {\n    \
                  labeled(t!(\"k\"), f)\n}\n";
    assert_eq!(unlabelled(&view(source)).len(), 1);
}

/// A helper's callers name its fields: each call is held to the rule, in the helper's folder.
#[test]
fn the_calls_of_a_field_helper_are_checked() {
    let helper = "fn field(p: &str, v: &str) -> T {\n    // field-label: each call names it\n    \
                  text_input(p, v)\n}\n";
    let files = vec![
        (format!("{APP}/c/view.rs"), helper.to_owned()),
        (
            format!("{APP}/c/view/form.rs"),
            "fn a() {\n    row![\n        hint(h),\n        field(t!(\"k\"), v),\n    ]\n}\n\
             fn b() { captioned(t!(\"k\"), field(\"\", v)) }\n"
                .to_owned(),
        ),
        (
            format!("{APP}/other/view.rs"),
            "fn c() { Stop::field(x, l); s.field(v) }\n".to_owned(),
        ),
    ];
    assert_eq!(
        unlabelled(&files),
        [format!("{APP}/c/view/form.rs:4 field(")]
    );
}

#[test]
fn a_placeholder_needs_the_readable_style() {
    assert_eq!(pale("v.rs", "fn a() { text_input(\"Name\", v) }").len(), 1);
    assert!(pale("v.rs", "fn a() { text_input(\"\", v) }").is_empty());
    let styled =
        "fn a() {\n    let mut f = text_input(p, v);\n    f = f.style(theme::text_field);\n}";
    assert!(pale("v.rs", styled).is_empty());
    assert!(
        pale(
            "v.rs",
            "fn a() { text_input(p, v).style(theme::text_field) }"
        )
        .is_empty()
    );
}

#[test]
fn a_function_starts_where_a_line_declares_one() {
    for line in [
        "fn a()",
        "    pub fn a()",
        "pub(super) fn a()",
        "pub(in crate::app) const fn a()",
    ] {
        assert!(starts_a_function(line), "{line}");
    }
    for line in [
        "    let f = fn_name();",
        "publish fn",
        "// fn a()",
        "    funds",
    ] {
        assert!(!starts_a_function(line), "{line}");
    }
    let code = "fn a() {\n    x\n}\nfn b() {}\n";
    assert_eq!(function_end(code, 9), code.find("fn b").expect("there"));
    assert_eq!(enclosing_fn(code, 9), Some("a"));
}
