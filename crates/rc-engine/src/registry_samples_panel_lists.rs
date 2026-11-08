//! The samples of the panels' smaller Total Commander functions (RC-100-92).

use crate::{Command, FileFilter, PanelId, TabId};

/// One sample of each command of RC-100-92.
pub(super) fn panel_lists() -> Vec<Command> {
    vec![
        Command::SetFileFilter {
            panel: Some(PanelId::Left),
            filter: Some(FileFilter::Mask("*.txt".into())),
        },
        Command::ListFileFilters,
        Command::SaveFileFilter {
            name: "Text".into(),
            mask: "*.txt;*.md".into(),
        },
        Command::RenameFileFilter {
            name: "Text".into(),
            new_name: "Docs".into(),
        },
        Command::DeleteFileFilter {
            name: "Docs".into(),
        },
        Command::ToggleIgnoreList {
            enabled: Some(true),
        },
        Command::SaveSelection { panel: None },
        Command::SelectionToFile {
            panel: None,
            path: "selection.txt".into(),
            overwrite: false,
        },
        Command::SelectionFromFile {
            panel: Some(PanelId::Right),
            path: "selection.txt".into(),
        },
        Command::SelectionFromText {
            panel: None,
            text: "a.txt\nb.txt".into(),
        },
        Command::LoadList {
            panel: None,
            path: "list.txt".into(),
        },
        Command::RenameTab {
            panel: None,
            tab: Some(TabId(1)),
            title: Some("Sources".into()),
        },
        Command::CloseAllTabs {
            panel: Some(PanelId::Left),
            include_locked: true,
        },
        Command::HistoryOpenFrequent {
            panel: None,
            index: 0,
        },
    ]
}
