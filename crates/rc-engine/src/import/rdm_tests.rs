use std::{fmt::Write as _, fs, path::Path, time::Instant};

use super::{
    MAX_ENTRIES, MAX_FILE_BYTES, RdmError, RdmPassword, RdmSkip, key_path_allowed, parse, read_file,
};
use crate::ConnectionKind;

/// The fixture's `SafePassword` and the one of its password history.
const CANARY: &str = "cmRtLWNhbmFyeS03ZjNhOQ==";
const HISTORY_CANARY: &str = "cmRtLWhpc3RvcnktYzRlMQ==";

fn fixture() -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rdm/sample.rdm"))
        .expect("fixture")
}

/// An export around `connections`.
fn export(connections: &str) -> String {
    format!("<RDMExport><Connections>{connections}</Connections><Version>2</Version></RDMExport>")
}

fn ssh(id: &str, name: &str) -> String {
    format!(
        "<Connection><ConnectionType>SSHShell</ConnectionType><ID>{id}</ID><Name>{name}</Name>\
         <Terminal><Host>h.example.test</Host></Terminal></Connection>"
    )
}

#[test]
fn the_sample_gives_its_ssh_entries_and_the_others_with_their_reason() {
    let found = parse(&fixture()).expect("parses");
    assert!(!found.truncated);
    let summary: Vec<(&str, Result<ConnectionKind, RdmSkip>)> = found
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("build box", Ok(ConnectionKind::Ssh)),
            ("nested box", Ok(ConnectionKind::Ssh)),
            ("office box", Ok(ConnectionKind::Ssh)),
            ("file server", Err(RdmSkip::NotSupportedYet)),
            ("windows desktop", Err(RdmSkip::NotSupported)),
            ("no id", Err(RdmSkip::NoId)),
        ]
    );
    let build = &found.entries[0];
    assert_eq!(
        build.id.as_deref(),
        Some("0f3c1a52-7d4e-4b8a-9c61-2e5b8d7a4f10")
    );
    assert_eq!(build.folders, vec!["Lab"]);
    assert_eq!(build.rdm_type, "SSHShell");
    assert_eq!(build.host, "build.example.test");
    assert_eq!(build.port, Some(2222));
    assert_eq!(build.user.as_deref(), Some("builder"));
    assert_eq!(build.password, RdmPassword::Obfuscated);
    let nested = &found.entries[1];
    assert_eq!(nested.folders, vec!["Lab", "Servers"]);
    assert_eq!(
        nested.id.as_deref(),
        Some("{a1b2c3d4-e5f6-4711-8899-aabbccddeeff}")
    );
    assert_eq!(nested.password, RdmPassword::None);
    assert_eq!(found.entries[3].rdm_type, "Ftp");
    assert_eq!(found.entries[4].rdm_type, "RDPConfigured");
}

/// Beside `SSHShell` (the owner's export), PuTTY's SSH, SFTP and SCP become SSH too, a PuTTY
/// session of another protocol does not; FTPS waits like FTP. Their element names are RDM's
/// forum examples, unverified.
#[test]
fn other_ssh_types_are_ssh_and_ftps_waits_like_ftp() {
    let entry = |kind: &str, id: &str, inner: &str| {
        format!(
            "<Connection><ConnectionType>{kind}</ConnectionType><ID>{id}</ID><Name>{id}</Name>\
             <{kind}>{inner}</{kind}></Connection>"
        )
    };
    let text = export(&[
        entry("Putty", "01", "<Host>p.example.test</Host><LoginName>pu</LoginName><Port>22</Port><ProtocolType>SSH</ProtocolType>"),
        entry("Putty", "02", "<Host>t.example.test</Host><ProtocolType>Telnet</ProtocolType>"),
        entry("Sftp", "03", "<Host>s.example.test</Host>"),
        entry("Scp", "04", "<Host>c.example.test</Host>"),
        entry("Ftps", "05", "<Host>f.example.test</Host>"),
    ]
    .concat());
    let found = parse(text.as_bytes()).expect("parses");
    let kinds: Vec<Result<ConnectionKind, RdmSkip>> =
        found.entries.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        vec![
            Ok(ConnectionKind::Ssh),
            Err(RdmSkip::NotSupported),
            Ok(ConnectionKind::Ssh),
            Ok(ConnectionKind::Ssh),
            Err(RdmSkip::NotSupportedYet),
        ]
    );
    let putty = &found.entries[0];
    assert_eq!(
        (putty.host.as_str(), putty.port, putty.user.as_deref()),
        ("p.example.test", Some(22), Some("pu"))
    );
}

/// A key path (`PrivateKeyPath`, unverified) is taken when local, left out and marked when not.
#[test]
fn a_local_key_path_is_taken_and_a_share_is_left_out() {
    let with_key = |id: &str, key: &str| {
        ssh(id, id).replace(
            "</Terminal>",
            &format!("<PrivateKeyPath>{key}</PrivateKeyPath></Terminal>"),
        )
    };
    let text = export(&format!(
        "{}{}",
        with_key("0a", r"C:\Users\someone\.ssh\id_ed25519"),
        with_key("0b", r"\\fileserver\keys\id_rsa")
    ));
    let found = parse(text.as_bytes()).expect("parses");
    assert_eq!(
        found.entries[0].key_path.as_deref(),
        Some(r"C:\Users\someone\.ssh\id_ed25519")
    );
    assert!(!found.entries[0].key_path_refused);
    assert_eq!(found.entries[1].key_path, None);
    assert!(found.entries[1].key_path_refused);
}

#[test]
fn no_password_or_its_history_is_kept() {
    let found = parse(&fixture()).expect("parses");
    let shown = format!("{found:?}");
    assert!(!shown.contains(CANARY), "{shown}");
    assert!(!shown.contains(HISTORY_CANARY), "{shown}");
}

#[test]
fn a_protected_export_is_refused_as_encrypted() {
    let cases: Vec<Vec<u8>> = vec![
        // The whole content encrypted: no XML at all.
        b"q83vEjRWeJAbCd+/AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=".to_vec(),
        vec![0x8f, 0x00, 0xff, 0x13, 0x37, 0xc3, 0x28],
        // An export that holds something, but no entries.
        b"<RDMExport><Data>q83vEjRWeJA=</Data><Version>2</Version></RDMExport>".to_vec(),
        // An encryption flag at the top.
        export(&ssh("0a", "x"))
            .replace("<Version>", "<IsEncrypted>true</IsEncrypted><Version>")
            .into_bytes(),
    ];
    for bytes in cases {
        assert!(
            matches!(parse(&bytes), Err(RdmError::Encrypted)),
            "{:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn broken_xml_and_other_documents_are_refused() {
    let broken = "<RDMExport>\n<Connections>\n<Connection></Connections>";
    assert!(matches!(
        parse(broken.as_bytes()),
        Err(RdmError::Invalid { line: 3 })
    ));
    assert!(matches!(
        parse(b"<html><body/></html>"),
        Err(RdmError::NotAnExport)
    ));
}

#[test]
fn entities_are_never_expanded_or_fetched() {
    let mut laughs = String::from("<?xml version=\"1.0\"?>\n<!DOCTYPE RDMExport [\n");
    laughs.push_str("<!ENTITY l0 \"lol\">\n");
    for level in 1..10 {
        let previous = format!("&l{};", level - 1).repeat(10);
        let _ = writeln!(laughs, "<!ENTITY l{level} \"{previous}\">");
    }
    laughs.push_str("]>\n");
    laughs.push_str(&export(&ssh("0a", "&l9;")));
    let external = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE RDMExport [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>{}",
        export(&ssh("0a", "&x;"))
    );
    let started = Instant::now();
    for text in [laughs, external] {
        assert!(
            matches!(parse(text.as_bytes()), Err(RdmError::Invalid { .. })),
            "{text}"
        );
    }
    assert!(started.elapsed().as_secs() < 5);
}

#[test]
fn a_file_over_the_cap_is_not_read() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("big.rdm");
    let file = fs::File::create(&path).expect("file");
    file.set_len(MAX_FILE_BYTES + 1).expect("size");
    assert!(matches!(read_file(&path), Err(RdmError::TooLarge)));
    assert!(matches!(read_file(dir.path()), Err(RdmError::NotAFile)));
}

#[test]
fn utf16_with_a_byte_order_mark_is_read() {
    let text = export(&ssh("0a", "wide"));
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let found = parse(&bytes).expect("parses");
    assert_eq!(found.entries[0].name, "wide");
}

#[test]
fn an_id_twice_is_refused_the_second_time() {
    let found = parse(export(&format!("{}{}", ssh("0A", "one"), ssh("0a", "two"))).as_bytes())
        .expect("parses");
    assert_eq!(found.entries[0].kind, Ok(ConnectionKind::Ssh));
    assert_eq!(found.entries[1].kind, Err(RdmSkip::DuplicateId));
}

#[test]
fn folders_too_deep_or_with_control_characters_are_refused() {
    let deep =
        ssh("0a", "deep").replace("<Name>", "<Group>a\\b\\c\\d\\e\\f\\g\\h\\i</Group><Name>");
    let control = ssh("0b", "control").replace("<Name>", "<Group>a\tb</Group><Name>");
    let found = parse(export(&format!("{deep}{control}")).as_bytes()).expect("parses");
    assert_eq!(found.entries[0].kind, Err(RdmSkip::TooDeep));
    assert_eq!(found.entries[1].kind, Err(RdmSkip::FolderInvalid));
}

#[test]
fn more_entries_than_the_cap_are_cut_with_a_mark() {
    let many: String = (0..=MAX_ENTRIES)
        .map(|at| ssh(&format!("{at:x}"), "x"))
        .collect();
    let found = parse(export(&many).as_bytes()).expect("parses");
    assert_eq!(found.entries.len(), MAX_ENTRIES);
    assert!(found.truncated);
}

#[test]
fn an_empty_export_and_the_bare_list_are_read() {
    let empty = parse(b"<RDMExport><DatabaseID>x</DatabaseID><Version>2</Version></RDMExport>")
        .expect("parses");
    assert!(empty.entries.is_empty());
    let bare = parse(
        format!(
            "<ArrayOfConnection>{}</ArrayOfConnection>",
            ssh("0a", "bare")
        )
        .as_bytes(),
    )
    .expect("parses");
    assert_eq!(bare.entries[0].name, "bare");
}

#[test]
fn only_plain_local_key_paths_are_taken() {
    for path in [
        r"C:\Users\me\.ssh\id_ed25519",
        "D:/keys/id_rsa",
        "/home/me/.ssh/id_ed25519",
        "~/.ssh/id_ed25519",
    ] {
        assert!(key_path_allowed(path), "{path}");
    }
    for path in [
        r"\\server\share\id_rsa",
        "//server/share/id_rsa",
        r"\\?\C:\keys\id_rsa",
        r"\\.\pipe\x",
        r"keys\id_rsa",
        r"$MYDOCUMENTS$\id_rsa",
        r"C:\keys\..\..\Windows\win.ini",
        "C:\\keys\\a\u{0}b",
    ] {
        assert!(!key_path_allowed(path), "{path}");
    }
}
