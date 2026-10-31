//! The theme tools over an in-process pipe: a list, a choice, and no way to edit a theme.

use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};

use crate::test_support::Fixture;

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    arguments: Value,
) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

fn selected(answer: &Value) -> Vec<String> {
    answer["themes"]
        .as_array()
        .map(|themes| {
            themes
                .iter()
                .filter(|theme| theme["selected"] == true)
                .filter_map(|theme| theme["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn themes_are_listed_and_chosen_per_mode() {
    let built = Fixture::new().build().await;
    let client = built.client().await;
    let list = call(&client, "theme_list", json!({})).await;
    assert_eq!(list["kind"], "themes", "{list}");
    assert_eq!(selected(&list), ["fluent-light", "fluent-dark"]);

    let chosen = call(
        &client,
        "theme_select",
        json!({ "mode": "dark", "id": "solarized-dark" }),
    )
    .await;
    assert_eq!(
        selected(&chosen),
        ["fluent-light", "solarized-dark"],
        "{chosen}"
    );

    let wrong = call(
        &client,
        "theme_select",
        json!({ "mode": "light", "id": "solarized-dark" }),
    )
    .await;
    assert_eq!(wrong["code"], "theme_wrong_mode", "{wrong}");
}

/// Saving, deleting, importing and exporting a theme are the window's.
#[test]
fn no_tool_edits_a_theme() {
    let names = crate::RcMcpServer::tool_names();
    assert!(names.contains(&"theme_list".to_owned()));
    assert!(names.contains(&"theme_select".to_owned()));
    for name in ["theme_save", "theme_delete", "theme_import", "theme_export"] {
        assert!(!names.contains(&name.to_owned()), "{name}");
    }
}
