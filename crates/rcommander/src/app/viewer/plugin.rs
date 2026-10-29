//! A file opened with its viewer plugin (RC-100-13): the plugin that claims it opens it off the
//! UI thread (`rc_engine::plugins`), its text becomes the Lister's file, its picture the image
//! view's, its views and questions the lister's [`Document`]. A document whose first output is
//! its picture has its text written on the first switch to it (RC-100-29, `output.rs`). No
//! claim, a sensitive file (`rc_mcp::sensitive_path`) or the plugins switched off leave the
//! built-in viewer; a failing plugin leaves it too, with a notice in the status line. Closing
//! the Lister or Quick View moving on drops the lister, whose flag ends the plugin's running
//! call and every later one.

use std::{path::PathBuf, sync::Arc};

use iced::{Color, Task};
use rc_db::Db;
use rc_engine::{
    Engine, VfsPath,
    plugins::{
        self, OpenRequest, Opening, Output, PictureFormat, PluginDocument, Purpose, Theme,
        ViewerPlugins,
    },
    viewer::{DETECT_BYTES, Detected, ViewMode, sniff},
};
use tokio::sync::OnceCell;

use super::{
    ViewerMessage,
    document::{Answer, DocView, Document, DocumentSource, Token, TokenClass, TreeNode},
    lister::{Lister, Request, Shown},
    load::{self, Opened, Picture},
    markdown::Markdown,
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    i18n::{self, t},
    theme,
};

/// The viewer plugins, loaded for the first file a viewer opens and again after a `plugins.*`
/// setting changed; documents open keep their sessions.
#[derive(Debug, Clone, Default)]
pub struct Plugins(Arc<OnceCell<ViewerPlugins>>);

/// Whether a changed setting is one the loaded plugins follow.
pub fn concerns(key: &str) -> bool {
    key.starts_with("plugins.") && key != rc_engine::settings::VIEWER_TOKEN_COLORS_KEY
}

/// What a plugin made of a file.
#[derive(Debug, Clone)]
pub enum PluginOpened {
    /// Its document; `opened` is its output, or the file itself for a document with no text.
    Document {
        opened: Result<Opened, &'static str>,
        document: Arc<PluginDocument>,
    },
    /// It failed: the file in the built-in viewer, and why.
    Fallback {
        opened: Result<Opened, &'static str>,
        notice: String,
    },
}

/// The window's side of a document: the session answers off the UI thread, and the outputs
/// are read as the Lister reads any file.
struct PluginSource {
    document: Arc<PluginDocument>,
    engine: Engine,
    db: Option<Db>,
}

impl std::fmt::Debug for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PluginSource").field(&self.document).finish()
    }
}

impl DocumentSource for PluginSource {
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>> {
        let document = Arc::clone(&self.document);
        Box::pin(async move {
            let lines = document.tokens(&starts).await?;
            Ok(lines
                .into_iter()
                .map(|spans| spans.into_iter().map(token).collect())
                .collect())
        })
    }

    fn children(&self, node: Option<u64>) -> Answer<Vec<TreeNode>> {
        let document = Arc::clone(&self.document);
        Box::pin(async move {
            let nodes = document.children(node).await?;
            Ok(nodes
                .into_iter()
                .map(|node| TreeNode {
                    id: node.id,
                    label: node.label,
                    value: node.value,
                    has_children: node.has_children,
                })
                .collect())
        })
    }

    fn text(&self) -> Answer<Opened> {
        let (document, engine, db) = (
            Arc::clone(&self.document),
            self.engine.clone(),
            self.db.clone(),
        );
        Box::pin(async move {
            let (path, detected) = document.write_text(&engine).await?;
            let path = path.to_path_buf();
            open_text(engine, path, detected, db).await
        })
    }

    fn picture(&self) -> Answer<Picture> {
        let (document, engine, db) = (
            Arc::clone(&self.document),
            self.engine.clone(),
            self.db.clone(),
        );
        Box::pin(async move {
            let (path, format) = document.write_picture().await?;
            let path = path.to_path_buf();
            read_picture(engine, path, format, db).await
        })
    }
}

/// A document's text output opened for the Lister: read as the plugin named it.
async fn open_text(
    engine: Engine,
    path: PathBuf,
    detected: Detected,
    db: Option<Db>,
) -> Result<Opened, &'static str> {
    let mut opened = load::open(engine, VfsPath::local(path), db).await?;
    opened.detected = detected;
    opened.mode = ViewMode::Text;
    Ok(opened)
}

/// A document's picture output decoded for the image view: an encoded one by the format its
/// head shows, a raw raster by the size the plugin named.
async fn read_picture(
    engine: Engine,
    path: PathBuf,
    format: PictureFormat,
    db: Option<Db>,
) -> Result<Picture, &'static str> {
    let file = engine
        .open_for_view(&VfsPath::local(path))
        .await
        .map_err(|error| error.code())?;
    let file = Arc::new(file);
    match format {
        PictureFormat::Rgba { width, height } => load::raster(file, width, height, db).await,
        PictureFormat::Encoded => {
            let head = file
                .read_uncached(0, DETECT_BYTES)
                .await
                .map_err(|error| error.code())?;
            let format = sniff::sniff_image(&head).ok_or("image_broken")?;
            load::picture(file, format, db).await
        }
    }
}

impl App {
    /// Opens `path` for the lister `id`: with the plugin that claims it, else built in.
    pub(super) fn open_file(&self, id: u64, path: VfsPath) -> Effect {
        Effect::Task(self.open_task(id, path))
    }

    /// [`App::open_file`] as a task, for F2 on a plugin's document too.
    pub(super) fn open_task(&self, id: u64, path: VfsPath) -> Task<Message> {
        let quick = self
            .viewers
            .quick
            .as_ref()
            .is_some_and(|quick| quick.shows(id));
        let Some(cancel) = self.viewers.lister(id).map(Lister::plugin_stop) else {
            return Task::none();
        };
        let request = OpenRequest {
            purpose: if quick {
                Purpose::QuickView
            } else {
                Purpose::Lister
            },
            theme: plugin_theme(self.dark),
            cancel,
        };
        let opening = open(
            self.engine.clone(),
            path,
            self.db.clone(),
            self.viewers.plugins.clone(),
            request,
        );
        Task::perform(opening, move |answer| wrap(answer.into_message(id)))
    }

    /// A plugin's answer for the lister `id`: its document attached, or the built-in viewer
    /// with the notice. The document is attached before the first reads run, so a picture
    /// asked for is the document's, not the file's.
    pub(super) fn on_plugin(&mut self, id: u64, opened: PluginOpened) -> Vec<Effect> {
        let colors = self.viewers.token_colors.0;
        let (engine, db) = (self.engine.clone(), self.db.clone());
        let Some(lister) = self.viewers.lister_mut(id) else {
            return Vec::new();
        };
        match opened {
            PluginOpened::Fallback { opened, notice } => {
                let requests = take(lister, opened);
                lister.note = Some(notice);
                self.run(id, requests)
            }
            PluginOpened::Document { opened, document } => {
                let views = views(document.outputs());
                if views.contains(&DocView::Markdown) {
                    lister.markdown.get_or_insert_with(Markdown::default);
                }
                // A document opened again (F2) has a picture of its own.
                lister.picture = Shown::None;
                let later = document.text_path().is_none() && views.iter().any(|v| v.is_text());
                let mut requests = take(lister, opened);
                let source: Arc<dyn DocumentSource> = Arc::new(PluginSource {
                    document,
                    engine,
                    db,
                });
                if let Some(mut shown) = Document::new(views, source) {
                    shown.colors = colors;
                    if later {
                        shown = shown.text_later();
                    }
                    requests.extend(lister.set_document(shown));
                }
                self.run(id, requests)
            }
        }
    }
}

/// The lister takes the file opened, or the error.
fn take(lister: &mut Lister, opened: Result<Opened, &'static str>) -> Vec<Request> {
    match opened {
        Ok(opened) => lister.opened(opened),
        Err(code) => {
            lister.failed(code);
            Vec::new()
        }
    }
}

/// What came of opening a file.
enum Loaded {
    BuiltIn(Result<Opened, &'static str>),
    Plugin(PluginOpened),
}

impl Loaded {
    fn into_message(self, id: u64) -> ViewerMessage {
        match self {
            Self::BuiltIn(opened) => ViewerMessage::Opened(id, opened),
            Self::Plugin(opened) => ViewerMessage::Plugin(id, opened),
        }
    }
}

/// `path` opened by the plugin that claims it, else by the built-in viewer.
async fn open(
    engine: Engine,
    path: VfsPath,
    db: Option<Db>,
    plugins: Plugins,
    request: OpenRequest,
) -> Loaded {
    let set = plugins
        .0
        .get_or_init(|| ViewerPlugins::load(&engine, db.as_ref()))
        .await;
    match set
        .open(&engine, &path, rc_mcp::sensitive_path, request)
        .await
    {
        Opening::BuiltIn => Loaded::BuiltIn(load::open(engine, path, db).await),
        // Only a lister gone stops its plugin: nobody waits for the file.
        Opening::Cancelled => Loaded::BuiltIn(Err(plugins::PLUGIN_CANCELLED)),
        Opening::Failed { plugin, code } => Loaded::Plugin(PluginOpened::Fallback {
            opened: load::open(engine, path, db).await,
            notice: t!(
                "viewer.plugin_failed",
                plugin = plugin,
                reason = i18n::error_text(code)
            ),
        }),
        Opening::Document(document) => {
            // The text when it is written; else the file itself, beside the document's picture
            // or tree.
            let opened = match (document.text_path(), document.detected()) {
                (Some(text), Some(detected)) => {
                    open_text(engine, text.to_path_buf(), detected, db).await
                }
                _ => load::open(engine, path, db).await,
            };
            Loaded::Plugin(PluginOpened::Document {
                opened,
                document: Arc::from(document),
            })
        }
    }
}

/// The views of a document's outputs, the first shown first.
fn views(outputs: &[Output]) -> Vec<DocView> {
    outputs
        .iter()
        .filter_map(|output| match output {
            Output::Text => Some(DocView::Text),
            Output::Highlighted => Some(DocView::Highlighted),
            Output::Markdown => Some(DocView::Markdown),
            Output::Image => Some(DocView::Image),
            Output::Tree => Some(DocView::Tree),
            Output::Pages => None,
        })
        .collect()
}

fn token(token: plugins::Token) -> Token {
    Token {
        start: token.start,
        len: token.len,
        class: match token.class {
            plugins::TokenClass::Keyword => TokenClass::Keyword,
            plugins::TokenClass::Key => TokenClass::Key,
            plugins::TokenClass::String => TokenClass::String,
            plugins::TokenClass::Number => TokenClass::Number,
            plugins::TokenClass::Literal => TokenClass::Literal,
            plugins::TokenClass::Comment => TokenClass::Comment,
            plugins::TokenClass::Punctuation => TokenClass::Punctuation,
            plugins::TokenClass::Operator => TokenClass::Operator,
            plugins::TokenClass::TypeName => TokenClass::TypeName,
            plugins::TokenClass::Function => TokenClass::Function,
            plugins::TokenClass::Invalid => TokenClass::Invalid,
        },
    }
}

/// The theme a plugin is told at `open`: light or dark, and the window's colours.
fn plugin_theme(dark: bool) -> Theme {
    let tokens = theme::tokens(dark);
    Theme {
        dark,
        background: hex(tokens.window),
        foreground: hex(tokens.text),
        accent: hex(tokens.accent),
    }
}

/// `#rrggbb`.
fn hex(color: Color) -> String {
    let [red, green, blue, _] = color.into_rgba8();
    format!("#{red:02x}{green:02x}{blue:02x}")
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
