use super::{Document, Output, Purpose, Theme, Token, TokenClass, Viewer, ViewerHost};
use crate::test_support::{MAX_IO_BYTES, MemoryHost};
use crate::{Failure, FailureKind, Sink, Source};

/// A viewer that shows its source upper-cased, the shape a template's logic takes.
struct Shouting {
    source: Option<Source>,
}

impl Viewer for Shouting {
    fn probe(&mut self, host: &mut dyn ViewerHost, src: Source) -> bool {
        host.info(src).name.ends_with(".txt")
    }

    fn open(
        &mut self,
        host: &mut dyn ViewerHost,
        src: Source,
        _purpose: Purpose,
        _theme: &Theme,
    ) -> Result<Document, Failure> {
        host.log("info", "opened");
        self.source = Some(src);
        Ok(Document {
            outputs: vec![Output::Highlighted, Output::Text],
            title: Some(host.info(src).name),
            pages: None,
        })
    }

    fn text(&mut self, host: &mut dyn ViewerHost, out: Sink) -> Result<String, Failure> {
        let src = self
            .source
            .ok_or_else(|| Failure::new(FailureKind::Other))?;
        let bytes = host.read_at(src, 0, u32::MAX)?;
        host.write_at(out, 0, &bytes.to_ascii_uppercase())?;
        Ok("utf-8".to_owned())
    }

    fn tokens(
        &mut self,
        _host: &mut dyn ViewerHost,
        _first_line: u32,
        count: u32,
    ) -> Result<Vec<Vec<Token>>, Failure> {
        Ok((0..count)
            .map(|_| {
                vec![Token {
                    start: 0,
                    len: 1,
                    class: TokenClass::Keyword,
                }]
            })
            .collect())
    }
}

fn theme() -> Theme {
    Theme {
        dark: true,
        background: "#000000".to_owned(),
        foreground: "#ffffff".to_owned(),
        accent: "#0078d4".to_owned(),
    }
}

#[test]
fn a_viewer_runs_against_the_memory_host() {
    let mut host = MemoryHost::new();
    host.add_source(1, "notes.txt", b"hello".to_vec());
    host.add_sink(2);
    let mut viewer = Shouting { source: None };

    assert!(viewer.probe(&mut host, 1));
    let document = viewer
        .open(&mut host, 1, Purpose::QuickView, &theme())
        .expect("open");
    assert_eq!(document.outputs, [Output::Highlighted, Output::Text]);
    assert_eq!(document.title.as_deref(), Some("notes.txt"));
    assert_eq!(viewer.text(&mut host, 2).expect("text"), "utf-8");
    assert_eq!(host.sink(2), Some(&b"HELLO"[..]));
    assert_eq!(viewer.tokens(&mut host, 0, 3).expect("tokens").len(), 3);
    assert_eq!(host.logs, [("info".to_owned(), "opened".to_owned())]);
}

#[test]
fn an_output_a_viewer_does_not_offer_is_not_supported() {
    let mut host = MemoryHost::new();
    let mut viewer = Shouting { source: None };
    for result in [
        viewer.image(&mut host, 2).map(|_| ()),
        viewer.page(&mut host, 0, 10, 10, 2),
        viewer.children(&mut host, None).map(|_| ()),
        viewer.search(&mut host, "x", 0, false).map(|_| ()),
        viewer.thumbnail(&mut host, 1, 64, 2).map(|_| ()),
    ] {
        assert_eq!(
            result.expect_err("not offered").kind,
            FailureKind::NotSupported
        );
    }
}

#[test]
fn the_memory_host_keeps_the_hosts_limits() {
    let mut host = MemoryHost::new();
    host.add_source(1, "big.bin", vec![7; MAX_IO_BYTES + 10]);
    host.add_sink(2);
    assert_eq!(
        host.read_at(1, 0, u32::MAX).expect("read").len(),
        MAX_IO_BYTES
    );
    assert_eq!(host.read_at(1, u64::MAX, 10).expect("past the end"), b"");
    assert!(host.read_at(9, 0, 1).is_err(), "a handle it was not given");
    assert!(host.write_at(2, 0, &vec![0; MAX_IO_BYTES + 1]).is_err());
    assert!(host.write_at(3, 0, b"x").is_err());
    host.write_at(2, 4, b"ab").expect("write past the end");
    assert_eq!(host.sink(2), Some(&b"\0\0\0\0ab"[..]));
}

#[test]
fn outputs_are_named_as_the_manifest_and_the_contract_name_them() {
    let names: Vec<_> = Output::ALL.iter().map(|output| output.as_str()).collect();
    assert_eq!(
        names,
        ["text", "highlighted", "markdown", "image", "tree", "pages"]
    );
    for output in Output::ALL {
        let parsed: Output =
            serde_json::from_str(&format!("\"{}\"", output.as_str())).expect("parses");
        assert_eq!(parsed, output);
    }
    assert!(Output::Markdown.is_textual() && !Output::Tree.is_textual());
}

#[test]
fn a_viewer_reads_its_settings_from_the_host() {
    let mut host = MemoryHost::new();
    host.settings
        .insert("separator".to_owned(), "semicolon".to_owned());
    assert_eq!(host.setting("separator").as_deref(), Some("semicolon"));
    assert_eq!(host.setting("undeclared"), None);
}

/// A host of contract 0.1.0 implements no `setting`: every name is undeclared.
#[test]
fn a_host_without_settings_answers_none() {
    struct Bare;
    impl ViewerHost for Bare {
        fn log(&mut self, _level: &str, _message: &str) {}
        fn info(&mut self, _src: Source) -> crate::SourceInfo {
            crate::SourceInfo {
                name: String::new(),
                size: 0,
                modified: None,
            }
        }
        fn read_at(
            &mut self,
            _src: Source,
            _offset: u64,
            _length: u32,
        ) -> Result<Vec<u8>, Failure> {
            Ok(Vec::new())
        }
        fn write_at(&mut self, _out: Sink, _offset: u64, _bytes: &[u8]) -> Result<(), Failure> {
            Ok(())
        }
        fn should_stop(&mut self) -> bool {
            false
        }
    }
    assert_eq!(Bare.setting("format"), None);
}
