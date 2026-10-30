use super::SCAN_CODES;

/// Every key a chord can name by place has its scan code, and only those.
#[test]
fn every_key_named_by_place_has_its_scan_code() {
    let codes: Vec<&str> = rc_engine::PHYSICAL_KEYS
        .iter()
        .map(|(code, _)| *code)
        .collect();
    let scanned: Vec<&str> = SCAN_CODES.iter().map(|(code, _)| *code).collect();
    assert_eq!(scanned, codes);
}

#[cfg(not(windows))]
#[test]
fn without_a_layout_to_ask_no_character_is_known() {
    assert_eq!(super::character("Semicolon"), None);
}
