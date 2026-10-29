use super::*;

const REDACTED: &str = "[redacted]";

#[test]
fn a_remembered_secret_is_found_wherever_it_occurs() {
    let mut prints = Fingerprints::default();
    prints.remember("hunter2-canary");
    assert_eq!(prints.scrub("nothing here", REDACTED), None);
    assert_eq!(prints.scrub("", REDACTED), None);
    assert_eq!(
        prints
            .scrub(
                "hunter2-canary at the start, in-hunter2-canary-between",
                REDACTED
            )
            .as_deref(),
        Some("[redacted] at the start, in-[redacted]-between")
    );
    assert_eq!(
        prints.scrub("hunter2-canary", REDACTED).as_deref(),
        Some(REDACTED),
        "the whole text"
    );
    assert_eq!(prints.scrub("hunter2-canar", REDACTED), None, "a part");
}

#[test]
fn overlapping_and_multibyte_matches_are_replaced_once_on_char_boundaries() {
    let mut prints = Fingerprints::default();
    prints.remember("abcdef");
    prints.remember("defghi");
    prints.remember("p\u{e4}ss\u{e4}");
    assert_eq!(
        prints.scrub("xabcdefghiy", REDACTED).as_deref(),
        Some("x[redacted]y")
    );
    assert_eq!(
        prints
            .scrub("\u{fc}p\u{e4}ss\u{e4}\u{fc} p\u{e4}ss", REDACTED)
            .as_deref(),
        Some("\u{fc}[redacted]\u{fc} p\u{e4}ss")
    );
}

#[test]
fn no_value_is_kept_only_its_length_and_hashes() {
    let mut prints = Fingerprints::default();
    prints.remember("hunter2-canary");
    prints.remember("hunter2-canary");
    assert_eq!(prints.len(), 1, "remembered once");
    let print = prints.order[0];
    assert_eq!(print.len, "hunter2-canary".len());
    assert_ne!(
        print.hash,
        Fingerprints::default()
            .keys
            .hash_one(b"hunter2-canary".as_slice())
    );
}

#[test]
fn at_most_the_bound_is_kept_and_the_oldest_go_first() {
    let mut prints = Fingerprints::default();
    for index in 0..MAX_FINGERPRINTS + 10 {
        prints.remember(&format!("secret-number-{index:05}"));
    }
    assert_eq!(prints.len(), MAX_FINGERPRINTS);
    assert_eq!(prints.scrub("secret-number-00009", REDACTED), None);
    assert!(prints.scrub("secret-number-00010", REDACTED).is_some());
    assert!(
        prints
            .scrub(
                &format!("secret-number-{:05}", MAX_FINGERPRINTS + 9),
                REDACTED
            )
            .is_some()
    );
}

/// RC-080-47: a secret seen again counts as new and outlasts the ones seen once since.
#[test]
fn a_secret_seen_again_is_kept_longer() {
    let mut prints = Fingerprints::default();
    prints.remember("the-password-in-use");
    for index in 0..MAX_FINGERPRINTS + 10 {
        prints.remember(&format!("secret-number-{index:05}"));
        if index % 100 == 0 {
            prints.remember("the-password-in-use");
        }
    }
    assert_eq!(prints.len(), MAX_FINGERPRINTS);
    assert!(prints.scrub("the-password-in-use", REDACTED).is_some());
    assert_eq!(prints.scrub("secret-number-00010", REDACTED), None);
}

/// The rolling hash of every window equals the hash computed afresh.
#[test]
fn the_rolling_hash_rolls() {
    let prints = Fingerprints::default();
    let text = b"\x00\xffthe quick brown fox \x00 jumps";
    for len in [1, 2, 7, text.len()] {
        let top = (1..len).fold(1, |power, _| mul(power, prints.base));
        let mut rolling = prints.rolling(&text[..len]);
        for start in 1..=text.len() - len {
            let out = mul(u64::from(text[start - 1]) + 1, top);
            let into = u64::from(text[start + len - 1]) + 1;
            rolling = add(mul(sub(rolling, out), prints.base), into);
            assert_eq!(rolling, prints.rolling(&text[start..start + len]));
        }
    }
    assert_eq!(mul(MODULUS - 1, MODULUS - 1), 1);
    assert_eq!(sub(0, 1), MODULUS - 1);
}
