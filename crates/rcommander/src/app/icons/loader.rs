//! The loader thread: it owns the platform's [`IconProvider`] and answers batches of keys, off
//! the UI thread. On Windows the shell wants COM on the thread that asks it, so the provider is
//! made on this thread and never leaves it.
//!
//! The newest batch counts most: while the list scrolls, batches queue up, and the loader
//! answers the per-file keys of every older one as skipped (the window asks again for those
//! still shown). Extension keys are cheap and few; they are always loaded.

use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use iced::widget::image::Handle;
use tokio::sync::oneshot;

use super::{key::IconKey, pixels::Rgba};

/// A source of native icons. Called on the loader thread only.
pub trait IconProvider {
    /// The icon of `key` at the list's size, or `None` when the platform has none.
    fn load(&self, key: &IconKey) -> Option<Rgba>;
}

/// What became of one key.
#[derive(Debug, Clone)]
pub enum Loaded {
    Icon(Handle),
    /// The provider has no icon for it.
    None,
    /// Passed over for a newer batch; asked for again while it is shown.
    Skipped,
}

/// The answers to one batch.
pub type Answers = Vec<(IconKey, Loaded)>;

struct Batch {
    keys: Vec<IconKey>,
    reply: oneshot::Sender<Answers>,
}

/// The window's end of the loader thread; the thread ends with it.
pub struct Loader {
    batches: Sender<Batch>,
}

impl Loader {
    /// Starts the thread, which makes its provider with `provider`; `None` when no thread
    /// starts.
    pub fn spawn<F>(provider: F) -> Option<Self>
    where
        F: FnOnce() -> Box<dyn IconProvider> + Send + 'static,
    {
        let (batches, queue) = mpsc::channel();
        thread::Builder::new()
            .name("rc-icons".into())
            .spawn(move || run(&*provider(), &queue))
            .ok()?;
        Some(Self { batches })
    }

    /// Sends `keys` to the thread; the answers come on the returned receiver. `None` when the
    /// thread is gone.
    pub fn request(&self, keys: Vec<IconKey>) -> Option<oneshot::Receiver<Answers>> {
        let (reply, answers) = oneshot::channel();
        self.batches.send(Batch { keys, reply }).ok()?;
        Some(answers)
    }
}

/// The thread's loop: every batch waiting, the newest one in full.
fn run(provider: &dyn IconProvider, queue: &Receiver<Batch>) {
    while let Ok(first) = queue.recv() {
        let mut batches = vec![first];
        batches.extend(queue.try_iter());
        let newest = batches.len() - 1;
        for (index, batch) in batches.into_iter().enumerate().rev() {
            let answers = answer(provider, batch.keys, index == newest);
            // The window may have stopped waiting; nothing is lost then.
            let _ = batch.reply.send(answers);
        }
    }
}

/// The answers to `keys`: per-file keys only for the newest batch.
pub fn answer(provider: &dyn IconProvider, keys: Vec<IconKey>, newest: bool) -> Answers {
    keys.into_iter()
        .map(|key| {
            let loaded = if key.per_file() && !newest {
                Loaded::Skipped
            } else {
                match provider.load(&key) {
                    Some(icon) => {
                        Loaded::Icon(Handle::from_rgba(icon.width, icon.height, icon.pixels))
                    }
                    None => Loaded::None,
                }
            };
            (key, loaded)
        })
        .collect()
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
