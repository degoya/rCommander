use std::path::{Path, PathBuf};

use super::ALL_CODES;

/// The non-test sources under `dir`, recursively.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src is readable") {
        let path = entry.expect("src entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && !path.to_string_lossy().ends_with("_tests.rs")
        {
            found.push(path);
        }
    }
}

/// The string literal at the start of `text`, after any whitespace.
fn literal(text: &str) -> Option<&str> {
    text.trim_start().strip_prefix('"')?.split('"').next()
}

/// Every code a source hands out - `code: "..."` in a refusal, the first argument of
/// `error_result("...", ...)` - is in `ALL_CODES`, and every listed code is used.
#[test]
fn all_codes_matches_the_sources() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut used = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("source is UTF-8");
        for marker in ["code: ", "error_result("] {
            for (at, _) in text.match_indices(marker) {
                if let Some(code) = literal(&text[at + marker.len()..]) {
                    used.push((code.to_owned(), path.display().to_string()));
                }
            }
        }
    }
    let unlisted: Vec<_> = used
        .iter()
        .filter(|(code, _)| !ALL_CODES.contains(&code.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "codes missing in ALL_CODES: {unlisted:?}"
    );
    let unused: Vec<_> = ALL_CODES
        .iter()
        .filter(|code| !used.iter().any(|(used, _)| used == *code))
        .collect();
    assert!(
        unused.is_empty(),
        "codes in ALL_CODES no source uses: {unused:?}"
    );
}
