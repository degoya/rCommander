use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn each_kind_has_its_lines_and_encoding() {
    let marked = names(&["a b.txt", "\u{fc}ber.txt"]);
    let lines = lines_of(ListKind::PathsUtf8, "/data/", &marked);
    assert_eq!(lines, ["/data/a b.txt", "/data/\u{fc}ber.txt"]);
    assert_eq!(lines_of(ListKind::NamesUtf16, "/data/", &marked), marked);

    let utf8 = encode(ListKind::PathsUtf8, &lines, encoding_rs::UTF_8).expect("utf-8");
    assert_eq!(
        String::from_utf8(utf8).expect("text"),
        format!("/data/a b.txt{LINE_END}/data/\u{fc}ber.txt{LINE_END}")
    );

    let utf16 = encode(ListKind::NamesUtf16, &names(&["a"]), encoding_rs::UTF_8).expect("utf-16");
    assert_eq!(&utf16[..2], &[0xFF, 0xFE], "the byte order mark");
    assert_eq!(&utf16[2..4], &[b'a', 0]);

    let latin = encoding_of(1252).expect("windows-1252");
    let ansi = encode(ListKind::NamesAnsi, &names(&["\u{fc}"]), latin).expect("fits");
    assert_eq!(ansi[0], 0xFC);
}

/// A name the ANSI code page cannot hold refuses `%L` and `%F` and names `%UL`.
#[test]
fn a_name_outside_the_code_page_refuses_the_ansi_list() {
    let latin = encoding_of(1252).expect("windows-1252");
    let error = encode(
        ListKind::PathsAnsi,
        &names(&["\u{65e5}\u{672c}.txt"]),
        latin,
    )
    .expect_err("not in 1252");
    assert_eq!(error.code(), "user_command_list_encoding");
    assert!(error.to_string().contains("%UL"));
    assert!(encode(ListKind::PathsUtf8, &names(&["\u{65e5}.txt"]), latin).is_ok());
}

#[test]
fn the_code_pages_windows_names_have_an_encoding() {
    for page in [874, 932, 936, 949, 950, 1250, 1251, 1252, 1253, 1258, 65001] {
        assert!(encoding_of(page).is_some(), "{page}");
    }
    assert_eq!(encoding_of(437), None, "the OEM page is no ANSI page");
}

/// The lists are written into a folder of the user's own, and the app's sweep at exit removes
/// it - not the end of the program.
#[tokio::test]
async fn list_files_are_written_and_removed_after_the_program() {
    let written = write_lists(
        &[ListKind::PathsUtf8, ListKind::NamesUtf16],
        "/data/",
        &names(&["one.txt", "two words.txt"]),
    )
    .await
    .expect("written");
    assert_eq!(written.len(), 2);
    let (kind, path) = &written[0];
    assert_eq!(*kind, ListKind::PathsUtf8);
    let text = std::fs::read_to_string(path).expect("readable");
    assert_eq!(
        text,
        format!("/data/one.txt{LINE_END}/data/two words.txt{LINE_END}")
    );
    let folder = Path::new(path).parent().expect("its folder").to_path_buf();
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    assert!(
        name.starts_with(&format!("list-{}-", std::process::id())),
        "{name}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&folder)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "the user's alone");
    }
    // What rCommander does at exit (`unpacked::remove_own`, one process per test under nextest).
    crate::unpacked::remove_own();
    assert!(!folder.exists());
}

/// A program that hands its list to a running instance returns at once; the list is still there
/// after it ended, and only the app's own sweep removes it.
#[tokio::test]
async fn list_files_outlive_a_program_that_returns_at_once() {
    let written = write_lists(&[ListKind::NamesUtf8], "/data/", &names(&["a.txt"]))
        .await
        .expect("written");
    let path = Path::new(&written[0].1).to_path_buf();
    // The program ran and returned: nothing removes the list.
    let quick = if cfg!(windows) { "cmd" } else { "true" };
    let mut command = std::process::Command::new(quick);
    if cfg!(windows) {
        command.args(["/c", "exit"]);
    }
    // console-window: allow (a test starts a program that ends at once; no window opens in CI)
    let status = command.status().expect("the program ran");
    assert!(status.success());
    assert!(path.exists(), "the list outlives the program");
    crate::unpacked::remove_own();
    assert!(!path.exists(), "gone when rCommander ends");
}

#[tokio::test]
async fn no_list_writes_nothing() {
    assert!(
        write_lists(&[], "/data/", &names(&["a"]))
            .await
            .expect("ok")
            .is_empty()
    );
}
