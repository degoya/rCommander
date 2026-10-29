use super::{Instance, MAX_INSTANCE_BYTES, read, write};

fn instance() -> Instance {
    Instance {
        port: 47_123,
        token: "ab".repeat(32),
        pid: 4_242,
    }
}

#[test]
fn a_written_file_reads_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data").join(super::INSTANCE_FILE);
    write(&path, &instance()).expect("writes");
    assert_eq!(read(&path), Some(instance()));
}

/// A planted or runaway file is not read into memory whole (RC-080-54): past the limit it is no
/// instance, even when it would parse.
#[test]
fn a_file_over_the_limit_is_not_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(super::INSTANCE_FILE);
    let mut text = serde_json::to_string(&instance()).expect("serializes");
    let padding = usize::try_from(MAX_INSTANCE_BYTES).expect("small") - text.len();
    text.insert_str(1, &" ".repeat(padding));
    std::fs::write(&path, &text).expect("writes");
    assert_eq!(
        read(&path),
        Some(instance()),
        "exactly the limit still reads"
    );

    text.insert(1, ' ');
    std::fs::write(&path, &text).expect("writes");
    assert_eq!(read(&path), None);
}
