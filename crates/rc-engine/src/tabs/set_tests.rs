use std::path::PathBuf;

use super::*;

fn state(path: &str) -> PanelState {
    PanelState::showing(VfsPath::local(PathBuf::from(path)), Vec::new())
}

fn tabs() -> Tabs {
    Tabs::new(&state("/left"), &state("/right"))
}

fn ids(tabs: &Tabs, panel: PanelId) -> Vec<u64> {
    tabs.set(panel).tabs().iter().map(|tab| tab.id.0).collect()
}

#[test]
fn each_panel_starts_with_one_tab_and_ids_are_unique() {
    let mut tabs = tabs();
    assert_eq!(ids(&tabs, PanelId::Left), [1]);
    assert_eq!(ids(&tabs, PanelId::Right), [2]);
    let tab = tabs.new_tab(VfsPath::local("/x"), SortOrder::default(), ListMode::Full);
    assert_eq!(tab.id, TabId(3));
}

#[test]
fn a_new_tab_goes_right_of_the_active_one() {
    let mut tabs = tabs();
    for path in ["/a", "/b"] {
        let tab = tabs.new_tab(VfsPath::local(path), SortOrder::default(), ListMode::Full);
        tabs.insert_active(PanelId::Left, tab);
    }
    tabs.activate(PanelId::Left, 0);
    let tab = tabs.new_tab(VfsPath::local("/c"), SortOrder::default(), ListMode::Full);
    assert_eq!(tabs.insert_active(PanelId::Left, tab), 1);
    assert_eq!(ids(&tabs, PanelId::Left), [1, 5, 3, 4]);
}

#[test]
fn removing_keeps_the_last_tab_and_the_active_one_in_view() {
    let mut tabs = tabs();
    assert_eq!(tabs.remove(PanelId::Left, 0), None, "the last tab stays");
    for path in ["/a", "/b"] {
        let tab = tabs.new_tab(VfsPath::local(path), SortOrder::default(), ListMode::Full);
        tabs.insert_active(PanelId::Left, tab);
    }
    // [1, 3, 4*]: removing a tab left of the active one keeps the active one.
    assert!(tabs.remove(PanelId::Left, 0).is_some());
    assert_eq!(tabs.set(PanelId::Left).active_tab().id, TabId(4));
    // [3, 4*]: removing the active last tab shows its left neighbour.
    assert!(tabs.remove(PanelId::Left, 1).is_some());
    assert_eq!(tabs.set(PanelId::Left).active_tab().id, TabId(3));
}

#[test]
fn a_moved_tab_takes_its_place_and_the_active_one_stays_active() {
    let mut tabs = tabs();
    for path in ["/a", "/b"] {
        let tab = tabs.new_tab(VfsPath::local(path), SortOrder::default(), ListMode::Full);
        tabs.insert_active(PanelId::Left, tab);
    }
    // [1, 3, 4*]: the first to the end, past the active one.
    assert_eq!(tabs.move_tab(PanelId::Left, TabId(1), 2), Some(2));
    assert_eq!(ids(&tabs, PanelId::Left), [3, 4, 1]);
    assert_eq!(tabs.set(PanelId::Left).active_tab().id, TabId(4));
    // The active one to the front; beyond the row is its last place.
    assert_eq!(tabs.move_tab(PanelId::Left, TabId(4), 0), Some(0));
    assert_eq!(tabs.move_tab(PanelId::Left, TabId(3), 9), Some(2));
    assert_eq!(ids(&tabs, PanelId::Left), [4, 1, 3]);
    assert_eq!(tabs.set(PanelId::Left).active(), 0);
    // Another panel's tab is not this panel's.
    assert_eq!(tabs.move_tab(PanelId::Left, TabId(2), 0), None);
    assert_eq!(ids(&tabs, PanelId::Right), [2]);
}

#[test]
fn sync_copies_the_live_state_into_the_active_tab() {
    let mut tabs = tabs();
    let mut live = state("/elsewhere");
    live.mode = ListMode::Brief;
    tabs.sync(PanelId::Left, &live);
    let tab = tabs.set(PanelId::Left).active_tab();
    assert_eq!(tab.path, VfsPath::local("/elsewhere"));
    assert_eq!(tab.mode, ListMode::Brief);
}

#[test]
fn views_show_the_live_folder_of_the_active_tab() {
    let mut tabs = tabs();
    let tab = tabs.new_tab(VfsPath::local("/b"), SortOrder::default(), ListMode::Full);
    tabs.push(PanelId::Left, tab);
    let views = tabs.views(PanelId::Left, &state("/now"));
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].title, "now");
    assert!(views[0].active && !views[1].active);
    assert_eq!(views[1].title, "b");
}

#[test]
fn a_copy_is_unlocked_under_a_new_id() {
    let mut tabs = tabs();
    let mut tab = tabs.set(PanelId::Left).active_tab().clone();
    tab.locked = true;
    let copy = tabs.copy_tab(&tab);
    assert_ne!(copy.id, tab.id);
    assert!(!copy.locked);
    assert_eq!(copy.path, tab.path);
}

#[test]
fn swapping_exchanges_the_rows() {
    let mut tabs = tabs();
    tabs.swap();
    assert_eq!(ids(&tabs, PanelId::Left), [2]);
    assert_eq!(ids(&tabs, PanelId::Right), [1]);
}

#[test]
fn a_set_read_back_is_never_empty() {
    assert_eq!(TabSet::from_tabs(Vec::new(), 0), None);
    let mut tabs = tabs();
    let tab = tabs.new_tab(VfsPath::local("/x"), SortOrder::default(), ListMode::Full);
    let set = TabSet::from_tabs(vec![tab], 5).expect("one tab");
    assert_eq!(set.active(), 0);
    tabs.replace(PanelId::Right, set);
    assert_eq!(ids(&tabs, PanelId::Right), [4], "new ids");
}
