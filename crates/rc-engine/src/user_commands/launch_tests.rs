use std::ffi::OsString;

use super::*;

fn values(list: &[&str]) -> Vec<String> {
    list.iter().map(|value| (*value).to_owned()).collect()
}

/// A batch file gets no value cmd would read as its own syntax (BatBadBut); any other program
/// does, and so does a batch file whose values are plain.
#[test]
fn batch_files_refuse_unsafe_values() {
    for unsafe_value in [
        "a&calc.exe",
        "50%",
        "x^y",
        "a|b",
        "a<b",
        "a>b",
        "say \"hi\"",
    ] {
        let list = values(&["plain.txt", unsafe_value]);
        assert!(!batch_values_are_safe("build.bat", &list), "{unsafe_value}");
        assert!(
            !batch_values_are_safe("C:\\Tools\\RUN.CMD", &list),
            "{unsafe_value}"
        );
        assert!(
            batch_values_are_safe("notepad.exe", &list),
            "{unsafe_value}"
        );
    }
    assert!(batch_values_are_safe(
        "\"build.bat\"",
        &values(&["C:\\Data\\a b.txt", "x-y_z (1).txt"])
    ));
}

#[test]
fn a_program_is_found_on_the_search_path_only() {
    let root = tempfile::tempdir().expect("temp dir");
    let bin = root.path().join("bin");
    let panel = root.path().join("panel");
    std::fs::create_dir_all(&bin).expect("bin");
    std::fs::create_dir_all(&panel).expect("panel");
    std::fs::write(bin.join("tool"), b"").expect("tool");
    std::fs::write(bin.join("edit.EXE"), b"").expect("edit");
    // A program of the same name in the panel's folder is never taken.
    std::fs::write(panel.join("planted"), b"").expect("planted");
    let search: OsString = std::env::join_paths([&bin]).expect("joined");

    assert_eq!(
        find_program("tool", Some(&search), &[]),
        Some(bin.join("tool"))
    );
    assert_eq!(
        find_program("edit", Some(&search), &[".EXE".to_owned()]),
        Some(bin.join("edit.EXE"))
    );
    assert_eq!(find_program("planted", Some(&search), &[]), None);
    assert_eq!(find_program("tool", None, &[]), None);
    assert_eq!(find_program("", Some(&search), &[]), None);
    // A path with a folder is taken as written, when it is a file.
    let written = bin.join("tool");
    assert_eq!(
        find_program(&written.to_string_lossy(), Some(&search), &[]),
        Some(written)
    );
    // A relative entry of the search path is no folder of the user's choosing.
    let relative: OsString = std::env::join_paths(["bin"]).expect("joined");
    assert_eq!(find_program("tool", Some(&relative), &[]), None);
}

/// Outside Windows a start as administrator starts nothing at all.
#[cfg(not(windows))]
#[tokio::test]
async fn an_elevated_start_outside_windows_is_refused() {
    use crate::recover::LockOrRecover as _;
    STARTED.lock_or_recover("test").clear();
    let error = start(Launch {
        program: "ls".into(),
        args: Vec::new(),
        folder: None,
        show: Show::Normal,
        elevated: true,
        values: Vec::new(),
    })
    .await
    .expect_err("refused");
    assert_eq!(error.code(), "run_as_admin_unsupported");
    assert!(
        STARTED.lock_or_recover("test").is_empty(),
        "nothing started"
    );
}

/// Windows ignores trailing dots and blanks and the case of an extension: `FOO.BAT.` and
/// `x.CmD` are batch files.
#[test]
fn a_batch_file_is_known_by_its_normalised_extension() {
    for name in [
        "FOO.BAT.",
        "x.CmD",
        "run.cmd. . ",
        "\"C:\\Tools\\build.bat\"",
        "C:\\a b\\RUN.BAT  ",
    ] {
        assert!(is_batch_file(name), "{name}");
    }
    for name in ["x.bat.exe", "batch", "x.bash", "cmd", "notes.cmdx", ""] {
        assert!(!is_batch_file(name), "{name}");
    }
    let unsafe_values = values(&["a&b"]);
    assert!(!batch_values_are_safe("FOO.BAT.", &unsafe_values));
    assert!(!batch_values_are_safe("x.CmD", &unsafe_values));
}

/// A bare name the search path turns into a batch file is checked as that batch file: `foo` found
/// as `foo.bat` (or `FOO.CMD`) gets no value with cmd's syntax, and starts by the path found.
#[test]
fn a_name_resolved_to_a_batch_file_is_checked_as_one() {
    let root = tempfile::tempdir().expect("temp dir");
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    std::fs::write(bin.join("foo.bat"), b"").expect("foo.bat");
    std::fs::write(bin.join("BAR.CMD"), b"").expect("BAR.CMD");
    std::fs::write(bin.join("tool.EXE"), b"").expect("tool.EXE");
    let search: OsString = std::env::join_paths([&bin]).expect("joined");
    let extensions = [".EXE".to_owned(), ".bat".to_owned(), ".CMD".to_owned()];
    let syntax = values(&["report.txt", "a&calc.exe"]);
    let plain = values(&["C:\\Data\\a b.txt"]);

    for name in ["foo", "BAR"] {
        let error = resolve_checked(name, Some(&search), &extensions, &syntax)
            .expect_err("a batch file with cmd syntax");
        assert_eq!(error.code(), "user_command_unsafe_argument", "{name}");
    }
    assert_eq!(
        resolve_checked("foo", Some(&search), &extensions, &plain).expect("plain values"),
        bin.join("foo.bat"),
        "it starts by the path found"
    );
    assert_eq!(
        resolve_checked("tool", Some(&search), &extensions, &syntax).expect("no batch file"),
        bin.join("tool.EXE")
    );
    // The name as written counts too, found or not.
    let written = resolve_checked("x.CmD", Some(&search), &extensions, &syntax)
        .expect_err("refused before the search");
    assert_eq!(written.code(), "user_command_unsafe_argument");
    let missing =
        resolve_checked("absent", Some(&search), &extensions, &plain).expect_err("not on the path");
    assert_eq!(missing.code(), "user_command_not_found");
}
