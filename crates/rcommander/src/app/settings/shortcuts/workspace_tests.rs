use super::*;

fn listed() -> Vec<WorkspaceRow> {
    vec![
        WorkspaceRow {
            id: 3,
            name: "Servers".to_owned(),
            active: false,
        },
        WorkspaceRow {
            id: 7,
            name: "Web".to_owned(),
            active: true,
        },
    ]
}

/// The panel lists every workspace as the palette names it, by its key's name; the active one is
/// offered first.
#[test]
fn the_panel_offers_every_workspace() {
    let options = options(&listed());
    assert_eq!(
        options,
        [
            WorkspaceOption {
                name: "workspace:3".to_owned(),
                label: row_label("Servers"),
            },
            WorkspaceOption {
                name: "workspace:7".to_owned(),
                label: row_label("Web"),
            },
        ]
    );
    assert_eq!(options[1].to_string(), row_label("Web"));
    assert_eq!(first(&listed()).as_deref(), Some("workspace:7"));
    let mut none_active = listed();
    none_active[1].active = false;
    assert_eq!(first(&none_active).as_deref(), Some("workspace:3"));
    assert_eq!(first(&[]), None);
}
