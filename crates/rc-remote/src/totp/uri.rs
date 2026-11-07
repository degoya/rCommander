//! The `otpauth://totp/<label>?secret=...&algorithm=...&digits=...&period=...` URI of Google
//! Authenticator's key format: what a QR code for an authenticator holds. The label and the
//! issuer are only names and are not kept; an unknown parameter is ignored, a known one given
//! twice refuses the URI.

use secrecy::zeroize::Zeroizing;

use super::{TotpAlgorithm, TotpError, TotpSeed};

/// What a URI for a seed starts with; the type follows.
pub(super) const SCHEME: &str = "otpauth://";

/// The seed `text` (starting with [`SCHEME`]) describes.
pub(super) fn parse(text: &str) -> Result<TotpSeed, TotpError> {
    let rest = text.get(SCHEME.len()..).ok_or(TotpError::NotTotp)?;
    let (kind, rest) = rest.split_once('/').ok_or(TotpError::NotTotp)?;
    if !kind.eq_ignore_ascii_case("totp") {
        return Err(TotpError::NotTotp);
    }
    let query = rest.split_once('?').map(|(_, query)| query).unwrap_or("");
    let query = query.split_once('#').map_or(query, |(query, _)| query);
    let mut secret: Option<Zeroizing<String>> = None;
    let mut algorithm = None;
    let mut digits = None;
    let mut period = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value).ok_or(TotpError::Malformed {
            reason: "a broken percent escape",
        })?;
        let twice = TotpError::Malformed {
            reason: "a parameter given twice",
        };
        match name.to_ascii_lowercase().as_str() {
            "secret" => {
                if secret.replace(value).is_some() {
                    return Err(twice);
                }
            }
            "algorithm" => {
                let parsed = TotpAlgorithm::parse(&value).ok_or(TotpError::AlgorithmUnknown)?;
                if algorithm.replace(parsed).is_some() {
                    return Err(twice);
                }
            }
            "digits" => {
                let parsed = value.parse::<u32>().map_err(|_| TotpError::DigitsInvalid)?;
                if digits.replace(parsed).is_some() {
                    return Err(twice);
                }
            }
            "period" => {
                let parsed = value.parse::<u64>().map_err(|_| TotpError::PeriodInvalid)?;
                if period.replace(parsed).is_some() {
                    return Err(twice);
                }
            }
            _ => {}
        }
    }
    let secret = secret
        .filter(|secret| !secret.trim().is_empty())
        .ok_or(TotpError::SecretMissing)?;
    TotpSeed::new(
        &secret,
        algorithm.unwrap_or_default(),
        digits.unwrap_or(6),
        period.unwrap_or(30),
    )
}

/// `text` with its `%XX` escapes decoded and `+` as a space; `None` for a broken escape or bytes
/// that are no UTF-8.
fn decode(text: &str) -> Option<Zeroizing<String>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(text.len()));
    let mut rest = text.bytes();
    while let Some(byte) = rest.next() {
        match byte {
            b'%' => {
                let high = hex(rest.next()?)?;
                let low = hex(rest.next()?)?;
                bytes.push((high << 4) | low);
            }
            b'+' => bytes.push(b' '),
            other => bytes.push(other),
        }
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    Some(Zeroizing::new(text.to_owned()))
}

fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}
