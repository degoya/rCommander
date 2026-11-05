use super::{LockOrRecover as _, LogPanic as _};

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
