use std::{
    fmt::Write as _,
    sync::{Arc, Mutex},
};

use tracing_subscriber::layer::SubscriberExt as _;

use super::{LockOrRecover as _, LogPanic as _, RwLockOrRecover as _, recovered};

#[test]
fn a_thread_that_panicked_stays_an_error() {
    let joined = std::thread::spawn(|| -> u8 { panic!("the test's own panic") })
        .join()
        .log_panic("test.thread");
    assert!(joined.is_err());
}

#[test]
fn a_thread_that_finished_keeps_its_value() {
    let joined = std::thread::spawn(|| 7).join().log_panic("test.thread");
    assert_eq!(joined.ok(), Some(7));
}

/// Every event's target and fields, as one line each.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut line = format!("{} ", event.metadata().target());
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                let _ = write!(line, "{}={value:?} ", field.name());
            },
        );
        self.0.lock_or_recover("test.lines").push(line);
    }
}

#[tokio::test]
async fn a_task_that_panicked_is_logged_with_its_name() {
    let lines = Lines::default();
    let joined = {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        tokio::task::spawn_blocking(|| -> u8 { panic!("the test's own panic") })
            .await
            .log_panic("test.task")
    };
    assert!(joined.is_err_and(|error| error.is_panic()));
    let lines = lines.0.lock_or_recover("test.lines");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("panic "), "{}", lines[0]);
    assert!(lines[0].contains("task=\"test.task\""), "{}", lines[0]);
    assert!(lines[0].contains("recover_tests.rs"), "{}", lines[0]);
}

#[tokio::test]
async fn a_task_that_finished_logs_nothing() {
    let lines = Lines::default();
    let joined = {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        tokio::task::spawn_blocking(|| 7_u8)
            .await
            .log_panic("test.task")
    };
    assert_eq!(joined.ok(), Some(7));
    assert!(lines.0.lock_or_recover("test.lines").is_empty());
}

/// A mutex a panic poisoned while it was held, holding `value`.
fn poisoned(value: u8) -> std::sync::Mutex<u8> {
    let mutex = std::sync::Mutex::new(value);
    let _ = std::panic::catch_unwind(|| {
        let _guard = mutex.lock();
        panic!("the test's own panic");
    });
    assert!(mutex.is_poisoned());
    mutex
}

#[test]
fn a_poisoned_mutex_is_taken_with_its_state_and_its_poison_cleared() {
    let mutex = poisoned(1);
    *mutex.lock_or_recover("test.mutex") += 1;
    assert!(!mutex.is_poisoned());
    assert_eq!(*mutex.lock_or_recover("test.mutex"), 2);
}

#[test]
fn a_poisoned_lock_is_logged_once_with_its_name() {
    let mutex = poisoned(3);
    let lines = Lines::default();
    {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        assert_eq!(*mutex.lock_or_recover("test.mutex"), 3);
        assert_eq!(*mutex.lock_or_recover("test.mutex"), 3);
    }
    let lines = lines.0.lock_or_recover("test.lines");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("lock=\"test.mutex\""), "{}", lines[0]);
    assert!(lines[0].contains("recover_tests.rs"), "{}", lines[0]);
}

#[test]
fn a_poisoned_rwlock_is_read_and_written_with_its_state() {
    let lock = std::sync::RwLock::new(1_u8);
    let _ = std::panic::catch_unwind(|| {
        let _guard = lock.write();
        panic!("the test's own panic");
    });
    assert!(lock.is_poisoned());
    assert_eq!(*lock.read_or_recover("test.rwlock"), 1);
    assert!(!lock.is_poisoned());
    *lock.write_or_recover("test.rwlock") += 1;
    assert_eq!(*lock.read_or_recover("test.rwlock"), 2);
}

#[test]
fn a_poisoned_wait_hands_its_guard_back_and_is_logged() {
    let mutex = poisoned(5);
    let lines = Lines::default();
    {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        let guard = mutex
            .lock()
            .unwrap_or_else(|poisoned| recovered(poisoned, "test.wait"));
        assert_eq!(*guard, 5);
    }
    let lines = lines.0.lock_or_recover("test.lines");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("lock=\"test.wait\""), "{}", lines[0]);
}
