//! Guards the rule that a lock a panic poisoned is never taken silently (RC-080-71): outside the
//! tests, every crate takes a poisoned lock through its `recover.rs` (`lock_or_recover`,
//! `read_or_recover`, `write_or_recover`), which logs the lock's name once and clears the poison.
//! `PoisonError::into_inner` - or a closure that takes the guard out of the error - anywhere
//! else is a regression.

use crate::{files_with, relative, scan::code_only, workspace_root};

/// Test-only sources that live under `src/`: fixtures for other crates' tests.
const TEST_ONLY: &[&str] = &["/testkit", "/test_support", "/test_server"];

fn is_exempt(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name == "recover.rs"
        || TEST_ONLY.iter().any(|part| file.contains(part))
}

/// The lines of `source` that take a poisoned lock's guard without the helper.
fn silent_recoveries(source: &str) -> Vec<usize> {
    let code = code_only(source);
    code.lines()
        .enumerate()
        .filter(|(_, line)| {
            let line: String = line.split_whitespace().collect();
            line.contains("PoisonError::into_inner")
                || (line.contains("unwrap_or_else(|") && line.contains(".into_inner())"))
        })
        .map(|(number, _)| number + 1)
        .collect()
}

#[test]
fn no_poisoned_lock_is_taken_silently() {
    let root = workspace_root();
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("read crates") {
        files_with(
            &entry.expect("dir entry").path().join("src"),
            &[".rs"],
            &mut sources,
        );
    }
    assert!(!sources.is_empty(), "expected to scan crates/*/src");
    let mut found = Vec::new();
    for path in &sources {
        let shown = relative(&root, path);
        if is_exempt(&shown) {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("read source");
        for line in silent_recoveries(&source) {
            found.push(format!("{shown}:{line}"));
        }
    }
    assert!(
        found.is_empty(),
        "a poisoned lock taken without crate::recover (lock_or_recover, read_or_recover, \
         write_or_recover):\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn the_silent_shapes_are_found_and_the_helper_is_not() {
    let source = "fn a(m: &Mutex<u8>) {\n\
                  let _ = m.lock().unwrap_or_else(PoisonError::into_inner);\n\
                  let _ = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner);\n\
                  let _ = m.lock().unwrap_or_else(|poisoned| poisoned.into_inner());\n\
                  let _ = m.lock_or_recover(\"a.m\");\n\
                  // PoisonError::into_inner in a comment\n\
                  let _ = \"PoisonError::into_inner\";\n\
                  }\n";
    assert_eq!(silent_recoveries(source), [2, 3, 4]);
}

#[test]
fn tests_fixtures_and_the_helpers_are_exempt() {
    assert!(is_exempt("crates/rc-engine/src/recover.rs"));
    assert!(is_exempt("crates/rc-engine/src/lib_tests.rs"));
    assert!(is_exempt("crates/rc-remote/src/testkit/server.rs"));
    assert!(!is_exempt("crates/rc-engine/src/lib.rs"));
    assert!(is_exempt("crates/rc-engine/src/recover_tests.rs"));
}
