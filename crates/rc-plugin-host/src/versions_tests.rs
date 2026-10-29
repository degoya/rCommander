use super::*;

fn choice(active: Option<&str>, staged: Option<&str>) -> VersionChoice {
    VersionChoice {
        active: active.map(str::to_owned),
        staged: staged.map(str::to_owned),
    }
}

#[test]
fn without_a_choice_the_newest_version_wins() {
    assert_eq!(
        default_version(&["1.0.0", "1.10.0", "1.2.0"], None),
        Some("1.10.0")
    );
}

#[test]
fn the_chosen_active_version_wins_while_it_is_loaded() {
    let rolled_back = choice(Some("1.0.0"), None);
    assert_eq!(
        default_version(&["1.0.0", "2.0.0"], Some(&rolled_back)),
        Some("1.0.0")
    );
    assert_eq!(
        default_version(&["2.0.0", "3.0.0"], Some(&rolled_back)),
        Some("3.0.0")
    );
}

#[test]
fn a_staged_version_is_never_the_default() {
    let staged = choice(None, Some("2.0.0"));
    assert_eq!(
        default_version(&["1.0.0", "2.0.0"], Some(&staged)),
        Some("1.0.0")
    );
    assert_eq!(default_version(&["2.0.0"], Some(&staged)), None);
    let both = choice(Some("2.0.0"), Some("2.0.0"));
    assert_eq!(
        default_version(&["1.0.0", "2.0.0"], Some(&both)),
        Some("1.0.0")
    );
}

#[test]
fn every_version_gets_exactly_one_role() {
    let pairs = [
        ("a".to_owned(), "1.0.0".to_owned()),
        ("a".to_owned(), "2.0.0".to_owned()),
        ("a".to_owned(), "3.0.0".to_owned()),
        ("b".to_owned(), "1.0.0".to_owned()),
    ];
    let choices: VersionChoices = [("a".to_owned(), choice(None, Some("3.0.0")))].into();
    let roles = roles_by_id(&pairs, &choices);
    let role = |id: &str, version: &str| roles[&(id.to_owned(), version.to_owned())];
    assert_eq!(role("a", "2.0.0"), VersionRole::Default);
    assert_eq!(role("a", "1.0.0"), VersionRole::Retained);
    assert_eq!(role("a", "3.0.0"), VersionRole::Staged);
    assert_eq!(role("b", "1.0.0"), VersionRole::Default);
}

#[test]
fn a_rollback_goes_to_the_newest_version_below_the_current_one() {
    let loaded = ["1.0.0", "1.2.0", "2.0.0", "2.1.0"];
    assert_eq!(previous_version(&loaded, "2.1.0"), Some("2.0.0"));
    assert_eq!(previous_version(&loaded, "2.0.0"), Some("1.2.0"));
    assert_eq!(previous_version(&loaded, "1.0.0"), None);
}
