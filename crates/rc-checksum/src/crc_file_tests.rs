use std::path::Path;

use super::*;

#[test]
fn the_written_file_reads_back() {
    let text = write("disk.iso", 5_368_709_120, "1a2b3c4d");
    assert_eq!(
        text,
        "filename=disk.iso\r\nsize=5368709120\r\ncrc32=1A2B3C4D\r\n"
    );
    assert_eq!(
        parse(Path::new("disk.crc"), &text).expect("parsed"),
        CrcInfo {
            file_name: "disk.iso".to_owned(),
            size: Some(5_368_709_120),
            crc32: "1a2b3c4d".to_owned(),
        }
    );
}

#[test]
fn keys_ignore_case_and_unknown_keys_are_skipped() {
    let info = parse(
        Path::new("a.crc"),
        "\u{feff}FileName=a b.zip\nversion=2\nCRC32=abc\n",
    )
    .expect("parsed");
    assert_eq!(info.file_name, "a b.zip");
    assert_eq!(info.size, None);
    assert_eq!(info.crc32, "00000abc");
}

#[test]
fn a_malformed_or_hostile_crc_file_is_refused() {
    let cases = [
        ("filename=../../evil.exe\ncrc32=00000000\n", "file name"),
        ("filename=C:\\evil.exe\ncrc32=00000000\n", "file name"),
        ("filename=\ncrc32=00000000\n", "file name"),
        // RC-080-43: a device, an NTFS stream, a name Windows strips, a turned direction.
        ("filename=CON\ncrc32=00000000\n", "file name"),
        ("filename=nul.iso\ncrc32=00000000\n", "file name"),
        ("filename=disk.iso:hidden\ncrc32=00000000\n", "file name"),
        ("filename=disk.iso.\ncrc32=00000000\n", "file name"),
        ("filename=a<b>.iso\ncrc32=00000000\n", "file name"),
        ("filename=fdp\u{202e}exe.iso\ncrc32=00000000\n", "file name"),
        ("filename=a\nsize=-1\ncrc32=00000000\n", "size"),
        ("filename=a\ncrc32=123456789\n", "crc32"),
        ("filename=a\ncrc32=xyz\n", "crc32"),
        ("crc32=00000000\n", "no filename line"),
        ("filename=a\n", "no crc32 line"),
    ];
    for (text, reason) in cases {
        match parse(Path::new("x.crc"), text) {
            Err(ChecksumError::InvalidCrcFile { reason: got, .. }) => {
                assert_eq!(got, reason, "{text:?}");
            }
            other => panic!("{text:?}: {other:?}"),
        }
    }
}
