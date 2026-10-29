use std::{fs, path::Path};

use super::{LoadError, holds_key, is_key_file};
use crate::import::ssh_config::{SshConfigContext, SshConfigImport, load, parse_str};

/// PEM armour, split so that no secret scanner takes the test for a leaked key.
const ARMOUR: &str = concat!("-----BEGIN ", "OPENSSH PRIVATE KEY-----\n");

#[test]
fn key_file_names_are_recognised() {
    for name in [
        "id_rsa",
        "id_ed25519",
        "ID_ECDSA",
        "server.pem",
        "deploy.KEY",
        "putty.ppk",
        "id_rsa.pub",
        "authorized_keys",
        "known_hosts",
    ] {
        assert!(
            is_key_file(&Path::new("/home/me/.ssh").join(name)),
            "{name}"
        );
    }
    for name in ["config", "work.conf", "10-home", "identity_notes"] {
        assert!(
            !is_key_file(&Path::new("/home/me/.ssh/config.d").join(name)),
            "{name}"
        );
    }
}

#[test]
fn key_text_is_recognised() {
    assert!(holds_key(&format!("{ARMOUR}b3BlbnNzaC1rZXk=\n")));
    assert!(holds_key(&format!("Host x\n{ARMOUR}")));
    assert!(holds_key(
        "PuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none\n"
    ));
    assert!(!holds_key("Host web\n    HostName web.example.org\n"));
}

/// A `.ssh` directory in a temp dir with `config` holding `text`.
fn ssh_dir(text: &str) -> (tempfile::TempDir, std::path::PathBuf, SshConfigContext) {
    let dir = tempfile::tempdir().expect("temp dir");
    let ssh = dir.path().join(".ssh");
    fs::create_dir_all(&ssh).expect("ssh dir");
    fs::write(ssh.join("config"), text).expect("config");
    let context = SshConfigContext::new(dir.path(), "tester");
    (dir, ssh, context)
}

fn refused_file(result: Result<SshConfigImport, LoadError>) -> String {
    match result {
        Err(LoadError::KeyMaterial { file }) => file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        other => panic!("refused as key material, got {other:?}"),
    }
}

#[test]
fn a_key_named_as_the_configuration_is_never_opened() {
    let (_dir, ssh, context) = ssh_dir("Host web\n");
    fs::write(ssh.join("id_ed25519"), "Host leaked\n").expect("key");
    assert_eq!(
        refused_file(load(&ssh.join("id_ed25519"), &context)),
        "id_ed25519"
    );
}

#[test]
fn a_configuration_holding_key_text_is_refused_whole() {
    let (_dir, ssh, context) = ssh_dir(&format!("Host web\n{ARMOUR}abc\n"));
    assert_eq!(refused_file(load(&ssh.join("config"), &context)), "config");
    let putty = "PuTTY-User-Key-File-3: ssh-ed25519\n";
    assert!(matches!(
        parse_str(putty, &ssh.join("config"), &context),
        Err(LoadError::KeyMaterial { .. })
    ));
}

#[test]
fn an_included_key_stops_the_whole_import() {
    let (_dir, ssh, context) = ssh_dir("Host before\nInclude id_rsa\nHost after\n");
    fs::write(ssh.join("id_rsa"), "Host leaked\n").expect("key");
    assert_eq!(refused_file(load(&ssh.join("config"), &context)), "id_rsa");

    let (_dir, ssh, context) = ssh_dir("Include config.d/*\nHost after\n");
    fs::create_dir_all(ssh.join("config.d")).expect("config.d");
    fs::write(ssh.join("config.d/work"), ARMOUR).expect("work");
    assert_eq!(refused_file(load(&ssh.join("config"), &context)), "work");
}

#[cfg(unix)]
#[test]
fn a_link_to_a_key_is_refused_by_its_target() {
    let (_dir, ssh, context) = ssh_dir("Include config.d/*\n");
    fs::create_dir_all(ssh.join("config.d")).expect("config.d");
    fs::write(ssh.join("deploy.pem"), "Host leaked\n").expect("key");
    std::os::unix::fs::symlink(ssh.join("deploy.pem"), ssh.join("config.d/work")).expect("link");
    assert_eq!(refused_file(load(&ssh.join("config"), &context)), "work");
}
