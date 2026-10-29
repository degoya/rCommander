//! Finding credentials in strings: `rc_db::redact`, shared with the error log so one set of
//! rules masks both what the database stores and what MCP answers.

pub(crate) use rc_db::redact::{PLACEHOLDER, is_secret_name, redact_text, redact_url};
