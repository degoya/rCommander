use serde_json::{Value, json};

use super::*;

fn opened(page: Option<Value>, tab: Option<Value>) -> Result<Opening> {
    check(RawOpening::Settings, page.as_ref(), tab.as_ref())
}

fn refused(page: Option<Value>, tab: Option<Value>, says: &str) {
    let error = opened(page, tab).expect_err("refused");
    assert!(
        error.to_string().contains(says),
        "{error} does not say {says:?}"
    );
}

#[test]
fn a_page_and_a_tab_of_it_are_read() {
    assert_eq!(
        opened(None, None).expect("as they open"),
        Opening::Settings {
            page: None,
            tab: None
        }
    );
    assert_eq!(
        opened(Some(json!("appearance")), Some(json!("themes"))).expect("the Themes tab"),
        Opening::Settings {
            page: Some(Page::Appearance),
            tab: Some(Tab::Themes)
        }
    );
    assert_eq!(
        opened(Some(json!("about")), None).expect("a page without tabs"),
        Opening::Settings {
            page: Some(Page::About),
            tab: None
        }
    );
}

#[test]
fn an_unknown_page_or_tab_is_refused() {
    refused(Some(json!("colours")), None, "no page 'colours'");
    refused(
        Some(json!("appearance")),
        Some(json!("palette")),
        "no tab 'palette'",
    );
    refused(Some(json!(3)), None, "a page of the settings is a name");
    refused(None, Some(json!("themes")), "needs its page");
    refused(
        Some(json!("keyboard")),
        Some(json!("themes")),
        "has no tab Themes",
    );
}

/// Every page and every tab has its name, once, and each tab is of a page.
#[test]
fn every_page_and_tab_has_one_name() {
    for page in Page::ALL {
        let names = PAGES.iter().filter(|(_, known)| *known == page).count();
        assert_eq!(names, 1, "{page:?}");
    }
    for (name, tab) in TABS {
        assert!(
            PAGES.iter().any(|(_, page)| on_page(*page, tab)),
            "{name} is on no page"
        );
        assert_eq!(TABS.iter().filter(|(_, known)| *known == tab).count(), 1);
    }
}

#[test]
fn the_workspaces_window_opens_without_a_page_or_a_tab() {
    assert_eq!(
        check(RawOpening::Workspaces, None, None).expect("the workspaces' window"),
        Opening::Workspaces
    );
    for (page, tab) in [
        (Some(json!("appearance")), None),
        (None, Some(json!("themes"))),
    ] {
        let error = check(RawOpening::Workspaces, page.as_ref(), tab.as_ref())
            .expect_err("no pages or tabs");
        assert!(error.to_string().contains("no pages or tabs"), "{error}");
    }
}
