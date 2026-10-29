//! A search of the dialog as an iced task: started off the window's thread, its events handed
//! to the dialog in batches of what is ready, so a fast search costs few redraws.

use iced::{
    Task,
    futures::{StreamExt, stream},
};
use rc_engine::{Engine, PanelId, SearchHandle, SearchQuery};

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
    },
    Running(SearchHandle),
    Done,
}

/// Searches `paths` (the panel's folder when empty) for `query`; every batch carries
/// `generation`. The search stops when the dialog cancels it or the task is dropped.
pub fn search(
    engine: Engine,
    panel: PanelId,
    paths: Vec<String>,
    query: SearchQuery,
    generation: u64,
) -> Task<Message> {
    let start = Step::Start {
        engine,
        panel,
        paths,
        query: Box::new(query),
    };
    let updates = stream::unfold(start, |step| async move {
        match step {
            Step::Start {
                engine,
                panel,
                paths,
                query,
            } => match engine.start_search(Some(panel), &paths, &query).await {
                Ok(handle) => Some((Update::Started(handle.canceller()), Step::Running(handle))),
                Err(error) => Some((Update::Failed(error.code()), Step::Done)),
            },
            Step::Running(mut handle) => handle
                .next()
                .await
                .map(|event| (Update::Event(event), Step::Running(handle))),
            Step::Done => None,
        }
    });
    Task::run(updates.ready_chunks(BATCH), move |batch| {
        Message::Search(SearchMsg::Updates(generation, batch))
    })
}
