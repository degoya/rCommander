use super::*;

/// A PEM line, put together here so no scanner takes the test for a leaked key.
fn pem(edge: &str, kind: &str) -> String {
    format!("-----{edge} {kind} {}-----\n", "PRIVATE KEY")
}

fn key_file() -> String {
    format!(
        "{}b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\nAAAAAAABAAAAMwAAAAtzc2gtZWQyNTUxOQ\n{}",
        pem("BEGIN", "OPENSSH"),
        pem("END", "OPENSSH")
    )
}

#[test]
fn a_key_block_becomes_one_placeholder_line() {
    let text = format!("before\n{}after\n", key_file());
    let masked = mask_private_keys(&text).expect("masked");
    assert_eq!(masked, "before\n[private key redacted]\nafter\n");
    // Masked once is masked for good.
    assert_eq!(mask_private_keys(&masked), None);
}

#[test]
fn two_blocks_and_every_pem_kind_are_masked() {
    let text = format!(
        "{}one\n{}cert stays\n{}two\n{}",
        pem("BEGIN", "RSA"),
        pem("END", "RSA"),
        pem("BEGIN", "ENCRYPTED"),
        pem("END", "ENCRYPTED")
    );
    let masked = mask_private_keys(&text).expect("masked");
    assert_eq!(
        masked,
        format!("{PLACEHOLDER}\ncert stays\n{PLACEHOLDER}\n")
    );
}

#[test]
fn a_window_cut_inside_a_key_hides_the_cut_part() {
    let starts_inside = format!(
        "AAAAAAABAAAAMwAAAAtzc2gtZWQyNTUxOQ\n{}tail\n",
        pem("END", "RSA")
    );
    let masked = mask_private_keys(&starts_inside).expect("masked");
    assert_eq!(masked, format!("{PLACEHOLDER}\ntail\n"));
    let ends_inside = format!("head\n{}b3BlbnNzaC1rZXktdjEAAAA", pem("BEGIN", "EC"));
    let masked = mask_private_keys(&ends_inside).expect("masked");
    assert_eq!(masked, format!("head\n{PLACEHOLDER}"));
}

#[test]
fn a_putty_key_is_masked_from_its_header_to_its_mac() {
    let ppk = "before\nPuTTY-User-Key-File-3: ssh-ed25519\nPublic-Lines: 1\nAAAApublic\nPrivate-Lines: 2\nsecretone\nsecrettwo\nPrivate-MAC: 00ff\nafter\n";
    let masked = mask_private_keys(ppk).expect("masked");
    assert_eq!(masked, format!("before\n{PLACEHOLDER}\nafter\n"));
    let cut = "secrettwo\nPrivate-MAC: 00ff\n";
    assert_eq!(
        mask_private_keys(cut).expect("masked"),
        format!("{PLACEHOLDER}\n")
    );
}

#[test]
fn text_without_a_key_comes_back_unchanged() {
    let text = "a public key: ssh-ed25519 AAAAC3Nz user@host\n";
    assert_eq!(mask_private_keys(text), None);
    assert!(!has_private_key(text.as_bytes()));
}

#[test]
fn a_key_marker_is_seen_in_ascii_and_utf16() {
    let ascii = key_file();
    assert!(has_private_key(ascii.as_bytes()));
    let utf16le: Vec<u8> = ascii.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert!(has_private_key(&utf16le));
    let utf16be: Vec<u8> = ascii.encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert!(has_private_key(&utf16be));
    assert!(has_private_key(b"PuTTY-User-Key-File-2: ssh-rsa\n"));
}

/// A window of nothing but a key's body carries no marker: the text around it tells.
#[test]
fn a_window_inside_a_key_body_is_known_by_its_surroundings() {
    let begin = pem("BEGIN", "OPENSSH");
    let end = pem("END", "OPENSSH");
    assert!(inside_key_block(&format!("text\n{begin}AAAA\n"), "BBBB\n"));
    assert!(inside_key_block("AAAA\n", &format!("BBBB\n{end}text\n")));
    assert!(inside_key_block("", &end));
    assert!(!inside_key_block(&format!("{begin}AAAA\n{end}"), "text\n"));
    assert!(!inside_key_block(
        "text\n",
        &format!("text\n{begin}AAAA\n{end}")
    ));
    assert!(!inside_key_block("plain\n", "plain\n"));
    let putty_open = format!("{}2: ssh-ed25519\n", "PuTTY-User-Key-File-");
    assert!(inside_key_block(&putty_open, "Private-Lines: 1\n"));
    assert!(inside_key_block("", "Private-MAC: 00\n"));
}
