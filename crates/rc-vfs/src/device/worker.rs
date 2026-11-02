//! One thread that owns a state no other thread may touch - a device's COM objects, which
//! Windows hands to the thread that made them - and runs the calls sent to it, one at a time, in
//! the order they came. A caller blocks until its call ran; a device answers one request at a
//! time anyway. The state is made on the thread and dropped there once the last handle is gone.

use std::{
    sync::mpsc::{self, Sender},
    thread,
};

use super::{DeviceError, DeviceResult};

type Job<S> = Box<dyn FnOnce(&mut S) + Send>;

/// The handle of a worker thread; its state lives until the last handle is dropped.
pub(crate) struct Worker<S> {
    jobs: Sender<Job<S>>,
}

impl<S: 'static> Worker<S> {
    /// A thread named `name` whose state `open` makes there; its failure is the answer.
    pub(crate) fn start<F>(name: &str, open: F) -> DeviceResult<Self>
    where
        F: FnOnce() -> DeviceResult<S> + Send + 'static,
    {
        let (jobs, inbox) = mpsc::channel::<Job<S>>();
        let (opened, answer) = mpsc::channel::<DeviceResult<()>>();
        thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let mut state = match open() {
                    Ok(state) => state,
                    Err(error) => {
                        // A starter that has gone needs no answer.
                        let _ = opened.send(Err(error));
                        return;
                    }
                };
                let _ = opened.send(Ok(()));
                while let Ok(job) = inbox.recv() {
                    job(&mut state);
                }
            })
            .map_err(DeviceError::Io)?;
        match answer.recv() {
            Ok(Ok(())) => Ok(Self { jobs }),
            Ok(Err(error)) => Err(error),
            // The thread ended without a word: `open` panicked.
            Err(_) => Err(DeviceError::Unavailable),
        }
    }

    /// Runs `job` on the thread and answers what it returned; `Unavailable` once the thread is
    /// gone (a call of an earlier job panicked).
    pub(crate) fn run<R, F>(&self, job: F) -> DeviceResult<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut S) -> R + Send + 'static,
    {
        let (reply, answer) = mpsc::channel::<R>();
        let job: Job<S> = Box::new(move |state| {
            // A caller that has gone needs no answer.
            let _ = reply.send(job(state));
        });
        self.jobs.send(job).map_err(|_| DeviceError::Unavailable)?;
        answer.recv().map_err(|_| DeviceError::Unavailable)
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
