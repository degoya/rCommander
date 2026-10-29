//! Between the generated WIT types and `rc-plugin-api`'s native ones, and a plugin's own failure
//! made safe to show.
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/component/convert.rs`
//! (`from_wit_failure`: a code of at most 128 characters, at most 16 parameters) at commit
//! 26c8592a6.

use rc_plugin_api::viewer::{Output, Purpose, Theme, TokenClass};
use rc_plugin_types::{Failure, FailureKind};

use crate::bindings::{exports::rcommander::plugin::viewer as wit, rcommander::plugin::types};
use crate::failure::readable;

const MAX_CODE_CHARS: usize = 128;
const MAX_PARAMS: usize = 16;
const MAX_PARAM_KEY_CHARS: usize = 64;
const MAX_PARAM_VALUE_CHARS: usize = 512;
/// The colours of a theme are `#rrggbb`; anything longer is cut before it reaches a plugin.
const MAX_COLOUR_CHARS: usize = 32;

pub(crate) fn purpose(purpose: Purpose) -> wit::Purpose {
    match purpose {
        Purpose::Lister => wit::Purpose::Lister,
        Purpose::QuickView => wit::Purpose::QuickView,
    }
}

pub(crate) fn theme(theme: &Theme) -> wit::Theme {
    let colour = |value: &str| value.chars().take(MAX_COLOUR_CHARS).collect::<String>();
    wit::Theme {
        dark: theme.dark,
        background: colour(&theme.background),
        foreground: colour(&theme.foreground),
        accent: colour(&theme.accent),
    }
}

pub(crate) fn output(output: wit::Output) -> Output {
    match output {
        wit::Output::Text => Output::Text,
        wit::Output::Highlighted => Output::Highlighted,
        wit::Output::Markdown => Output::Markdown,
        wit::Output::Image => Output::Image,
        wit::Output::Tree => Output::Tree,
        wit::Output::Pages => Output::Pages,
    }
}

pub(crate) fn token_class(class: wit::TokenClass) -> TokenClass {
    match class {
        wit::TokenClass::Keyword => TokenClass::Keyword,
        wit::TokenClass::Key => TokenClass::Key,
        wit::TokenClass::String => TokenClass::String,
        wit::TokenClass::Number => TokenClass::Number,
        wit::TokenClass::Literal => TokenClass::Literal,
        wit::TokenClass::Comment => TokenClass::Comment,
        wit::TokenClass::Punctuation => TokenClass::Punctuation,
        wit::TokenClass::Operator => TokenClass::Operator,
        wit::TokenClass::TypeName => TokenClass::TypeName,
        wit::TokenClass::Function => TokenClass::Function,
        wit::TokenClass::Invalid => TokenClass::Invalid,
    }
}

/// A failure the plugin returned, held to what the window can show safely.
///
/// Its code is kept only in a plugin's own namespace (`<slug>.<condition>`: a dot, lower-case
/// ASCII), so a plugin can neither claim one of the host's codes - a fake `plugin_crashed`
/// would count towards its own suspension - nor carry text in a code. Parameters are cut and
/// their control and direction characters spelled out.
pub(crate) fn from_wit_failure(failure: types::Failure) -> Failure {
    let kind = match failure.kind {
        types::FailureKind::NotFound => FailureKind::NotFound,
        types::FailureKind::Exists => FailureKind::Exists,
        types::FailureKind::AccessDenied => FailureKind::AccessDenied,
        types::FailureKind::NotSupported => FailureKind::NotSupported,
        types::FailureKind::BadData => FailureKind::BadData,
        types::FailureKind::WrongPassword => FailureKind::WrongPassword,
        types::FailureKind::Cancelled => FailureKind::Cancelled,
        types::FailureKind::Io => FailureKind::Io,
        types::FailureKind::Other => FailureKind::Other,
    };
    let mut native = Failure::new(kind);
    native.code = failure.code.filter(|code| plugin_code(code));
    native.params = failure
        .params
        .into_iter()
        .filter(|(key, _)| {
            !key.is_empty()
                && key.len() <= MAX_PARAM_KEY_CHARS
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .take(MAX_PARAMS)
        .map(|(key, value)| (key, readable(&value, MAX_PARAM_VALUE_CHARS)))
        .collect();
    native
}

fn plugin_code(code: &str) -> bool {
    code.len() <= MAX_CODE_CHARS
        && code.contains('.')
        && code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}
