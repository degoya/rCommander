//! The case probe: which name it asks for, what counts as the same object, and its verdict
//! against the in-process server - one that folds case on any disk, a plain one, a home that is
//! not there - and the caller's word over the probe.

use std::path::Path;

use rc_vfs::{Vfs, conformance};

use super::*;
use crate::{
    SftpVfs,
    sftp::{SftpLimits, vfs::tests::Fixture},
    testkit::SftpServe,
};

/// A served directory with `/home/tester` in it, the server's home.
fn at_home(serve: &mut SftpServe) {
    std::fs::create_dir_all(serve.root.join("home/tester")).expect("home");
    serve.home = "/home/tester".to_owned();
}

/// The backend of `fixture` opened with `case`, and the session it runs on.
async fn opened(fixture: &Fixture, case: SftpCase) -> (SftpVfs, crate::SshSession) {
    let session = fixture.session().await;
    let names = session.open_channel().await.expect("channel");
    let files = session.open_channel().await.expect("channel");
    let vfs = SftpVfs::connect_as(names, files, SftpLimits::default(), case)
        .await
        .expect("sftp");
    (vfs, session)
}

#[test]
fn the_last_name_with_a_letter_is_asked_in_the_other_case() {
    assert_eq!(flipped("/home/Tester").as_deref(), Some("/home/tESTER"));
    assert_eq!(flipped("/data/0001").as_deref(), Some("/DATA/0001"));
    assert_eq!(flipped("/C:/Users/me").as_deref(), Some("/C:/Users/ME"));
    assert_eq!(flipped("/home/\u{e9}1").as_deref(), Some("/HOME/\u{e9}1"));
    assert_eq!(flipped("/"), None);
    assert_eq!(flipped("/1/2"), None);
}

#[test]
fn the_same_object_has_the_same_kind_size_and_time() {
    let mut named = FileAttributes::empty();
    named.set_dir(true);
    named.size = Some(4096);
    named.mtime = Some(1_700_000_000);
    assert!(same_object(&named, &named));
    let mut larger = named.clone();
    larger.size = Some(8192);
    assert!(!same_object(&named, &larger));
    let mut later = named.clone();
    later.mtime = Some(1_700_000_001);
    assert!(!same_object(&named, &later));
    let mut file = named.clone();
    file.set_dir(false);
    file.set_regular(true);
    assert!(!same_object(&named, &file));
}

#[tokio::test]
async fn a_server_that_folds_case_is_found_out() {
    let fixture = Fixture::start(|serve| {
        at_home(serve);
        serve.fold_case = true;
    })
    .await;
    let (vfs, _session) = fixture.vfs().await;
    assert!(!vfs.capabilities().case_sensitive);
}

#[tokio::test]
async fn a_plain_server_is_as_case_sensitive_as_its_disk() {
    let fixture = Fixture::start(at_home).await;
    let (vfs, _session) = fixture.vfs().await;
    assert_eq!(
        vfs.capabilities().case_sensitive,
        crate::ftp::test_server::case_sensitive(fixture.root.path())
    );
}

#[tokio::test]
async fn a_probe_that_fails_counts_as_case_sensitive() {
    // The home the server names is not there: neither name is found.
    let fixture = Fixture::start(|serve| {
        serve.home = "/missing".to_owned();
        serve.fold_case = true;
    })
    .await;
    let (vfs, _session) = fixture.vfs().await;
    assert!(vfs.capabilities().case_sensitive);
    // A home without a letter cannot be probed.
    let fixture = Fixture::start(|serve| serve.fold_case = true).await;
    let (vfs, _session) = fixture.vfs().await;
    assert!(vfs.capabilities().case_sensitive);
}

#[tokio::test]
async fn the_caller_s_word_is_taken_over_the_probe() {
    let folding = Fixture::start(|serve| {
        at_home(serve);
        serve.fold_case = true;
    })
    .await;
    let (vfs, _session) = opened(&folding, SftpCase::Sensitive).await;
    assert!(vfs.capabilities().case_sensitive);
    let plain = Fixture::start(at_home).await;
    let (vfs, _session) = opened(&plain, SftpCase::Insensitive).await;
    assert!(!vfs.capabilities().case_sensitive);
}

#[tokio::test]
async fn sftp_conforms_on_a_server_that_folds_case() {
    let fixture = Fixture::start(|serve| {
        serve.home = "/conformance".to_owned();
        serve.fold_case = true;
    })
    .await;
    std::fs::create_dir(fixture.local("conformance")).expect("root");
    let (vfs, _session) = fixture.vfs().await;
    assert!(!vfs.capabilities().case_sensitive);
    conformance::run(&vfs, Path::new("/conformance")).await;
}
