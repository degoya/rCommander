use super::*;

#[test]
fn every_template_makes_a_valid_script_with_a_unique_id() {
    let mut ids: Vec<&str> = TEMPLATES.iter().map(|template| template.id).collect();
    assert_eq!(ids, ["sudo_i", "su_root", "second_password"]);
    ids.dedup();
    assert_eq!(ids.len(), TEMPLATES.len());
    for template in TEMPLATES {
        let script = template.script();
        script.validate().expect(template.id);
        assert!(!script.steps.is_empty(), "{}", template.id);
    }
}

#[test]
fn the_sudo_template_is_the_job_s() {
    let script = TEMPLATES[0].script();
    let json = serde_json::to_value(&script.steps).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!([
            { "step": "send_text", "text": "sudo -i", "newline": true },
            { "step": "wait_for", "pattern": "(?i)password.*:" },
            { "step": "send_secret", "source": { "kind": "connection_password" }, "newline": true },
            { "step": "wait_for", "pattern": "#\\s*$" },
        ])
    );
}
