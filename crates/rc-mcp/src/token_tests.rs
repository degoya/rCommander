use rc_db::Db;

use super::{McpToken, TOKEN_KEY};

#[tokio::test]
async fn the_token_is_generated_once_and_kept() {
    let db = Db::open_in_memory().await.expect("db");
    let first = McpToken::load_or_create(&db).await.expect("token");
    assert_eq!(first.expose().len(), 64);
    assert!(first.expose().bytes().all(|byte| byte.is_ascii_hexdigit()));
    let again = McpToken::load_or_create(&db).await.expect("token");
    assert_eq!(first, again);
    let stored: Option<String> = db.get(TOKEN_KEY).await.expect("reads");
    assert_eq!(stored.as_deref(), Some(first.expose()));
    assert_ne!(McpToken::generate(), first);
}

#[test]
fn the_token_never_shows_in_debug_and_compares_exactly() {
    let token = McpToken::from_text("abc123");
    assert!(!format!("{token:?}").contains("abc123"));
    assert!(token.matches("abc123"));
    assert!(!token.matches("abc124"));
    assert!(!token.matches("abc12"));
    assert!(!token.matches(""));
}

/// RC-080-12 (M9): the probe's proof depends on the token, the port and the nonce, and shows
/// none of the token.
#[test]
fn the_proof_binds_token_port_and_nonce() {
    let token = McpToken::from_text("abc123");
    let proof = token.proof(4000, "00ff");
    assert_eq!(proof, token.proof(4000, "00ff"));
    assert_ne!(proof, token.proof(4001, "00ff"));
    assert_ne!(proof, token.proof(4000, "00fe"));
    assert_ne!(proof, McpToken::from_text("abc124").proof(4000, "00ff"));
    assert!(!proof.to_hex().contains("abc123"));
}
