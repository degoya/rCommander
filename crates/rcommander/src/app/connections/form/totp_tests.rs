use rc_engine::{Connection, ConnectionKind, CredentialId, SecretChange, SecretKind};
use rc_vault::{ExposeSecret as _, SecretString};
use serde_json::json;

use super::super::Edit;
use super::{Field, Form, secret_field};

const SEED: &str = "otpauth://totp/web?secret=JBSWY3DPEHPK3PXP";

/// A stored SSH connection, with or without a seed.
fn stored(has_totp_seed: bool) -> Connection {
    serde_json::from_value(json!({
        "id": 7, "folder_id": null, "name": "web", "kind": "ssh", "host": "example.org",
        "port": 22, "username": "deploy", "auth_method": "password", "key_path": null,
        "login_script_id": null, "default_view": "files", "start_directory": null,
        "notes": null, "sort_order": 0, "created_at": "", "updated_at": "",
        "has_password": false, "has_key": false, "has_passphrase": false,
        "has_totp_seed": has_totp_seed, "key": null
    }))
    .expect("a connection")
}

fn seed_set(changes: &[SecretChange]) -> Option<String> {
    changes.iter().find_map(|change| match change {
        SecretChange::Set {
            kind: SecretKind::TotpSeed,
            value,
        } => Some(serde_json::to_string(value).expect("serializes")),
        _ => None,
    })
}

#[test]
fn an_ssh_connection_shows_the_seed_field_and_sends_what_is_typed() {
    let mut form = Form::of(&stored(false), false);
    assert!(form.fields().contains(&Field::TotpSeed));
    assert!(form.plan().expect("valid").secrets.is_empty());
    form.secret(SecretKind::TotpSeed, SecretString::from(SEED));
    assert_eq!(form.totp_seed.expose_secret(), SEED);
    assert!(form.dirty);
    let plan = form.plan().expect("valid");
    // The value travels in the plan only; its serde form is redacted.
    assert_eq!(seed_set(&plan.secrets).as_deref(), Some("\"[redacted]\""));
    form.saved();
    assert!(form.totp_seed.expose_secret().is_empty());
}

#[test]
fn ftp_and_shared_credentials_have_no_seed_field() {
    let mut form = Form::new(None);
    form.set_kind(ConnectionKind::Ftp);
    assert!(!form.fields().contains(&Field::TotpSeed));
    form.secret(SecretKind::TotpSeed, SecretString::from(SEED));
    form.edit(Edit::Name("ftp".into()));
    form.edit(Edit::Host("ftp.example.org".into()));
    assert_eq!(seed_set(&form.plan().expect("valid").secrets), None);
    assert!(
        !Form::new_credential(None)
            .fields()
            .contains(&Field::TotpSeed)
    );
}

#[test]
fn a_connection_on_a_shared_credential_keeps_its_own_seed() {
    let mut form = Form::of(&stored(true), false);
    form.set_shared(Some(CredentialId(4)));
    let fields = form.fields();
    assert!(fields.contains(&Field::TotpSeed));
    assert!(fields.contains(&Field::Forget(SecretKind::TotpSeed)));
    form.toggle(Field::Forget(SecretKind::TotpSeed));
    form.secret(SecretKind::TotpSeed, SecretString::from(SEED));
    let secrets = form.plan().expect("valid").secrets;
    assert_eq!(
        secrets.first(),
        Some(&SecretChange::Clear {
            kind: SecretKind::TotpSeed
        })
    );
    assert!(seed_set(&secrets).is_some());
    assert_eq!(secrets.len(), 2, "no other secret of its own");
}

#[test]
fn a_refused_seed_stands_at_its_field() {
    let form = Form::of(&stored(false), false);
    assert_eq!(form.field_of("totp_seed_invalid"), Some(Field::TotpSeed));
    assert_eq!(secret_field(SecretKind::TotpSeed), Field::TotpSeed);
    assert_eq!(secret_field(SecretKind::Passphrase), Field::Passphrase);
    assert_eq!(secret_field(SecretKind::Password), Field::Password);
}
