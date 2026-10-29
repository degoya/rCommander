use super::*;

#[test]
fn one_updater_at_a_time() {
    let data = tempfile::tempdir().expect("tempdir");
    assert!(!updater_running(data.path()));
    let held = UpdaterLock::acquire(data.path())
        .expect("lock")
        .expect("free");
    assert!(updater_running(data.path()));
    assert!(UpdaterLock::acquire(data.path()).expect("lock").is_none());
    drop(held);
    assert!(!updater_running(data.path()));
}

/// The updater waits for the last window; windows share the lock among themselves.
#[test]
fn the_updater_waits_until_no_window_holds_the_instance_lock() {
    let data = tempfile::tempdir().expect("tempdir");
    let first = InstanceLock::share(data.path())
        .expect("lock")
        .expect("free");
    let second = InstanceLock::share(data.path())
        .expect("lock")
        .expect("shared");
    assert!(!InstanceLock::wait_until_free(data.path(), Duration::ZERO).expect("wait"));
    drop(first);
    assert!(!InstanceLock::wait_until_free(data.path(), Duration::ZERO).expect("wait"));
    drop(second);
    assert!(InstanceLock::wait_until_free(data.path(), Duration::ZERO).expect("wait"));
    // Let go again at once: the new window takes its share.
    assert!(InstanceLock::share(data.path()).expect("lock").is_some());
}
