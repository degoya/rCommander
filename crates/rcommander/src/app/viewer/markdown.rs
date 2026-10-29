//! The Markdown preview of Lister and Quick View (RC-020-09): a `.md` or `.markdown` file shows
//! formatted - headings, emphasis, lists, quotes, code blocks, tables, links - and `5` turns it
//! to text and back, as Lister's `5` shows an HTML file formatted in Total Commander. While
//! `viewer.markdown_preview` is off such a file opens as text, and `5` still shows the preview.
//!
//! The file is foreign input. It is read whole only below [`PREVIEW_LIMIT`] (the hard limit of
//! `files_read_text`), larger files show as text and the status line says why; reading and
//! parsing run off the UI thread. Parsing and layout are iced's own `markdown` (pulldown-cmark);
//! before it, raw HTML is turned into text ([`html_as_text`]), because iced would drop it
//! silently - it is shown, never rendered. Nothing is fetched: an image shows its alt text
//! (`markdown_view.rs`), and a link opens only on a click through `links.rs`.

use std::{borrow::Cow, sync::Arc};

use iced::{
    Task,
    widget::{
        markdown::{self, Item},
        operation,
        scrollable::{AbsoluteOffset, RelativeOffset},
    },
};
use pulldown_cmark::{Event, Options, Parser};
use rc_db::Db;
use rc_engine::{
    VfsPath,
    settings::{MARKDOWN_PREVIEW_KEY, stored_bool},
    viewer::{Detected, FileWindow, ViewMode, encoding},
};

use super::{
    ViewerMessage,
    keys::{ListerKey, Scroll},
    links,
    lister::{Lister, Request},
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    i18n::{self, t},
};

/// The largest file the preview reads, as `files_read_text` at most does.
pub const PREVIEW_LIMIT: u64 = 4 * 1024 * 1024;
/// The error codes of the preview, each with its `error.<code>` text.
#[cfg(test)]
pub const ALL_CODES: &[&str] = &["markdown_too_large", "markdown_broken"];
/// Passes of [`html_as_text`] before a file counts as broken: one finds every HTML of a real
/// document, a second the rare HTML the first pass let surface.
const HTML_PASSES: usize = 4;

/// Whether `path` names a Markdown file, by its extension.
pub fn is_markdown(path: &VfsPath) -> bool {
    path.path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

/// A Markdown file's preview in a lister.
#[derive(Debug, Default)]
pub enum Markdown {
    /// Shown as text: the setting is off, the file too large, or no preview was asked for.
    #[default]
    Text,
    /// Being read and parsed; shown once ready while `show` holds.
    Asked {
        show: bool,
    },
    Ready {
        preview: Preview,
        shown: bool,
    },
}

/// A file parsed for the preview.
#[derive(Debug, Clone)]
pub struct Preview {
    pub items: Vec<Item>,
}

/// A message of the preview.
#[derive(Debug, Clone)]
pub enum PreviewMessage {
    /// A file read and parsed; `None` while the setting is off.
    Rendered(VfsPath, Result<Option<Preview>, &'static str>),
    /// A link clicked.
    Link(String),
    /// Whether the system took the link.
    LinkOpened(bool),
    /// The preview scrolled to this share of the way down (RC-020-12).
    Scrolled(f32),
}

/// The options iced's parser runs with: the HTML found here is the HTML it would see.
fn options() -> Options {
    Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// `source` with every `<` of raw HTML (blocks and inline) written as `&lt;`: the parser then
/// sees text where it saw HTML and shows it as such. Code spans and blocks keep their `<`. `None`
/// when HTML is still found after [`HTML_PASSES`] passes.
pub fn html_as_text(source: &str) -> Option<Cow<'_, str>> {
    let mut text = Cow::Borrowed(source);
    for _ in 0..HTML_PASSES {
        let mut cuts: Vec<usize> = Vec::new();
        for (event, range) in Parser::new_ext(&text, options()).into_offset_iter() {
            if let Event::Html(_) | Event::InlineHtml(_) = event {
                let start = range.start;
                cuts.extend(text[range].match_indices('<').map(|(at, _)| start + at));
            }
        }
        if cuts.is_empty() {
            return Some(text);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut escaped = String::with_capacity(text.len() + 3 * cuts.len());
        let mut from = 0;
        for at in cuts {
            escaped.push_str(&text[from..at]);
            escaped.push_str("&lt;");
            from = at + 1;
        }
        escaped.push_str(&text[from..]);
        text = Cow::Owned(escaped);
    }
    None
}

/// `bytes` of a file in `detected`'s encoding, parsed for the preview.
pub fn parse(bytes: &[u8], detected: Detected) -> Result<Preview, &'static str> {
    let start = usize::try_from(detected.bom_len)
        .unwrap_or(0)
        .min(bytes.len());
    let text = encoding::decode(&bytes[start..], detected.encoding);
    let text = html_as_text(&text).ok_or("markdown_broken")?;
    Ok(Preview {
        items: markdown::parse(&text).collect(),
    })
}

/// The preview of `file`, read and parsed off the UI thread; `None` while the setting is off,
/// unless `forced` by the key.
pub async fn render(
    file: Arc<FileWindow>,
    detected: Detected,
    db: Option<Db>,
    forced: bool,
) -> Result<Option<Preview>, &'static str> {
    if !forced && !preview_on(db).await {
        return Ok(None);
    }
    if file.size() > PREVIEW_LIMIT {
        return Err("markdown_too_large");
    }
    let len = usize::try_from(file.size()).map_err(|_| "markdown_too_large")?;
    let bytes = file.read_uncached(0, len).await.map_err(|e| e.code())?;
    tokio::task::spawn_blocking(move || parse(&bytes, detected))
        .await
        .map_err(|_| "markdown_broken")?
        .map(Some)
}

/// `viewer.markdown_preview`; on without a database, as its default is.
async fn preview_on(db: Option<Db>) -> bool {
    let Some(db) = db else {
        return true;
    };
    stored_bool(&db, MARKDOWN_PREVIEW_KEY).await
}

/// The id of lister `id`'s preview, to scroll it by key.
pub fn scroll_id(id: u64) -> iced::widget::Id {
    iced::widget::Id::from(format!("rcommander.viewer.markdown.{id}"))
}

impl Lister {
    /// The parsed preview, while it is shown.
    pub fn preview(&self) -> Option<&[Item]> {
        match &self.markdown {
            Some(Markdown::Ready {
                preview,
                shown: true,
            }) if self.mode == ViewMode::Text => Some(&preview.items),
            _ => None,
        }
    }

    /// Whether the preview is being made and is to show: the view waits for it, not the text.
    pub fn preview_waiting(&self) -> bool {
        self.mode == ViewMode::Text && matches!(self.markdown, Some(Markdown::Asked { show: true }))
    }

    /// The file is open: a Markdown file in the text mode asks for its preview.
    pub(super) fn ask_preview(&mut self) -> Vec<Request> {
        match &mut self.markdown {
            Some(markdown) if self.mode == ViewMode::Text => {
                *markdown = Markdown::Asked { show: true };
                vec![Request::Render { forced: false }]
            }
            _ => Vec::new(),
        }
    }

    /// The file was read again (F2): a preview there is made again.
    pub(super) fn preview_reloaded(&mut self) -> Vec<Request> {
        match &mut self.markdown {
            Some(markdown @ Markdown::Ready { .. }) => {
                let show = matches!(markdown, Markdown::Ready { shown: true, .. });
                *markdown = Markdown::Asked { show };
                vec![Request::Render { forced: true }]
            }
            _ => Vec::new(),
        }
    }

    /// The preview's answer for `path`; one for another file, or not asked for, is dropped.
    pub(super) fn rendered(
        &mut self,
        path: &VfsPath,
        rendered: Result<Option<Preview>, &'static str>,
    ) {
        if *path != self.path {
            return;
        }
        let Some(markdown) = self.markdown.as_mut() else {
            return;
        };
        let Markdown::Asked { show } = *markdown else {
            return;
        };
        *markdown = match rendered {
            Ok(Some(preview)) => Markdown::Ready {
                preview,
                shown: show,
            },
            Ok(None) => Markdown::Text,
            Err(code) => {
                self.note = Some(i18n::error_text(code));
                Markdown::Text
            }
        };
    }

    /// A key while the file is a Markdown file: `5` turns the preview on or off, the scrolling
    /// keys scroll it while it shows. A mode, an encoding or a search shows the text first and
    /// goes on (`None`), as does every other key.
    pub(super) fn preview_key(&mut self, key: ListerKey) -> Option<Vec<Request>> {
        self.markdown.as_ref()?;
        match key {
            ListerKey::Preview => Some(self.toggle_preview()),
            ListerKey::Scroll(scroll) if self.preview().is_some() => {
                Some(vec![Request::PreviewScroll(scroll)])
            }
            ListerKey::Mode(_)
            | ListerKey::Encoding(_)
            | ListerKey::NextEncoding
            | ListerKey::Search
            | ListerKey::GoTo
            | ListerKey::FindNext
            | ListerKey::FindPrevious => {
                if let Some(Markdown::Asked { show } | Markdown::Ready { shown: show, .. }) =
                    &mut self.markdown
                {
                    *show = false;
                }
                None
            }
            _ => None,
        }
    }

    /// `5`: the preview on or off; from the binary, hex or image view it shows the preview.
    pub(super) fn toggle_preview(&mut self) -> Vec<Request> {
        let in_text = self.mode == ViewMode::Text;
        let mut requests = Vec::new();
        let show = match &mut self.markdown {
            None => return requests,
            Some(markdown @ Markdown::Text) => {
                *markdown = Markdown::Asked { show: true };
                requests.push(Request::Render { forced: true });
                true
            }
            Some(Markdown::Asked { show } | Markdown::Ready { shown: show, .. }) => {
                *show = !*show || !in_text;
                *show
            }
        };
        if show && !in_text {
            self.mode = ViewMode::Text;
            self.left = 0;
            requests.extend(self.show(super::load::Seek::At(self.top), None));
        }
        requests
    }
}

impl App {
    /// A message of lister `id`'s preview.
    pub(super) fn on_preview(&mut self, id: u64, message: PreviewMessage) -> Vec<Effect> {
        let Some(lister) = self.viewers.lister_mut(id) else {
            return Vec::new();
        };
        match message {
            PreviewMessage::Rendered(path, rendered) => lister.rendered(&path, rendered),
            PreviewMessage::Link(url) if links::opens(&url) => {
                lister.note = Some(t!("viewer.link_opening", url = links::shown(&url)));
                return vec![Effect::Task(Task::perform(
                    links::open(url),
                    move |opened| {
                        wrap(ViewerMessage::Preview(
                            id,
                            PreviewMessage::LinkOpened(opened),
                        ))
                    },
                ))];
            }
            PreviewMessage::Link(url) => {
                lister.note = Some(t!("viewer.link_refused", url = links::shown(&url)));
            }
            PreviewMessage::LinkOpened(true) => {}
            PreviewMessage::Scrolled(at) if at.is_finite() => {
                lister.preview_at = at.clamp(0.0, 1.0)
            }
            PreviewMessage::Scrolled(_) => {}
            PreviewMessage::LinkOpened(false) => {
                lister.note = Some(t!("viewer.link_failed").to_owned());
            }
        }
        Vec::new()
    }

    /// The task behind a request of the preview: reading and parsing, or a scroll.
    pub(super) fn preview_task(
        &self,
        lister: &Lister,
        file: Arc<FileWindow>,
        request: Request,
    ) -> Task<Message> {
        let id = lister.id;
        match request {
            Request::Render { forced } => {
                let path = lister.path.clone();
                let rendering = render(file, lister.detected, self.db.clone(), forced);
                Task::perform(rendering, move |rendered| {
                    wrap(ViewerMessage::Preview(
                        id,
                        PreviewMessage::Rendered(path, rendered),
                    ))
                })
            }
            Request::PreviewScroll(scroll) => {
                scroll_task(id, scroll, lister.rows, lister.font().line_height())
            }
            _ => Task::none(),
        }
    }
}

/// `scroll` in lister `id`'s preview: a `line` high, a page of `rows` of them, or to an end.
fn scroll_task(id: u64, scroll: Scroll, rows: usize, line: f32) -> Task<Message> {
    let page = rows.saturating_sub(1).max(1) as f32 * line;
    let by = |y: f32| -> Task<Message> {
        operation::scroll_by(scroll_id(id), AbsoluteOffset { x: 0.0, y })
    };
    match scroll {
        Scroll::LineUp => by(-line),
        Scroll::LineDown => by(line),
        Scroll::PageUp => by(-page),
        Scroll::PageDown => by(page),
        Scroll::Start => operation::snap_to(scroll_id(id), RelativeOffset::START),
        Scroll::End => operation::snap_to_end(scroll_id(id)),
        Scroll::Left | Scroll::Right => Task::none(),
    }
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
