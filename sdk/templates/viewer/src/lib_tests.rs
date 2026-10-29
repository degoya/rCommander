//! The viewer against an in-memory file: `cargo test` needs no WebAssembly toolchain.

use super::{TABLE_BYTES, TABLE_ROWS, Viewer, WINDOW, probe, table};
use crate::types::{Failure, FailureKind, Output, Sink, Source};

/// A file in memory, read in windows as the host serves it.
struct File {
    name: &'static str,
    bytes: Vec<u8>,
    reads: usize,
    stop: bool,
}

impl File {
    fn new(name: &'static str, bytes: &[u8]) -> Self {
        Self {
            name,
            bytes: bytes.to_vec(),
            reads: 0,
            stop: false,
        }
    }
}

impl Source for File {
    fn name(&self) -> String {
        self.name.to_owned()
    }

    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        assert!(length <= WINDOW, "a window is at most 1 MiB");
        self.reads += 1;
        let start = usize::try_from(offset)
            .expect("offset")
            .min(self.bytes.len());
        let end = (start + length as usize).min(self.bytes.len());
        Ok(self.bytes[start..end].to_vec())
    }

    fn should_stop(&mut self) -> bool {
        self.stop
    }
}

/// A sink in memory that checks every write is a window.
#[derive(Default)]
struct Out(Vec<u8>);

impl Sink for Out {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        assert!(bytes.len() <= WINDOW as usize);
        assert_eq!(offset, self.0.len() as u64, "written in order");
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

fn shown(name: &'static str, bytes: &[u8]) -> (Output, String) {
    let (mut viewer, document) = Viewer::open(File::new(name, bytes), None).expect("open");
    let mut out = Out::default();
    viewer.text(&mut out).expect("text");
    (
        document.outputs[0],
        String::from_utf8(out.0).expect("UTF-8"),
    )
}

#[test]
fn a_csv_file_is_a_markdown_table() {
    let (output, text) = shown("people.csv", b"name,age\nAda,36\n\"Hopper, Grace\",85\n");
    assert_eq!(output, Output::Markdown);
    assert_eq!(
        text,
        "| name | age |\n| --- | --- |\n| Ada | 36 |\n| Hopper, Grace | 85 |\n"
    );
}

#[test]
fn a_tsv_file_splits_at_tabs_and_semicolons_are_found() {
    let (_, text) = shown("a.tsv", b"x,y\tz\n1,2\t3\n");
    assert!(text.starts_with("| x,y | z |\n"), "{text}");
    let (_, text) = shown("b.csv", b"\xEF\xBB\xBFa;b;c\r\n1;2;3\r\n");
    assert_eq!(text, "| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n");
}

#[test]
fn quotes_line_breaks_and_markdown_characters_stay_in_their_cell() {
    let rows = table::rows(b"a,b\n\"x\"\"y\",\"two\nlines\"\n|*_,\n", b',', true, 10);
    assert_eq!(rows.rows[1], ["x\"y", "two\nlines"]);
    let text = table::markdown(&rows);
    assert!(text.contains("| x\"y | two lines |"), "{text}");
    assert!(text.contains("| \\|\\*\\_ |  |"), "{text}");
}

#[test]
fn a_short_row_is_filled_and_a_long_one_widens_the_table() {
    let rows = table::rows(b"a,b\n1\n1,2,3\n", b',', true, 10);
    let text = table::markdown(&rows);
    assert_eq!(
        text,
        "| a | b |  |\n| --- | --- | --- |\n| 1 |  |  |\n| 1 | 2 | 3 |\n"
    );
}

#[test]
fn one_column_or_no_utf8_is_text() {
    let (output, text) = shown("notes.csv", b"just a line\nand another\n");
    assert_eq!(output, Output::Text);
    assert_eq!(text, "just a line\nand another\n");

    let (mut viewer, document) =
        Viewer::open(File::new("old.csv", b"a,b\n\xE4,\xF6\n"), None).expect("open");
    assert_eq!(document.outputs, [Output::Text]);
    assert_eq!(
        viewer.text(&mut Out::default()).expect("text"),
        "windows-1252"
    );
}

#[test]
fn an_empty_file_opens_as_empty_text() {
    let (output, text) = shown("empty.csv", b"");
    assert_eq!(output, Output::Text);
    assert!(text.is_empty());
}

#[test]
fn a_large_file_is_read_only_as_far_as_the_table_goes() {
    let mut bytes = b"id,value\n".to_vec();
    let mut row = 0_u64;
    while (bytes.len() as u64) < 3 * TABLE_BYTES {
        bytes.extend_from_slice(format!("{row},some value {row}\n").as_bytes());
        row += 1;
    }
    let mut file = File::new("large.csv", &bytes);
    let (viewer, document) = Viewer::open(&mut file, None).expect("open");
    drop(viewer);
    assert_eq!(document.outputs, [Output::Markdown]);
    assert!(
        file.reads <= (TABLE_BYTES / u64::from(WINDOW)) as usize,
        "{} reads",
        file.reads
    );

    let (_, text) = shown("large.csv", &bytes);
    assert_eq!(
        text.lines().filter(|line| line.starts_with('|')).count(),
        TABLE_ROWS + 1
    );
    assert!(text.ends_with(&format!(
        "_The first {TABLE_ROWS} rows of a larger file._\n"
    )));
}

#[test]
fn a_cut_last_row_is_left_out() {
    let rows = table::rows(b"a,b\n1,2\n3,", b',', false, 10);
    assert_eq!(rows.rows.len(), 2);
    assert!(rows.more);
}

#[test]
fn moving_on_cancels_the_text() {
    let (mut viewer, _) = Viewer::open(File::new("x.txt", b"plain"), None).expect("open");
    viewer.src.stop = true;
    let failure = viewer.text(&mut Out::default()).expect_err("cancelled");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

#[test]
fn probe_wants_utf8_with_a_separator() {
    assert!(probe(&mut File::new("a.csv", b"a,b\n1,2\n")));
    assert!(probe(&mut File::new("a.tsv", b"a\tb\n")));
    assert!(!probe(&mut File::new("a.csv", b"no separator here\n")));
    assert!(!probe(&mut File::new("a.csv", b"\xFF\xFE,\n")));
}

impl<S: Source + ?Sized> Source for &mut S {
    fn name(&self) -> String {
        (**self).name()
    }

    fn size(&self) -> u64 {
        (**self).size()
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        (**self).read_at(offset, length)
    }

    fn should_stop(&mut self) -> bool {
        (**self).should_stop()
    }
}

#[test]
fn the_separator_setting_overrides_the_files_own() {
    assert_eq!(table::chosen_separator(Some("semicolon")), Some(b';'));
    assert_eq!(table::chosen_separator(Some("auto")), None);
    assert_eq!(table::chosen_separator(None), None);
    // The first line holds more commas, the person chose semicolons.
    let bytes = b"a;b,c,d\n1;2,3,4\n";
    let (mut viewer, document) =
        Viewer::open(File::new("data.csv", bytes), Some(b';')).expect("open");
    assert_eq!(document.outputs, vec![Output::Markdown]);
    let mut out = Out::default();
    viewer.text(&mut out).expect("text");
    assert_eq!(
        String::from_utf8(out.0).expect("UTF-8"),
        "| a | b,c,d |\n| --- | --- |\n| 1 | 2,3,4 |\n"
    );
}
