//! The port a passive-mode reply names: `229 Entering Extended Passive Mode (|||port|)`
//! (RFC 2428) or `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)` (RFC 959). Only the port is
//! taken; the data connection goes to the control connection's peer (`Connection::open_passive`).

/// The port of an `EPSV` reply: the third field between the delimiters in parentheses, which
/// RFC 2428 lets the server choose (`|` by convention).
pub(crate) fn epsv_port(reply: &str) -> Option<u16> {
    let inner = between_parentheses(reply)?;
    let delimiter = inner.chars().next()?;
    let fields: Vec<&str> = inner.split(delimiter).collect();
    match fields.as_slice() {
        ["", "", "", port, ""] => port.parse().ok(),
        _ => None,
    }
}

/// The port of a `PASV` reply: `p1 * 256 + p2` of the six numbers. Some servers leave out
/// the parentheses, so the numbers are looked for after the code.
pub(crate) fn pasv_port(reply: &str) -> Option<u16> {
    let text = between_parentheses(reply).unwrap_or_else(|| reply.get(4..).unwrap_or(""));
    let start = text.find(|character: char| character.is_ascii_digit())?;
    let numbers: Vec<u8> = text[start..]
        .split(',')
        .map(|part| {
            let digits: String = part
                .trim()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .take(6)
        .collect::<Option<_>>()?;
    let [_, _, _, _, high, low] = numbers.as_slice() else {
        return None;
    };
    Some((u16::from(*high) << 8) | u16::from(*low))
}

fn between_parentheses(reply: &str) -> Option<&str> {
    let open = reply.find('(')?;
    let close = open + reply[open..].find(')')?;
    Some(&reply[open + 1..close])
}

#[cfg(test)]
#[path = "passive_tests.rs"]
mod tests;
