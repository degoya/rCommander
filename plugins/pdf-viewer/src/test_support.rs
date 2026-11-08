//! Sources, sinks and PDF files for the tests: a file in memory that counts its reads and can be
//! told to stop, and small PDF documents assembled with correct cross-reference offsets.

use crate::types::{Failure, FailureKind, Sink, Source};

/// The most one `read-at` or `write-at` moves (`docs/plugins.md`, "Shared interfaces").
pub(crate) const MAX_WINDOW: u32 = 1 << 20;

/// A source over bytes in memory that counts its reads and can be told to stop.
pub(crate) struct TestSource {
    pub(crate) bytes: Vec<u8>,
    pub(crate) reads: u64,
    /// `should-stop` answers true after this many reads.
    pub(crate) stop_after: Option<u64>,
}

impl TestSource {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            reads: 0,
            stop_after: None,
        }
    }
}

impl Source for TestSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn name(&self) -> String {
        "test.pdf".to_owned()
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        assert!(length <= MAX_WINDOW, "read-at of {length} bytes");
        let start = usize::try_from(offset).map_err(|_| Failure::of(FailureKind::BadData))?;
        if start > self.bytes.len() {
            return Err(Failure::of(FailureKind::BadData));
        }
        let end = (start + length as usize).min(self.bytes.len());
        self.reads += 1;
        Ok(self.bytes[start..end].to_vec())
    }

    fn should_stop(&mut self) -> bool {
        self.stop_after.is_some_and(|after| self.reads >= after)
    }
}

/// A sink in memory.
#[derive(Default)]
pub(crate) struct VecSink(pub(crate) Vec<u8>);

impl Sink for VecSink {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        assert!(
            bytes.len() <= MAX_WINDOW as usize,
            "write-at of {} bytes",
            bytes.len()
        );
        let start = offset as usize;
        if self.0.len() < start + bytes.len() {
            self.0.resize(start + bytes.len(), 0);
        }
        self.0[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

/// A PDF of `pages`, each a content stream that shows its lines in Helvetica (a standard font,
/// not embedded), with an information dictionary and, when given, `extra` in the trailer.
pub(crate) fn pdf(pages: &[&[&str]], info: &str, extra: &str) -> Vec<u8> {
    // Objects: 1 catalog, 2 pages, 3 font, 4 info, then a page and its content per page.
    let mut objects: Vec<String> = Vec::new();
    let kids: Vec<String> = (0..pages.len())
        .map(|index| format!("{} 0 R", 5 + 2 * index))
        .collect();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_owned());
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_owned(),
    );
    objects.push(format!("<< {info} >>"));
    for (index, lines) in pages.iter().enumerate() {
        let content_id = 6 + 2 * index;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
        ));
        let mut stream = String::from("BT /F1 12 Tf 72 760 Td 14 TL\n");
        for line in *lines {
            stream.push_str(&format!("({line}) Tj T*\n"));
        }
        stream.push_str("ET\n");
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}endstream",
            stream.len()
        ));
    }
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R {extra} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// The trailer entries of a document encrypted with a user password that is not empty: RC4 of
/// revision 2, whose `U` an empty password does not reproduce.
pub(crate) const ENCRYPTED: &str = "/Encrypt << /Filter /Standard /V 1 /R 2 /Length 40 /P -4 \
     /O <00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF> \
     /U <FFEEDDCCBBAA99887766554433221100FFEEDDCCBBAA99887766554433221100> >> \
     /ID [<0123456789ABCDEF0123456789ABCDEF> <0123456789ABCDEF0123456789ABCDEF>]";
