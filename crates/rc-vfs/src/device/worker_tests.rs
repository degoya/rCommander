use std::{rc::Rc, sync::Arc, thread};

use super::Worker;
use crate::device::DeviceError;

/// A state no other thread may hold (`Rc` is neither `Send` nor `Sync`), as COM objects are.
struct ThreadBound {
    count: Rc<u32>,
    thread: thread::ThreadId,
}

#[test]
fn calls_run_one_after_another_on_the_thread_that_made_the_state() {
    let worker = Worker::start("test-worker", || {
        Ok(ThreadBound {
            count: Rc::new(0),
            thread: thread::current().id(),
        })
    })
    .expect("start");
    let worker = Arc::new(worker);
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let worker = Arc::clone(&worker);
            thread::spawn(move || {
                worker
                    .run(|state| {
                        state.count = Rc::new(*state.count + 1);
                        state.thread == thread::current().id()
                    })
                    .expect("run")
            })
        })
        .collect();
    for caller in callers {
        assert!(caller.join().expect("join"), "on the worker's own thread");
    }
    assert_eq!(worker.run(|state| *state.count).expect("run"), 4);
}

#[test]
fn a_state_that_does_not_open_is_the_answer() {
    let started = Worker::<u32>::start("test-worker", || Err(DeviceError::Unavailable));
    assert!(matches!(started, Err(DeviceError::Unavailable)));
}

#[test]
fn a_worker_whose_call_panicked_is_unavailable_not_a_hang() {
    let worker = Worker::start("test-worker", || Ok(0u32)).expect("start");
    let panicked = worker.run(|_| -> u32 { panic!("a device call that panics") });
    assert!(matches!(panicked, Err(DeviceError::Unavailable)));
    assert!(matches!(
        worker.run(|state| *state),
        Err(DeviceError::Unavailable)
    ));
}
