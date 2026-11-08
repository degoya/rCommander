use super::*;

fn every_refusal() -> Vec<TabsError> {
    vec![
        TabsError::TabNotFound { tab: TabId(7) },
        TabsError::LastTab,
        TabsError::HistoryEnd,
        TabsError::HistoryOutOfRange { index: 3, len: 2 },
        TabsError::HotlistNotFound { id: 1 },
        TabsError::HotlistNotAFolder { id: 1 },
        TabsError::HotlistNotAnEntry { id: 1 },
        TabsError::HotlistCycle,
        TabsError::HotlistTooDeep { max: 8 },
        TabsError::HotlistBadName { max: 200 },
        TabsError::HotlistBadPath,
        TabsError::TabTitleInvalid { reason: "x" },
        TabsError::SavedFilterInvalid { reason: "x" },
        TabsError::SavedFilterNotFound { name: "x".into() },
        TabsError::ListFileTooLarge {
            max_bytes: 1,
            max_lines: 1,
        },
        TabsError::ListFileNotLocal,
        TabsError::ListFileExists,
        TabsError::ListFileEmpty,
    ]
}

#[test]
fn every_code_is_listed_once() {
    let codes: Vec<&str> = every_refusal().iter().map(TabsError::code).collect();
    assert_eq!(codes, TabsError::ALL_CODES);
}

#[test]
fn an_engine_error_keeps_the_code() {
    let error = EngineError::from(TabsError::LastTab);
    assert_eq!(error.code(), "last_tab");
    let error = EngineError::from(HotlistError::Cycle);
    assert_eq!(error.code(), "hotlist_cycle");
}
