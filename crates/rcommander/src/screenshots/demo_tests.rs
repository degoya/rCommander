use std::{fs, path::Path};

use rc_db::Db;
use rc_engine::{Command, Engine, Outcome, VfsPath};

use super::{ARCHIVE, CONNECTIONS, FILES, add_connections, claim, tar, write_tree};

#[test]
fn a_missing_or_empty_folder_is_claimed() {
    let temp = tempfile::tempdir().expect("temp dir");
    let missing = temp.path().join("demo").join("root");
    assert_eq!(claim(&missing).expect("made"), missing);
    assert!(missing.is_dir());
    assert_eq!(claim(&missing).expect("empty"), missing);
}

#[test]
fn a_folder_with_anything_in_it_a_file_or_a_relative_path_is_refused() {
    let temp = tempfile::tempdir().expect("temp dir");
    fs::write(temp.path().join("mine.txt"), b"the person's").expect("write");
    let error = claim(temp.path()).expect_err("not empty");
    assert!(error.to_string().contains("not empty"), "{error}");
    assert_eq!(
        fs::read(temp.path().join("mine.txt")).expect("kept"),
        b"the person's"
    );
    let file = temp.path().join("mine.txt");
    let error = claim(&file).expect_err("a file");
    assert!(error.to_string().contains("no folder"), "{error}");
    let error = claim(Path::new("demo")).expect_err("relative");
    assert!(error.to_string().contains("not absolute"), "{error}");
}

#[test]
fn the_tree_holds_every_file_and_the_archive() {
    let temp = tempfile::tempdir().expect("temp dir");
    write_tree(temp.path()).expect("tree");
    for (name, text) in FILES {
        let read = fs::read_to_string(temp.path().join(name)).expect(name);
        assert_eq!(read, *text, "{name}");
    }
    let archive = fs::read(temp.path().join(ARCHIVE)).expect("archive");
    assert_eq!(archive.len() % 512, 0);
    assert!(archive.starts_with(b"website/"));
}

#[test]
fn the_demo_names_nothing_real() {
    for (name, _) in FILES {
        assert!(!name.contains(".."), "{name}");
        assert!(!name.starts_with('/'), "{name}");
    }
    for (_, _, host, _) in CONNECTIONS {
        assert!(
            [".example.com", ".example.net", ".example.org"]
                .iter()
                .any(|domain| host.ends_with(domain)),
            "{host} is no example domain (RFC 2606)"
        );
    }
}

/// Reads a header's octal field.
fn octal(field: &[u8]) -> u64 {
    let text: String = field
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .map(|byte| char::from(*byte))
        .collect();
    u64::from_str_radix(&text, 8).expect("octal")
}

#[test]
fn the_tar_has_valid_headers_and_ends_with_two_empty_blocks() {
    let archive = tar(&[("a/one.txt", "hello"), ("two.txt", "")], 1_000);
    // A header and one data block, a header alone, two empty blocks.
    assert_eq!(archive.len(), 512 * 5);
    let header = &archive[..512];
    assert_eq!(&header[..9], b"a/one.txt");
    assert_eq!(octal(&header[124..136]), 5);
    assert_eq!(octal(&header[136..148]), 1_000);
    assert_eq!(header[156], b'0');
    assert_eq!(&header[257..263], b"ustar\0");
    let mut summed = header.to_vec();
    summed[148..156].fill(b' ');
    let sum: u64 = summed.iter().map(|byte| u64::from(*byte)).sum();
    assert_eq!(octal(&header[148..156]), sum);
    assert_eq!(&archive[512..517], b"hello");
    assert_eq!(&archive[1024..1031], b"two.txt");
    assert!(archive[1536..].iter().all(|byte| *byte == 0));
}

#[tokio::test]
async fn the_demo_connections_are_stored() {
    let temp = tempfile::tempdir().expect("temp dir");
    let engine = Engine::headless(VfsPath::local(temp.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    add_connections(&engine).await.expect("stored");
    let Ok(Outcome::ConnectionTree { tree }) = engine.dispatch(Command::ListConnections).await
    else {
        panic!("a tree");
    };
    let mut names: Vec<&str> = tree
        .connections
        .iter()
        .map(|connection| connection.name.as_str())
        .collect();
    names.sort_unstable();
    let mut wanted: Vec<&str> = CONNECTIONS.iter().map(|(name, ..)| *name).collect();
    wanted.sort_unstable();
    assert_eq!(names, wanted);
}
