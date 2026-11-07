use super::*;

/// RFC 6238's seeds: 20, 32 and 64 ASCII bytes, Base32 here.
const SHA1_SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const SHA256_SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====";
const SHA512_SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\
GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA=";

/// RFC 6238, appendix B: time, then the SHA-1, SHA-256 and SHA-512 codes of eight digits.
const VECTORS: [(u64, &str, &str, &str); 6] = [
    (59, "94287082", "46119246", "90693936"),
    (1_111_111_109, "07081804", "68084774", "25091201"),
    (1_111_111_111, "14050471", "67062674", "99943326"),
    (1_234_567_890, "89005924", "91819424", "93441116"),
    (2_000_000_000, "69279037", "90698825", "38618901"),
    (20_000_000_000, "65353130", "77737706", "47863826"),
];

fn seed(secret: &str, algorithm: &str) -> TotpSeed {
    TotpSeed::parse(&format!(
        "otpauth://totp/RFC%206238:test?secret={secret}&algorithm={algorithm}&digits=8&period=30"
    ))
    .expect("a valid seed")
}

#[test]
fn the_rfc_6238_vectors_hold_for_sha1_sha256_and_sha512() {
    let seeds = [
        seed(SHA1_SEED, "SHA1"),
        seed(SHA256_SEED, "SHA256"),
        seed(SHA512_SEED, "SHA512"),
    ];
    for (time, sha1, sha256, sha512) in VECTORS {
        for (seed, expected) in seeds.iter().zip([sha1, sha256, sha512]) {
            assert_eq!(
                seed.code_at(time).expose_secret(),
                expected,
                "{:?} at {time}",
                seed.algorithm()
            );
        }
    }
}

#[test]
fn a_bare_secret_is_sha1_six_digits_thirty_seconds() {
    let seed = TotpSeed::parse(" jbsw y3dp-ehpk 3pxp ").expect("a bare secret");
    assert_eq!(seed.algorithm(), TotpAlgorithm::Sha1);
    assert_eq!((seed.digits(), seed.period()), (6, 30));
    // Worked out with Python's hmac over the same ten bytes.
    assert_eq!(seed.code_at(59).expose_secret(), "996554");
    assert_eq!(seed.code_at(1_234_567_890).expose_secret(), "742275");
    // One period on, the code changes; within it, it stays.
    assert_eq!(
        seed.code_at(30).expose_secret(),
        seed.code_at(59).expose_secret()
    );
}

#[test]
fn a_uri_sets_algorithm_digits_and_period() {
    let seed = TotpSeed::parse(
        "OTPAUTH://TOTP/Example:alice@example.com?issuer=Example&SECRET=JBSWY3DPEHPK3PXP\
         &algorithm=sha256&digits=8&period=60",
    )
    .expect("valid");
    assert_eq!(seed.algorithm(), TotpAlgorithm::Sha256);
    assert_eq!((seed.digits(), seed.period()), (8, 60));
    assert_eq!(seed.code_at(1_234_567_890).expose_secret(), "45806924");
}

#[test]
fn a_broken_uri_is_refused() {
    let cases = [
        ("", TotpError::Empty),
        ("   ", TotpError::Empty),
        (
            "otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1",
            TotpError::NotTotp,
        ),
        ("otpauth://totp", TotpError::NotTotp),
        (
            "https://example.com/?secret=JBSWY3DPEHPK3PXP",
            TotpError::NotTotp,
        ),
        ("otpauth://totp/x?issuer=Example", TotpError::SecretMissing),
        ("otpauth://totp/x?secret=", TotpError::SecretMissing),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PX1",
            TotpError::NotBase32,
        ),
        ("not base32 at all!", TotpError::NotBase32),
        ("JBSWY3DP", TotpError::KeyLength),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
            TotpError::AlgorithmUnknown,
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=4",
            TotpError::DigitsInvalid,
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=six",
            TotpError::DigitsInvalid,
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=0",
            TotpError::PeriodInvalid,
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=86400",
            TotpError::PeriodInvalid,
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&secret=GEZDGNBVGY3TQOJQ",
            TotpError::Malformed {
                reason: "a parameter given twice",
            },
        ),
        (
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP%G1",
            TotpError::Malformed {
                reason: "a broken percent escape",
            },
        ),
    ];
    for (text, expected) in cases {
        let refused = TotpSeed::parse(text).expect_err(text);
        assert_eq!(refused, expected, "{text}");
        assert_eq!(refused.code(), "totp_seed_invalid");
    }
    let long = "A".repeat(MAX_SEED_CHARS + 1);
    assert_eq!(
        TotpSeed::parse(&long).expect_err("too long"),
        TotpError::TooLong {
            max: MAX_SEED_CHARS
        }
    );
}

#[test]
fn neither_debug_nor_an_error_shows_the_secret() {
    let seed = TotpSeed::parse("JBSWY3DPEHPK3PXP").expect("valid");
    let shown = format!("{seed:?}");
    assert!(shown.contains("[redacted]"), "{shown}");
    assert!(!shown.contains("JBSWY3DPEHPK3PXP"), "{shown}");
    let refused = TotpSeed::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PX1").expect_err("bad");
    let shown = format!("{refused} {refused:?}");
    assert!(!shown.contains("JBSWY3DPEHPK3PX1"), "{shown}");
}

#[test]
fn the_default_prompt_takes_the_usual_questions() {
    let pattern = crate::login_script::Pattern::new(DEFAULT_PROMPT).expect("compiles");
    for prompt in [
        "Verification code: ",
        "One-time password (OATH) for `alice': ",
        "Enter OTP: ",
        "Token: ",
    ] {
        assert!(pattern.match_end(prompt).is_some(), "{prompt}");
    }
    for prompt in [
        "Password: ",
        "alice@host's password: ",
        "Passcode or option (1-3): ",
    ] {
        assert!(pattern.match_end(prompt).is_none(), "{prompt}");
    }
}
