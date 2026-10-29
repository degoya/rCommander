//! Opening a file with a plugin that claims it, off the UI thread: the sensitive-file check,
//! the claims, `probe` of each in turn until one takes it, `open` with the theme, the document's
//! first output - its text or its picture - into a file of the session (the other one waits for
//! the first switch to it, RC-100-29), and cancellation at every step - Quick View moving on
//! raises the flag, the running call is asked to stop (`should-stop`) and ended at the epoch,
//! and nothing of it stays open.
//!
//! Sensitive files - what the MCP read guard refuses (`rc_mcp::sensitive_path`, handed in as the
//! [`ReadJudge`]: `rc-mcp` depends on this crate) and any file whose head holds a private key -
//! never reach a plugin; the built-in viewer shows them (`docs/plugins.md`, decision 8).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use rc_plugin_api::viewer::{Output, Purpose, Theme};
use rc_plugin_host::{SourceReader, codes, installed};
use rc_vfs::VfsPath;
use tokio::sync::OnceCell;

use super::{
    document::{LiveGuard, PluginDocument},
    set::{InstalledViewer, ViewerPlugins},
    streams::WindowSource,
    write::{self, Stop},
};
use crate::{Engine, ReadJudge, read_guard::ReadGuard, viewer::secret};

/// How a document is to be opened.
#[derive(Clone, Debug)]
pub struct OpenRequest {
    pub purpose: Purpose,
    pub theme: Theme,
    /// The window's flag: raised when the Lister closes or Quick View moves on.
    pub cancel: Arc<AtomicBool>,
}

/// What became of a file the window opens.
#[derive(Debug)]
pub enum Opening {
    /// The built-in viewer shows it: no plugin claims it, it is sensitive, the plugins are off,
    /// or every plugin that claims it declined it in `probe`.
    BuiltIn,
    /// The person moved on before the plugin was done; nothing stays open.
    Cancelled,
    /// The plugin's document.
    Document(Box<PluginDocument>),
    /// A plugin that claims the file failed: the built-in viewer shows it, with `code` as the
    /// notice; the plugins after it are not asked.
    Failed { plugin: String, code: &'static str },
}

impl ViewerPlugins {
    /// `path` opened by the first plugin of its claims whose `probe` takes it, or the reason the
    /// built-in viewer shows it. The judge decides what is sensitive; the engine opens the file
    /// on its backend.
    pub async fn open(
        &self,
        engine: &Engine,
        path: &VfsPath,
        judge: ReadJudge,
        request: OpenRequest,
    ) -> Opening {
        if self.plugins.is_empty() {
            return Opening::BuiltIn;
        }
        let guard = ReadGuard::new(judge, &path.backend);
        if guard.refuses_off_thread(&path.path).await.is_some() {
            return Opening::BuiltIn;
        }
        let Ok(file) = engine.open_for_view(path).await else {
            return Opening::BuiltIn;
        };
        let Ok(head) = file.read_uncached(0, self.head_length()).await else {
            return Opening::BuiltIn;
        };
        if secret::has_private_key(&head) {
            return Opening::BuiltIn;
        }
        let name = path
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let claims = self.claims(&name, &head, file.size());
        let file = Arc::new(file);
        // Only a declining `probe` hands the file on; a failure is the notice, so a broken
        // plugin does not vanish behind the next one. `run` checks the flag before each `probe`.
        for viewer in claims {
            let source: Arc<dyn SourceReader> =
                Arc::new(WindowSource::new(Arc::clone(&file), &name));
            match self.run(engine, &viewer, source, request.clone()).await {
                Ok(Some(document)) => return Opening::Document(Box::new(document)),
                Ok(None) => {}
                Err(Stop::Cancelled) => return Opening::Cancelled,
                Err(Stop::Failed(code)) => {
                    return Opening::Failed {
                        plugin: viewer.name().to_owned(),
                        code,
                    };
                }
            }
        }
        Opening::BuiltIn
    }

    /// `probe`, `open` and the output of `viewer` for `source`; `None` when `probe` declines.
    async fn run(
        &self,
        engine: &Engine,
        viewer: &InstalledViewer,
        source: Arc<dyn SourceReader>,
        request: OpenRequest,
    ) -> Result<Option<PluginDocument>, Stop> {
        let stopped = || request.cancel.load(Ordering::SeqCst);
        if stopped() {
            return Err(Stop::Cancelled);
        }
        if !viewer
            .plugin
            .probe(Arc::clone(&source))
            .await
            .map_err(|failure| Stop::of(&failure))?
        {
            return Ok(None);
        }
        if stopped() {
            return Err(Stop::Cancelled);
        }
        let live = LiveGuard::new(&self.live);
        let in_use = viewer.version_dir.as_deref().map(installed::hold);
        let cancel = Arc::clone(&request.cancel);
        let mut session = viewer
            .plugin
            .open(source, request.purpose, &request.theme, Arc::clone(&cancel))
            .await
            .map_err(|failure| Stop::of(&failure))?;
        let document = session.document().clone();
        let mut outputs = document.outputs;
        // Pages come after stage 1 (`docs/plugins.md`, "Staged implementation").
        outputs.retain(|offered| *offered != Output::Pages);
        if outputs.is_empty() {
            return Err(Stop::Failed(codes::PLUGIN_OUTPUT_NOT_OFFERED));
        }
        // The first output with a file is written now, the other one on the first switch to it
        // (RC-100-29): Quick View, moving on fast, pays for one.
        let first = outputs
            .iter()
            .find(|offered| offered.is_textual() || **offered == Output::Image);
        let (text, picture) = match first {
            Some(Output::Image) => (None, Some(write::picture(&mut session).await?)),
            Some(_) => (
                Some(write::text(engine, &mut session, &cancel).await?),
                None,
            ),
            None => (None, None),
        };
        if stopped() {
            return Err(Stop::Cancelled);
        }
        Ok(Some(PluginDocument {
            plugin: viewer.name().to_owned(),
            outputs,
            title: document.title,
            text: OnceCell::new_with(text),
            picture: OnceCell::new_with(picture),
            session: tokio::sync::Mutex::new(session),
            cancel,
            _live: live,
            _in_use: in_use,
        }))
    }
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "open_secret_tests.rs"]
mod secret_tests;
