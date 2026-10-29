use rc_engine::Outcome;
use rmcp::model::CallToolResult;
use serde_json::Value;

use crate::test_support::Fixture;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn app_about_answers_what_the_about_page_shows() {
    let built = Fixture::new().build().await;
    let server = built.server();
    let result = server.app_about().await.expect("answer");
    assert_ne!(result.is_error, Some(true));
    let answer = body(&result);
    let expected =
        serde_json::to_value(Outcome::About(rc_engine::about::about())).expect("serializes");
    assert_eq!(answer, expected);
    assert_eq!(answer["kind"], "about");
    assert_eq!(answer["version"], env!("CARGO_PKG_VERSION"));
    assert!(answer.get("licenses").is_none(), "{answer}");
}
