//! One mask over every tool answer, ported from rDownloader's `rd-api-mcp/src/mask.rs`.
//!
//! Masking each tool that happens to carry a credential today would miss the next one, so every
//! answer passes through here on its way out of `RcMcpServer::call_tool`: every string at any
//! depth - text content, structured content, a JSON-RPC error's message and data. A string that
//! is an address, without white space, loses its credentials through [`redact_url`]; any other string goes through
//! [`redact_text`]; a JSON field whose *name* is a credential's (`"password": "..."`) loses its
//! string value; the body of a private key block (PEM, PuTTY) in any string becomes one
//! placeholder line (`rc_engine::viewer::secret`). A string with nothing to hide comes out
//! exactly as it went in, so paths and names a later tool takes as input survive.
//!
//! A second pass replaces, word for word, every secret the engine stored or decrypted in this
//! process, after the vault locked too ([`rc_engine::Engine::scrub_secrets`]): a password that
//! turns up in a terminal screen or in a connection's notes has no name or shape to be found by.
//!
//! What the mask cannot walk does not leave (RC-080-12): an answer of another shape than a
//! complete result becomes the error `answer_unmaskable`, and a content block that does not
//! survive the way through JSON is replaced by a note.

use rc_engine::viewer::secret::mask_private_keys;
use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock};
use serde_json::Value;

use crate::{
    answer::error_result,
    redact::{PLACEHOLDER, is_secret_name, redact_text, redact_url},
};

/// What stands in for a content block the mask cannot walk.
const WITHHELD: &str = "[content withheld: it cannot be checked for credentials]";

/// The word-for-word pass: `text` with the known secrets replaced, `None` when none occurs.
pub(crate) type Known<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The answer of one tool call with every credential masked and every `known` secret replaced.
pub(crate) fn mask_response(
    answer: Result<CallToolResponse, rmcp::ErrorData>,
    known: Known<'_>,
) -> Result<CallToolResponse, rmcp::ErrorData> {
    match answer {
        Ok(CallToolResponse::Complete(mut result)) => {
            mask_result(&mut result, known);
            Ok(CallToolResponse::Complete(result))
        }
        // No tool produces the other shapes; one that did would leave unmasked: fail closed.
        Ok(_) => Ok(CallToolResponse::Complete(error_result(
            "answer_unmaskable",
            "the tool answered in a shape the mask cannot check; nothing of it is shown",
        ))),
        Err(mut error) => {
            if let Some(masked) = mask_string(&error.message, known) {
                error.message = masked.into();
            }
            if let Some(data) = error.data.as_mut() {
                mask_value_with(data, known);
            }
            Err(error)
        }
    }
}

fn mask_result(result: &mut CallToolResult, known: Known<'_>) {
    for block in &mut result.content {
        if let ContentBlock::Text(content) = block {
            if let Some(masked) = mask_text(&content.text, known) {
                content.text = masked;
            }
        } else {
            let walked = serde_json::to_value(&*block).ok().and_then(|mut value| {
                if mask_value_with(&mut value, known) {
                    serde_json::from_value(value).ok().map(Some)
                } else {
                    Some(None)
                }
            });
            match walked {
                // Nothing to hide.
                Some(None) => {}
                Some(Some(masked)) => *block = masked,
                // The mask cannot walk it, or it does not come back: it does not leave.
                None => *block = ContentBlock::text(WITHHELD),
            }
        }
    }
    if let Some(structured) = result.structured_content.as_mut() {
        mask_value_with(structured, known);
    }
}

/// A text block: nearly always a JSON document, walked value by value so an address is seen
/// whole; anything else as prose.
fn mask_text(text: &str, known: Known<'_>) -> Option<String> {
    match serde_json::from_str::<Value>(text) {
        Ok(mut value) => {
            if !mask_value_with(&mut value, known) {
                return None;
            }
            serde_json::to_string_pretty(&value).ok()
        }
        Err(_) => mask_string(text, known),
    }
}

/// Masks every string in `value`, keys included; whether anything changed.
#[cfg(test)]
pub(crate) fn mask_value(value: &mut Value) -> bool {
    mask_value_with(value, &|_: &str| None)
}

/// [`mask_value`] with the known secrets replaced too; also what a question in the window
/// shows of a call's arguments (`ask.rs`).
pub(crate) fn mask_value_with(value: &mut Value, known: Known<'_>) -> bool {
    match value {
        Value::String(text) => match mask_string(text, known) {
            Some(masked) => {
                *text = masked;
                true
            }
            None => false,
        },
        Value::Array(items) => items.iter_mut().fold(false, |changed, item| {
            mask_value_with(item, known) | changed
        }),
        Value::Object(map) => {
            let mut changed = false;
            if map.keys().any(|key| mask_string(key, known).is_some()) {
                let entries = std::mem::take(map);
                for (key, nested) in entries {
                    map.insert(mask_string(&key, known).unwrap_or(key), nested);
                }
                changed = true;
            }
            for (key, nested) in map.iter_mut() {
                changed |= match nested {
                    Value::String(text) if is_secret_name(key) && text != PLACEHOLDER => {
                        PLACEHOLDER.clone_into(text);
                        true
                    }
                    _ => mask_value_with(nested, known),
                };
            }
            changed
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// The masked string with the known secrets replaced, or `None` when it has nothing to hide.
fn mask_string(text: &str, known: Known<'_>) -> Option<String> {
    let redacted = redact_string(text);
    known(redacted.as_deref().unwrap_or(text)).or(redacted)
}

/// The string with its credentials masked, or `None` when it has none.
fn redact_string(text: &str) -> Option<String> {
    let masked = match whole_address(text) {
        Some(address) => {
            let masked = redact_url(&address);
            // Nothing to hide: the text as it came, not as the parser writes the address.
            if masked == address.as_str() {
                text.to_owned()
            } else {
                masked
            }
        }
        // Addresses of any scheme first, then what the text rules know.
        None => match redact_addresses(text) {
            Some(masked) => redact_text(&masked),
            None => redact_text(text),
        },
    };
    let masked = mask_private_keys(&masked).unwrap_or(masked);
    (masked != text).then_some(masked)
}

/// `text` as one address with a host - any scheme, so `sftp://user:pass@host` is seen whole -
/// or `None`. Text with white space or a control character is never one (RC-110-20): the parser
/// drops tabs and line breaks and encodes spaces, so a note whose first line is an address
/// would count as that address down to its last line, and none of its lines would be masked.
fn whole_address(text: &str) -> Option<url::Url> {
    if text
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return None;
    }
    url::Url::parse(text).ok().filter(url::Url::has_host)
}

/// `text` with the user info of every address in it masked, whatever its scheme, or `None`
/// when it holds none: `redact_text` searches a list of schemes, and `postgres://user:pass@host`
/// in a text is on none of them (RC-110-20). Word by word between white space, which stays
/// exactly as it was.
fn redact_addresses(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut rest = text;
    while !rest.is_empty() {
        let word = rest
            .find(|character: char| !character.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..word]);
        rest = &rest[word..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        match redact_address_in(&rest[..end]) {
            Some(masked) => {
                out.push_str(&masked);
                changed = true;
            }
            None => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    changed.then_some(out)
}

/// A word without white space with the user info of the address in it masked, or `None` when
/// it holds no address with a host and user info. The address starts at its scheme (`url=` and
/// an opening bracket or quote before it stay) and ends at a quote or angle bracket.
fn redact_address_in(word: &str) -> Option<String> {
    let colon = word.find("://")?;
    let start = word[..colon]
        .rfind(|character: char| !(character.is_ascii_alphanumeric() || "+-.".contains(character)))
        .map_or(0, |before| before + 1);
    let end = word[colon..]
        .find(['"', '\'', '<', '>', '`'])
        .map_or(word.len(), |offset| colon + offset);
    let address = url::Url::parse(&word[start..end]).ok()?;
    if !address.has_host() || (address.username().is_empty() && address.password().is_none()) {
        return None;
    }
    let masked = redact_url(&address);
    (masked != word[start..end]).then(|| format!("{}{masked}{}", &word[..start], &word[end..]))
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
