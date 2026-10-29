//! The `tracing` layer that feeds the error log, after rDownloader's
//! `rd-diagnostics/src/capture.rs`.
//!
//! Only `warn` and `error` events become lines; `info`, `debug` and `trace` return before a
//! single field is read, whatever filter the layer runs under. A line is masked
//! ([`crate::redact`]: a credential-named field loses its value, every address its user info,
//! the text its `password=...`) and then capped, before it leaves the event. It goes to the sink
//! (`diagnostics.rs`) through a bounded channel: a burst beyond [`CHANNEL_CAPACITY`] is dropped,
//! never buffered without end, and logging never waits for the database.

use std::fmt;

use tokio::sync::mpsc;
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

use super::{LogLevel, NewErrorLogEntry};
use crate::redact::{PLACEHOLDER, is_secret_name, redact_text};

/// Bytes of a stored message at most; a longer one is cut at a character boundary and ends in
/// [`CUT_MARK`].
pub(crate) const MAX_MESSAGE_BYTES: usize = 2_048;

/// Bytes of a stored target or code at most.
const MAX_NAME_BYTES: usize = 128;

/// What a cut line ends in.
const CUT_MARK: &str = "...";

/// Lines waiting for the sink at most.
pub(crate) const CHANNEL_CAPACITY: usize = 1_024;

/// The target of the sink's own complaints, which the layer never stores: a database that cannot
/// be written would otherwise feed its failure back into the queue it cannot drain.
pub(crate) const UNSTORED_TARGET: &str = "rc_db::unstored";

/// Hands every `warn` and `error` event to the sink as a masked, capped line.
pub(crate) struct ErrorLogLayer {
    sender: mpsc::Sender<NewErrorLogEntry>,
}

/// A layer and the receiver its lines arrive on.
pub(crate) fn channel() -> (ErrorLogLayer, mpsc::Receiver<NewErrorLogEntry>) {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    (ErrorLogLayer { sender }, receiver)
}

impl<S: Subscriber> Layer<S> for ErrorLogLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if let Some(entry) = entry_of(event) {
            // Full or closed: the line is lost, the program goes on.
            let _ = self.sender.try_send(entry);
        }
    }
}

/// The line `event` becomes, or `None` for one the error log does not keep.
pub(crate) fn entry_of(event: &Event<'_>) -> Option<NewErrorLogEntry> {
    let metadata = event.metadata();
    let level = match *metadata.level() {
        Level::ERROR => LogLevel::Error,
        Level::WARN => LogLevel::Warn,
        _ => return None,
    };
    if metadata.target() == UNSTORED_TARGET {
        return None;
    }
    let mut fields = Fields::default();
    event.record(&mut fields);
    Some(NewErrorLogEntry {
        level,
        target: cap(metadata.target().to_owned(), MAX_NAME_BYTES),
        message: cap(redact_text(&fields.text()), MAX_MESSAGE_BYTES),
        code: fields
            .code
            .map(|code| cap(redact_text(&code), MAX_NAME_BYTES)),
    })
}

/// `text` cut to at most `max` bytes, at a character boundary, ending in [`CUT_MARK`].
pub(crate) fn cap(mut text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max.saturating_sub(CUT_MARK.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(CUT_MARK);
    text
}

/// An event's fields: the message, `code`, and every other field as `name=value`.
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
    code: Option<String>,
}

impl Fields {
    fn push(&mut self, name: &str, value: String) {
        match name {
            "message" => self.message = value,
            "code" => self.code = Some(value),
            _ => {
                let value = if is_secret_name(name) {
                    PLACEHOLDER.to_owned()
                } else {
                    value
                };
                self.rest.push(' ');
                self.rest.push_str(name);
                self.rest.push('=');
                self.rest.push_str(&value);
            }
        }
    }

    /// The message, then the other fields.
    fn text(&self) -> String {
        format!("{}{}", self.message, self.rest)
            .trim_start()
            .to_owned()
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field.name(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field.name(), value.to_owned());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.push(field.name(), value.to_string());
    }
}

#[cfg(test)]
#[path = "layer_tests.rs"]
mod tests;
