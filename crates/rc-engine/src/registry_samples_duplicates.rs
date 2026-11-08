//! The samples of the duplicate search and the disk-usage analysis (RC-100-98).

use crate::{
    Command, DuplicatesRequest, PanelId, SearchQuery,
    search::{DatePick, DuplicateQuery, DuplicateRule, DuplicateSelection, Keep},
};

/// One sample of each command of RC-100-98.
pub(super) fn duplicates() -> Vec<Command> {
    vec![
        Command::FindDuplicates(DuplicatesRequest {
            panel: Some(PanelId::Left),
            paths: vec!["photos".into()],
            query: SearchQuery {
                name: "*.jpg".into(),
                ..SearchQuery::default()
            },
            duplicates: DuplicateQuery {
                name: false,
                size: true,
                content: true,
            },
            limit: Some(10),
            cursor: None,
        }),
        Command::SelectDuplicates {
            panel: None,
            spec: DuplicateSelection {
                rule: DuplicateRule::Date {
                    pick: DatePick::AllOlder,
                },
                leave_one: true,
                keep: vec![Keep::Newest, Keep::Largest, Keep::First],
            },
        },
        Command::DiskUsage {
            panel: Some(PanelId::Right),
            path: Some("data".into()),
            depth: Some(2),
            limit: Some(50),
        },
        Command::DiskUsageDialog { panel: None },
    ]
}
