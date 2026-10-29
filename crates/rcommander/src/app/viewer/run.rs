//! A lister's requests run as tasks whose answers come back to it: the reads of its file
//! (`load.rs`), the Markdown preview, and the questions to a document's source (RC-100-10) - its
//! picture and its text among them (RC-100-29). Split from `viewer.rs` to keep it under 500
//! lines.

use std::sync::Arc;

use iced::Task;

use super::{
    ViewerMessage,
    document::DocumentMessage,
    lister::{Lister, Request},
    load, wrap,
};
use crate::app::{App, Effect, Message};

impl App {
    /// The reads and questions a lister asked for, as tasks whose answers come back to it.
    pub(super) fn run(&self, id: u64, requests: Vec<Request>) -> Vec<Effect> {
        let Some(lister) = self.viewers.lister(id) else {
            return Vec::new();
        };
        let (asks, reads): (Vec<Request>, Vec<Request>) =
            requests.into_iter().partition(|request| match request {
                Request::Tokens(_) | Request::Children(_) | Request::Text => true,
                Request::Picture { .. } => lister.document_picture(),
                _ => false,
            });
        let mut effects: Vec<Effect> = asks
            .into_iter()
            .filter_map(|ask| ask_source(lister, id, ask))
            .map(Effect::Task)
            .collect();
        let Some(opened) = &lister.file else {
            return effects;
        };
        let (layout, top) = (lister.layout(), lister.top);
        effects.extend(reads.into_iter().map(|request| {
            let file = Arc::clone(&opened.file);
            Effect::Task(match request {
                Request::Page {
                    generation,
                    seek,
                    mark,
                } => Task::perform(
                    load::page((file, opened.last_page.clone()), layout, top, seek, mark),
                    move |page| wrap(ViewerMessage::Page(id, generation, page)),
                ),
                Request::Picture { generation } => match opened.image {
                    Some(format) => {
                        Task::perform(load::picture(file, format, self.db.clone()), move |shown| {
                            wrap(ViewerMessage::Picture(id, generation, shown))
                        })
                    }
                    None => Task::done(wrap(ViewerMessage::Picture(
                        id,
                        generation,
                        Err("image_broken"),
                    ))),
                },
                Request::Search {
                    needle,
                    from,
                    direction,
                    cancel,
                } => Task::run(
                    load::search(file, needle, from, direction, cancel),
                    move |news| wrap(ViewerMessage::Found(id, news)),
                ),
                // A plugin's document is opened again by its plugin, from the file itself.
                Request::Reload if lister.document.is_some() => {
                    self.open_task(id, lister.path.clone())
                }
                Request::Reload => {
                    let opening =
                        load::open(self.engine.clone(), lister.path.clone(), self.db.clone());
                    Task::perform(opening, move |opened| {
                        wrap(ViewerMessage::Reloaded(id, opened))
                    })
                }
                preview => self.preview_task(lister, file, preview),
            })
        }));
        effects
    }
}

/// A question to the source of lister `id`'s document; its answer names the document asked.
fn ask_source(lister: &Lister, id: u64, ask: Request) -> Option<Task<Message>> {
    let document = lister.document.as_ref()?;
    let (source, serial) = (document.source(), document.serial());
    Some(match ask {
        Request::Tokens(starts) => {
            let asking = source.tokens(starts.clone());
            Task::perform(asking, move |answer| {
                wrap(ViewerMessage::Document(
                    id,
                    DocumentMessage::Tokens(serial, starts, answer),
                ))
            })
        }
        Request::Children(node) => Task::perform(source.children(node), move |answer| {
            wrap(ViewerMessage::Document(
                id,
                DocumentMessage::Children(serial, node, answer),
            ))
        }),
        Request::Text => Task::perform(source.text(), move |answer| {
            wrap(ViewerMessage::Document(
                id,
                DocumentMessage::Text(serial, answer),
            ))
        }),
        Request::Picture { .. } => Task::perform(source.picture(), move |answer| {
            wrap(ViewerMessage::Document(
                id,
                DocumentMessage::Picture(serial, answer),
            ))
        }),
        _ => return None,
    })
}
