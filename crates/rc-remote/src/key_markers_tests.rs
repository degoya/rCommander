use super::{PRIVATE_KEY_MARKERS, holds_key_material, holds_private_key};

/// One armour line, built at run time so the secret scan finds no key in this file.
fn armour(edge: &str, label: &str) -> String {
    format!("-----{edge} {label}-----\n")
}

#[test]
fn every_private_key_format_carries_a_marker() {
    let mut texts: Vec<String> = [
        "OPENSSH PRIVATE KEY",
        "RSA PRIVATE KEY",
        "ENCRYPTED PRIVATE KEY",
    ]
    .iter()
    .flat_map(|label| [armour("BEGIN", label), armour("END", label)])
    .collect();
    texts.extend(
        [
            "PuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none\n",
            "Private-Lines: 1\nAAAA\n",
            "Private-MAC: 0123\n",
        ]
        .map(str::to_owned),
    );
    for text in &texts {
        assert!(holds_private_key(text), "{text:?}");
        assert!(holds_key_material(text), "{text:?}");
    }
}

#[test]
fn public_material_is_key_material_but_no_private_key() {
    for text in [
        armour("BEGIN", "CERTIFICATE"),
        armour("BEGIN", "PUBLIC KEY"),
    ] {
        assert!(!holds_private_key(&text), "{text:?}");
        assert!(holds_key_material(&text), "{text:?}");
    }
    for text in ["Host example\n  User me\n", "ssh-ed25519 AAAA me@host", ""] {
        assert!(!holds_private_key(text), "{text:?}");
        assert!(!holds_key_material(text), "{text:?}");
    }
}

#[test]
fn the_table_has_no_empty_marker() {
    assert!(PRIVATE_KEY_MARKERS.iter().all(|marker| marker.len() > 8));
}
