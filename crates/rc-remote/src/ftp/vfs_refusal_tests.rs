//! A server that refuses a command it offers, or one a client may assume: the panel still
//! opens where it can, and where it cannot the error names the refused command.

use std::path::Path;

use rc_vfs::{Vfs, VfsError};

use super::FtpVfs;
use crate::ftp::{
    Encryption,
    test_server::{Options, ServerTls, TestServer},
};

use super::tests::config;

/// A server of `tls` that answers `500` to every command of `refuse`, a file and a folder in
/// its root.
async fn refusing(tls: ServerTls, mlst: bool, refuse: &'static [&'static str]) -> TestServer {
    let server = TestServer::start(Options {
        tls,
        mlst,
        refuse,
        ..Options::default()
    })
    .await;
    std::fs::write(server.local("file.txt"), b"content").expect("file");
    std::fs::create_dir(server.local("folder")).expect("folder");
    server
}

/// What a panel does on the server: connect, list where the login starts.
async fn open_panel(server: &TestServer, encryption: Encryption) -> rc_vfs::Result<Vec<String>> {
    let vfs = FtpVfs::connect(config(server, encryption))
        .await
        .expect("the login itself works");
    let home = vfs.home().to_path_buf();
    let mut names: Vec<String> = vfs
        .list(&home)
        .await?
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name != "." && name != "..")
        .collect();
    names.sort();
    Ok(names)
}

fn expected() -> Vec<String> {
    vec!["file.txt".to_owned(), "folder".to_owned()]
}

#[tokio::test]
async fn refused_epsv_falls_back_to_pasv() {
    let server = refusing(ServerTls::None, true, &["EPSV"]).await;
    assert_eq!(
        open_panel(&server, Encryption::None).await.expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_epsv_falls_back_to_pasv_with_list_only() {
    let server = refusing(ServerTls::None, false, &["EPSV"]).await;
    assert_eq!(
        open_panel(&server, Encryption::None).await.expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_epsv_falls_back_to_pasv_over_explicit_ftps() {
    let server = refusing(ServerTls::Explicit, true, &["EPSV"]).await;
    assert_eq!(
        open_panel(&server, Encryption::Explicit)
            .await
            .expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_epsv_falls_back_to_pasv_over_implicit_ftps() {
    let server = refusing(ServerTls::Implicit, true, &["EPSV"]).await;
    assert_eq!(
        open_panel(&server, Encryption::Implicit)
            .await
            .expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_mlsd_falls_back_to_list() {
    let server = refusing(ServerTls::None, true, &["MLSD"]).await;
    assert_eq!(
        open_panel(&server, Encryption::None).await.expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_list_option_falls_back_to_plain_list() {
    let server = refusing(ServerTls::None, false, &["LIST -a"]).await;
    let names = open_panel(&server, Encryption::None).await.expect("listed");
    assert_eq!(names, expected());
}

#[tokio::test]
async fn refused_feat_syst_opts_pwd_still_list() {
    let server = refusing(ServerTls::None, true, &["FEAT", "SYST", "OPTS", "PWD"]).await;
    assert_eq!(
        open_panel(&server, Encryption::None).await.expect("listed"),
        expected()
    );
}

#[tokio::test]
async fn refused_mlst_stats_through_size_and_cwd() {
    let server = refusing(ServerTls::None, true, &["MLST"]).await;
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");
    let file = vfs.stat(Path::new("/file.txt")).await.expect("file");
    assert_eq!(file.size, 7);
    assert!(
        vfs.stat(Path::new("/folder"))
            .await
            .expect("folder")
            .is_dir_like()
    );
}

#[tokio::test]
async fn both_passive_modes_refused_name_the_command() {
    let server = refusing(ServerTls::None, true, &["EPSV", "PASV"]).await;
    let error = open_panel(&server, Encryption::None)
        .await
        .expect_err("no data connection");
    let VfsError::Unsupported { operation, .. } = error else {
        panic!("unsupported, got {error:?}");
    };
    assert_eq!(operation, "PASV");
}

/// A `PASV` reply that names another address - a server behind NAT, or one that would turn the
/// client against a third host (FTP bounce) - is followed for its port only: the data
/// connection goes to the control connection's peer, plain and over FTPS.
#[tokio::test]
async fn a_pasv_address_other_than_the_server_is_not_followed() {
    for (tls, encryption) in [
        (ServerTls::None, Encryption::None),
        (ServerTls::Explicit, Encryption::Explicit),
    ] {
        for address in [[10, 0, 0, 5], [127, 0, 0, 2]] {
            let server = TestServer::start(Options {
                tls,
                refuse: &["EPSV"],
                pasv_address: address,
                ..Options::default()
            })
            .await;
            std::fs::write(server.local("file.txt"), b"content").expect("file");
            std::fs::create_dir(server.local("folder")).expect("folder");
            let names = open_panel(&server, encryption)
                .await
                .unwrap_or_else(|error| panic!("listed despite {address:?}: {error:?}"));
            assert_eq!(names, expected());
        }
    }
}
