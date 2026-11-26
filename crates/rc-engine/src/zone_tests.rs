use super::host_url;
use crate::ConnectionKind;

#[test]
fn a_connection_is_its_scheme_host_and_port() {
    assert_eq!(
        host_url(ConnectionKind::Ssh, "files.example.org", 22),
        "sftp://files.example.org:22/"
    );
    assert_eq!(
        host_url(ConnectionKind::Ftp, "10.0.0.5", 21),
        "ftp://10.0.0.5:21/"
    );
    assert_eq!(host_url(ConnectionKind::Ftps, "h", 21), "ftps://h:21/");
    assert_eq!(
        host_url(ConnectionKind::FtpsImplicit, "h", 990),
        "ftps://h:990/"
    );
}

#[test]
fn an_ipv6_host_goes_in_brackets_once() {
    assert_eq!(
        host_url(ConnectionKind::Ssh, "fe80::1", 22),
        "sftp://[fe80::1]:22/"
    );
    assert_eq!(
        host_url(ConnectionKind::Ssh, "[::1]", 2222),
        "sftp://[::1]:2222/"
    );
}
