use suppaftp::types::Features;

use super::ServerInfo;

fn features(names: &[&str]) -> Features {
    names
        .iter()
        .map(|name| ((*name).to_owned(), None))
        .collect()
}

#[test]
fn a_modern_unix_server_is_described() {
    let info = ServerInfo::from_replies(
        &features(&["UTF8", "MLST", "MFMT", "EPSV", "REST"]),
        "UNIX Type: L8",
    );
    assert_eq!(
        info,
        ServerInfo {
            utf8: true,
            mlst: true,
            mfmt: true,
            epsv: true,
            unix: true,
            case_sensitive: true,
        }
    );
}

#[test]
fn windows_servers_are_case_insensitive() {
    let iis = ServerInfo::from_replies(&features(&[]), "Windows_NT");
    assert!(!iis.case_sensitive);
    assert!(!iis.unix);
    let filezilla = ServerInfo::from_replies(&features(&["mlsd"]), "UNIX emulated by FileZilla");
    assert!(!filezilla.case_sensitive);
    assert!(filezilla.unix);
    assert!(filezilla.mlst, "FEAT names are matched in any case");
}

#[test]
fn a_bare_server_offers_nothing_optional() {
    let info = ServerInfo::from_replies(&features(&[]), "");
    assert!(!info.utf8 && !info.mlst && !info.mfmt && !info.epsv);
}
