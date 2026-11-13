//! A search of the dialog as an iced task: started off the window's thread, its events handed
//! to the dialog in batches of what is ready, so a fast search costs few redraws. A search whose
//! events end without its `Finished` - its task died - is closed here with one, as stopped, so
//! the dialog does not wait on it for good (RC-110-29).

use std::sync::Arc;

use iced::{
    Task,
    futures::{StreamExt, stream},
};
use rc_engine::{
    Engine, PanelId, SearchEvent, SearchHandle, SearchQuery, SearchStats,
    search::everything::Everything,
};

use super::{SearchMsg, Update};
use crate::app::Message;

/// Events handed over in one message at most.
const BATCH: usize = 512;

enum Step {
    Start {
        engine: Engine,
        panel: PanelId,
        paths: Vec<String>,
        query: Box<SearchQuery>,
        everything: Option<Arc<dyn Everything>>,
    },
    /// The search and the counts of its last progress.
    Running(SearchHandle, SearchStats),
    Done,
}

/// What the dialog hears after the search said `event` - nothing more (`None`): its task ended
/// without a `Finished` - at the counts `stats`, and the counts the search goes on with, `None`
/// once this was its last word.
fn heard(event: Option<SearchEvent>, stats: SearchStats) -> (SearchEvent, Option<SearchStats>) {
    match event {
        None => {
            let cancelled = true;
            (SearchEvent::Finished { stats, cancelled }, None)
        }
        Some(event) => {
            let goes_on = match &event {
                SearchEvent::Finished { .. } => None,
                SearchEvent::Progress { stats, .. } => Some(*stats),
                _ => Some(stats),
            };
            (event, goes_on)
        }
    }
}

/// Searches `paths` (the panel's folder when empty) for `query`, the candidates asked of
/// `everything` when given; every batch carries `generation`. The search stops when the dialog
/// cancels it or the task is dropped.
pub fn search(
    engine: Engine,
    panel: PanelId,
    paths: Vec<String>,
    query: SearchQuery,
    everything: Option<Arc<dyn Everything>>,
    generation: u64,
) -> Task<Message> {
    let start = Step::Start {
        engine,
        panel,
        paths,
        query: Box::new(query),
        everything,
    };
    let updates = stream::unfold(start, |step| async move {
        match step {
            Step::Start {
                engine,
                panel,
                paths,
                query,
                everything,
            } => match engine
                .start_search_with(Some(panel), &paths, &query, everything)
                .await
            {
                Ok(handle) => {
                    let started = Update::Started(handle.canceller());
                    Some((started, Step::Running(handle, SearchStats::default())))
                }
                Err(error) => Some((Update::Failed(error.code()), Step::Done)),
            },
            Step::Running(mut handle, stats) => {
                let (event, goes_on) = heard(handle.next().await, stats);
                let step = goes_on.map_or(Step::Done, |stats| Step::Running(handle, stats));
                Some((Update::Event(Box::new(event)), step))
            }
            Step::Done => None,
        }
    });
    Task::run(updates.ready_chunks(BATCH), move |batch| {
        Message::Search(SearchMsg::Updates(generation, batch))
    })
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
