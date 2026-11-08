//! A terminal session's events as the engine's, and the prompt as a reason to read the panels
//! again (RC-110-02): a file made in the embedded terminal (`touch x`, `git checkout`, a build)
//! shows without the focus ever leaving the window, so coming back to it (RC-100-69) never
//! reads it. When a local or WSL shell is back at its prompt after a line the person entered
//! (`TermEvent::Returned`, OSC 133), the shown local file lists are read again through the
//! watcher's bundling - also where no watcher runs (`panels.watch` off, a WSL share, an
//! excluded drive). `terminal.refresh_panels_on_prompt` switches it off. An SSH terminal reads
//! nothing: its prompt says nothing about this machine's folders. A forged prompt mark costs one
//! bundled reading, no more.

use rc_term::{TermEvent, TermId, TermSession};
use tokio::sync::broadcast::error::RecvError;

use crate::{Engine, EngineEvent, TerminalChange, settings::REFRESH_ON_PROMPT_KEY};

impl Engine {
    /// Announces the session's title changes and its end as engine events; after the end its
    /// panel turns away from it. Holds the engine weakly: a session never keeps it alive.
    pub(super) fn watch_terminal(&self, session: &TermSession) {
        let terminal = session.id();
        let mut events = session.subscribe();
        let exited_already = session.state().is_exited();
        let engine_events = self.inner.events.clone();
        let engine = std::sync::Arc::downgrade(&self.inner);
        let announce = move |change| {
            // No subscriber is not an error: a headless engine may have none.
            let _ = engine_events.send(EngineEvent::Terminal { terminal, change });
        };
        if exited_already {
            announce(TerminalChange::Exited);
            return;
        }
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(TermEvent::Title(_)) => announce(TerminalChange::Title),
                    Ok(TermEvent::Folder) => announce(TerminalChange::Folder),
                    Ok(TermEvent::Prompt) => announce(TerminalChange::Follow),
                    Ok(TermEvent::Returned) => {
                        if let Some(inner) = engine.upgrade() {
                            Engine { inner }.shell_returned(terminal).await;
                        }
                    }
                    Ok(TermEvent::Exited(_)) => {
                        announce(TerminalChange::Exited);
                        if let Some(inner) = engine.upgrade() {
                            Engine { inner }.terminal_ended(terminal);
                        }
                        break;
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    /// The shell of `terminal` is back at its prompt after a command: the shown local file
    /// lists are to be read again, unless it is an SSH terminal or
    /// `terminal.refresh_panels_on_prompt` is off. Whether the watcher's loop was told.
    pub(crate) async fn shell_returned(&self, terminal: TermId) -> bool {
        let local = self
            .terminals()
            .get(&terminal)
            .is_some_and(|entry| entry.remote.is_none());
        local && self.setting_on(REFRESH_ON_PROMPT_KEY).await && self.watch_prompt()
    }
}
