use super::*;
use crate::kdf::KEY_LEN;

const MINUTE: Duration = Duration::from_secs(60);

fn key() -> VaultKey {
    VaultKey::from_bytes([9; KEY_LEN])
}

#[test]
fn no_vault_is_absent_and_has_no_key() {
    let mut machine = Machine::new(false, 15);
    let now = Instant::now();
    assert_eq!(machine.state(), VaultState::Absent);
    assert!(matches!(
        machine.key_for_use(now, Use::Person),
        Err(VaultError::Absent)
    ));
    assert!(!machine.lock());
    assert_eq!(machine.status(now).state, VaultStateKind::Absent);
}

#[test]
fn unlock_use_and_lock() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 15);
    assert_eq!(machine.state(), VaultState::Locked);
    assert!(matches!(
        machine.key_for_use(clock.now(), Use::Person),
        Err(VaultError::Locked)
    ));

    let since = clock.now();
    machine.unlocked(key(), since);
    clock.advance(MINUTE);
    machine
        .key_for_use(clock.now(), Use::Person)
        .expect("unlocked");
    assert_eq!(
        machine.state(),
        VaultState::Unlocked {
            since,
            last_use: clock.now()
        }
    );

    assert!(machine.lock());
    assert_eq!(machine.state(), VaultState::Locked);
    assert!(matches!(
        machine.key_for_use(clock.now(), Use::Person),
        Err(VaultError::Locked)
    ));
    assert!(!machine.lock(), "locking twice reports the second as no-op");
}

#[test]
fn the_idle_lock_falls_due_after_the_set_minutes_without_use() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 15);
    machine.unlocked(key(), clock.now());

    clock.advance(14 * MINUTE);
    assert!(!machine.lock_if_idle(clock.now()));
    assert_eq!(machine.status(clock.now()).locks_in_secs, Some(60));
    // Use defers it: fifteen minutes from now.
    machine
        .key_for_use(clock.now(), Use::Person)
        .expect("still unlocked");
    clock.advance(14 * MINUTE);
    assert!(!machine.lock_if_idle(clock.now()));

    clock.advance(MINUTE);
    assert!(machine.lock_if_idle(clock.now()));
    assert_eq!(machine.state(), VaultState::Locked);
    assert_eq!(machine.status(clock.now()).locks_in_secs, None);
}

#[test]
fn a_use_after_the_deadline_finds_it_locked() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 1);
    machine.unlocked(key(), clock.now());
    clock.advance(MINUTE);
    assert!(matches!(
        machine.key_for_use(clock.now(), Use::Person),
        Err(VaultError::Locked)
    ));
}

#[test]
fn zero_minutes_never_locks() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 0);
    machine.set_max_unlocked_hours(0);
    machine.unlocked(key(), clock.now());
    clock.advance(1000 * MINUTE);
    assert!(!machine.lock_if_idle(clock.now()));
    assert_eq!(machine.lock_deadline(), None);
    machine
        .key_for_use(clock.now(), Use::Person)
        .expect("still unlocked");

    machine.set_idle_lock_minutes(2);
    clock.advance(2 * MINUTE);
    assert!(machine.lock_if_idle(clock.now()));
}

/// RC-080-11 (V1): an agent reading secrets over MCP every few minutes does not keep the vault
/// open; the person's own use does.
#[test]
fn an_agents_use_does_not_defer_the_idle_lock() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 15);
    machine.unlocked(key(), clock.now());
    for _ in 0..2 {
        clock.advance(7 * MINUTE);
        machine
            .key_for_use(clock.now(), Use::Agent)
            .expect("the key opens for the agent");
    }
    assert_eq!(machine.status(clock.now()).locks_in_secs, Some(60));
    clock.advance(MINUTE);
    assert!(matches!(
        machine.key_for_use(clock.now(), Use::Agent),
        Err(VaultError::Locked)
    ));
    assert_eq!(machine.state(), VaultState::Locked);
}

/// RC-080-11 (V1): however often the person uses it, the vault locks the set hours after the
/// unlock; typing the master password again starts them over.
#[test]
fn the_longest_time_unlocked_locks_however_often_it_is_used() {
    let clock = ManualClock::new();
    let mut machine = Machine::new(true, 15);
    machine.set_max_unlocked_hours(1);
    machine.unlocked(key(), clock.now());
    for _ in 0..5 {
        clock.advance(10 * MINUTE);
        machine
            .key_for_use(clock.now(), Use::Person)
            .expect("still unlocked");
    }
    // Fifty minutes in, the hour comes before the idle lock.
    assert_eq!(machine.status(clock.now()).locks_in_secs, Some(10 * 60));
    clock.advance(10 * MINUTE);
    assert!(matches!(
        machine.key_for_use(clock.now(), Use::Person),
        Err(VaultError::Locked)
    ));

    machine.unlocked(key(), clock.now());
    clock.advance(59 * MINUTE);
    machine.set_idle_lock_minutes(0);
    assert!(
        !machine.lock_if_idle(clock.now()),
        "a new hour from the unlock"
    );
    clock.advance(MINUTE);
    assert!(machine.lock_if_idle(clock.now()));

    machine.set_max_unlocked_hours(0);
    machine.unlocked(key(), clock.now());
    clock.advance(1000 * MINUTE);
    assert!(!machine.lock_if_idle(clock.now()), "0 sets no limit");
}

#[test]
fn failed_attempts_wait_longer_after_the_first_three() {
    let base = Duration::from_secs(1);
    let delays: Vec<u64> = (0..12)
        .map(|failures| failure_delay(failures, base).as_secs())
        .collect();
    assert_eq!(delays, [0, 0, 0, 1, 2, 4, 8, 16, 30, 30, 30, 30]);
    assert_eq!(failure_delay(u32::MAX, base), MAX_FAILURE_DELAY);
    assert_eq!(failure_delay(10, Duration::ZERO), Duration::ZERO);

    let mut machine = Machine::new(true, 15);
    // The answer is the wait before the next attempt: the fourth is the first to wait.
    for _ in 0..2 {
        assert_eq!(machine.failed(base), Duration::ZERO);
    }
    assert_eq!(machine.failed(base), base);
    assert_eq!(machine.pending_delay(base), base);
    machine.unlocked(key(), Instant::now());
    assert_eq!(
        machine.pending_delay(base),
        Duration::ZERO,
        "success resets"
    );
}

#[test]
fn the_status_serializes_without_times_or_keys() {
    let status = VaultStatus {
        state: VaultStateKind::Unlocked,
        idle_lock_minutes: 15,
        locks_in_secs: Some(42),
    };
    assert_eq!(
        serde_json::to_value(&status).expect("serializes"),
        serde_json::json!({ "state": "unlocked", "idle_lock_minutes": 15, "locks_in_secs": 42 })
    );
    assert_eq!(
        serde_json::to_value(VaultStatus::absent()).expect("serializes"),
        serde_json::json!({ "state": "absent", "idle_lock_minutes": 15 })
    );
}
