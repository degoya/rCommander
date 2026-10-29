//! The bearer token of the HTTP transport: 32 random bytes, generated once and kept in the
//! setting `mcp.token`.

use std::fmt;

use rc_db::Db;

/// The settings key.
pub const TOKEN_KEY: &str = "mcp.token";

/// The token. Its `Debug` is redacted and it has no `Display`, so it cannot reach a log by
/// accident; [`McpToken::expose`] is the one way to the text.
#[derive(Clone, PartialEq, Eq)]
pub struct McpToken(String);

impl fmt::Debug for McpToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("McpToken([redacted])")
    }
}

impl McpToken {
    /// A new random token: 32 bytes as 64 hex digits.
    #[must_use]
    pub fn generate() -> Self {
        Self(crate::confirm::random_hex(32))
    }

    /// A token from its text (the instance file, a test).
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The token's text, for the `Authorization` header and the instance file.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether `offered` is this token, compared in constant time.
    #[must_use]
    pub fn matches(&self, offered: &str) -> bool {
        let (ours, theirs) = (self.0.as_bytes(), offered.as_bytes());
        ours.len() == theirs.len()
            && ours
                .iter()
                .zip(theirs)
                .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    }

    /// The answer to the health probe's challenge `nonce` on `port`: BLAKE3 keyed with a key
    /// derived from the token, over the port and the nonce. It proves the token without sending
    /// it, for this port and this nonce only, so a server holding another port cannot pass on the
    /// answer of the real one (RC-080-12). [`blake3::Hash`] compares in constant time.
    #[must_use]
    pub(crate) fn proof(&self, port: u16, nonce: &str) -> blake3::Hash {
        let key = blake3::derive_key("rcommander mcp health proof v1", self.0.as_bytes());
        let mut hasher = blake3::Hasher::new_keyed(&key);
        hasher.update(&port.to_be_bytes());
        hasher.update(nonce.as_bytes());
        hasher.finalize()
    }

    /// The stored token, generated and stored on first use.
    ///
    /// # Errors
    ///
    /// The database read or write fails.
    pub async fn load_or_create(db: &Db) -> rc_db::Result<Self> {
        if let Some(text) = db.get::<String>(TOKEN_KEY).await?
            && !text.is_empty()
        {
            return Ok(Self(text));
        }
        let token = Self::generate();
        db.set(TOKEN_KEY, &token.0).await?;
        Ok(token)
    }

    /// A new token, stored in place of the old one: from the settings dialog, whose server
    /// restarts with it, so the old token stops working at once.
    ///
    /// # Errors
    ///
    /// The database write fails; the old token stays then.
    pub async fn rotate(db: &Db) -> rc_db::Result<Self> {
        let token = Self::generate();
        db.set(TOKEN_KEY, &token.0).await?;
        Ok(token)
    }
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
