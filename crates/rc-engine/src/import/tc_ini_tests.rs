use super::{IniError, MAX_FILE_BYTES, MAX_VALUE_CHARS, decode, parse, read_file};

fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[test]
fn sections_keys_and_comments() {
    let ini = parse(
        "stray=before any section\n; a comment\n[Left]\npath=C:\\Work\\\n  Path = D:\\ \nno equals sign\n[lefttabs]\n0_path=E:\\\n",
    );
    assert_eq!(ini.get("left", "PATH"), Some("C:\\Work\\"));
    assert_eq!(ini.get("LEFTTABS", "0_path"), Some("E:\\"));
    assert!(ini.section("stray").is_none());
    let keys: Vec<(&str, &str)> = ini.section("left").expect("left").entries().collect();
    assert_eq!(keys, vec![("path", "C:\\Work\\"), ("Path", "D:\\")]);
}

#[test]
fn a_section_that_comes_again_adds_to_the_first() {
    let ini = parse("[a]\nx=1\n[b]\ny=2\n[A]\nx=3\nz=4\n");
    assert_eq!(ini.get("a", "x"), Some("1"));
    assert_eq!(ini.get("a", "z"), Some("4"));
}

#[test]
fn a_value_too_long_is_dropped_and_counted() {
    let long = "x".repeat(MAX_VALUE_CHARS + 1);
    let ini = parse(&format!("[s]\nlong={long}\nshort=ok\n"));
    assert_eq!(ini.get("s", "long"), None);
    assert_eq!(ini.get("s", "short"), Some("ok"));
    assert_eq!(ini.dropped, 1);
}

#[test]
fn utf16_utf8_and_ansi_are_told_apart() {
    let text = "[s]\nname=Gr\u{fc}\u{df}e \u{20ac}\n";
    assert_eq!(decode(&utf16(text)).as_str(), text);
    assert_eq!(decode(text.as_bytes()).as_str(), text);
    assert_eq!(decode(b"\xEF\xBB\xBFk=v").as_str(), "k=v");
    // Windows-1252: 0xFC is u-umlaut, 0x80 the euro sign.
    assert_eq!(decode(b"k=Gr\xFC\x80").as_str(), "k=Gr\u{fc}\u{20ac}");
}

#[test]
fn a_file_over_the_limit_or_no_file_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let big = dir.path().join("wincmd.ini");
    let size = usize::try_from(MAX_FILE_BYTES).expect("fits") + 1;
    std::fs::write(&big, vec![b';'; size]).expect("write");
    assert!(matches!(read_file(&big), Err(IniError::TooLarge)));
    assert!(matches!(read_file(dir.path()), Err(IniError::NotAFile)));
    assert!(matches!(
        read_file(&dir.path().join("missing.ini")),
        Err(IniError::Io(_))
    ));
    let small = dir.path().join("wcx_ftp.ini");
    std::fs::write(&small, utf16("[connections]\n1=a\n")).expect("write");
    let ini = read_file(&small).expect("reads");
    assert_eq!(ini.get("connections", "1"), Some("a"));
}
