use super::LockOrRecover as _;

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
