use std::path::PathBuf;

use super::{
    EverythingError, LIST2_HEADER, MAX_PATH_UNITS, REQUEST_FULL_PATH_AND_NAME, REQUEST_NAME,
    answers, expression, parse_reply, request,
};
use crate::SearchQuery;

/// An `EVERYTHING_IPC_LIST2` as Everything sends it: the header, one item per path (folders
/// flagged), then each path's length, its UTF-16 units and a NUL.
pub(crate) fn reply(paths: &[&str]) -> Vec<u8> {
    reply_with(paths, REQUEST_FULL_PATH_AND_NAME)
}

fn reply_with(paths: &[&str], flags: u32) -> Vec<u8> {
    let count = u32::try_from(paths.len()).expect("a few paths");
    let mut bytes = Vec::new();
    for value in [count, count, 0, flags, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut data = Vec::new();
    let data_start = LIST2_HEADER + paths.len() * 8;
    for path in paths {
        let offset = u32::try_from(data_start + data.len()).expect("small");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
        let mut texts = vec![*path];
        if flags & REQUEST_NAME != 0 {
            texts.insert(0, path.rsplit(['\\', '/']).next().unwrap_or(path));
        }
        for text in texts {
            let units: Vec<u16> = text.encode_utf16().collect();
            data.extend_from_slice(&u32::try_from(units.len()).expect("short").to_le_bytes());
            for unit in units.iter().chain(&[0]) {
                data.extend_from_slice(&unit.to_le_bytes());
            }
        }
    }
    bytes.extend_from_slice(&data);
    bytes
}

fn masks(name: &str) -> SearchQuery {
    SearchQuery {
        name: name.to_owned(),
        ..SearchQuery::default()
    }
}

fn roots(paths: &[&str]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

const SEP: char = std::path::MAIN_SEPARATOR;

#[test]
fn masks_become_alternatives_below_the_start_folders() {
    let one = roots(&["/work"]);
    assert_eq!(
        expression(&masks("*.rs;*.toml"), &one).as_deref(),
        Some(format!("\"/work{SEP}\" <\"*.rs\"|\"*.toml\">").as_str())
    );
    assert_eq!(
        expression(&masks("report"), &one).as_deref(),
        Some(format!("\"/work{SEP}\" \"report\"").as_str()),
        "a mask without a wildcard is a part of the name, in both"
    );
    let two = roots(&["/a/", "/b c"]);
    assert_eq!(
        expression(&masks("x.*"), &two).as_deref(),
        Some(format!("<\"/a{SEP}\"|\"/b c{SEP}\"> \"x*\"").as_str()),
        "`x.*` also takes `x`"
    );
}

#[test]
fn every_name_asks_for_the_folders_only() {
    let one = roots(&["/work"]);
    for name in ["", "*", "*.*", "*.rs;*"] {
        assert_eq!(
            expression(&masks(name), &one).as_deref(),
            Some(format!("\"/work{SEP}\"").as_str()),
            "{name:?}"
        );
    }
}

#[test]
fn exclusions_and_folder_masks_are_left_to_the_walk() {
    let one = roots(&["/work"]);
    assert_eq!(
        expression(&masks("*.txt | *.bak node_modules\\"), &one).as_deref(),
        Some(format!("\"/work{SEP}\" \"*.txt\"").as_str())
    );
    assert_eq!(
        expression(&masks("docs\\ \"my file\""), &one).as_deref(),
        Some(format!("\"/work{SEP}\" <\"docs\"|\"my file\">").as_str()),
        "a folder mask asks for the name; quotes keep spaces"
    );
}

#[test]
fn unicode_names_and_control_characters() {
    let one = roots(&["/w"]);
    assert_eq!(
        expression(&masks("Gr\u{fc}\u{df}e*.txt"), &one).as_deref(),
        Some(format!("\"/w{SEP}\" \"Gr\u{fc}\u{df}e*.txt\"").as_str())
    );
    assert_eq!(
        expression(&masks("a\u{7}b"), &one).as_deref(),
        Some(format!("\"/w{SEP}\" \"ab\"").as_str()),
        "control characters are dropped"
    );
}

#[test]
fn what_everything_cannot_take_is_not_asked() {
    let one = roots(&["/w"]);
    let regex = SearchQuery {
        name_regex: true,
        ..masks("^a")
    };
    assert_eq!(expression(&regex, &one), None);
    assert_eq!(expression(&masks("*.rs"), &[]), None);
    assert_eq!(expression(&masks("\u{7}"), &one), None);
}

#[test]
fn a_request_is_the_query2_header_and_the_utf16_search() {
    let bytes = request(0x1234, 7, "a\u{fc}", 50);
    let dwords: Vec<u32> = bytes[..28]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|four| u32::from_le_bytes(*four))
        .collect();
    assert_eq!(dwords, [0x1234, 7, 0, 0, 50, REQUEST_FULL_PATH_AND_NAME, 1]);
    assert_eq!(&bytes[28..], &[b'a', 0, 0xfc, 0, 0, 0]);
}

#[test]
fn an_answer_reads_as_its_paths() {
    let paths = [
        "C:\\work\\a.txt",
        "C:\\work\\Gr\u{fc}\u{df}e\\\u{1f600}.txt",
    ];
    assert_eq!(
        parse_reply(&reply(&paths), 10).expect("reads"),
        paths.map(PathBuf::from)
    );
    assert_eq!(
        parse_reply(
            &reply_with(&paths, REQUEST_FULL_PATH_AND_NAME | REQUEST_NAME),
            10
        )
        .expect("reads"),
        paths.map(PathBuf::from),
        "a name sent before the path is skipped"
    );
    assert_eq!(
        parse_reply(&reply(&[]), 10).expect("reads"),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_broken_answer_is_an_error_never_a_panic() {
    let whole = reply(&["C:\\a.txt", "C:\\b.txt"]);
    for cut in 0..whole.len() {
        assert!(
            parse_reply(&whole[..cut], 10).is_err(),
            "cut at {cut} of {}",
            whole.len()
        );
    }
    assert_eq!(
        parse_reply(&whole, 1),
        Err(EverythingError::Reply("too many items"))
    );
    assert_eq!(
        parse_reply(&reply_with(&["C:\\a"], REQUEST_NAME), 10),
        Err(EverythingError::Reply("no full paths"))
    );

    let mut outside = whole.clone();
    outside[LIST2_HEADER + 4..LIST2_HEADER + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_reply(&outside, 10).is_err());

    let mut many = whole.clone();
    many[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        parse_reply(&many, usize::MAX).is_err(),
        "items beyond the answer"
    );

    let mut long = whole.clone();
    let first = LIST2_HEADER + 2 * 8;
    let too_long = u32::try_from(MAX_PATH_UNITS + 1).expect("small");
    long[first..first + 4].copy_from_slice(&too_long.to_le_bytes());
    assert_eq!(
        parse_reply(&long, 10),
        Err(EverythingError::Reply("text too long"))
    );

    let mut unended = whole;
    let nul = first + 4 + "C:\\a.txt".len() * 2;
    unended[nul] = b'x';
    assert_eq!(
        parse_reply(&unended, 10),
        Err(EverythingError::Reply("text without its end"))
    );
}

#[test]
fn only_everything_1_4_and_later_answers() {
    assert!(!answers((1, 3)));
    assert!(answers((1, 4)));
    assert!(answers((1, 5)));
    assert!(!answers((0, 9)));
}
