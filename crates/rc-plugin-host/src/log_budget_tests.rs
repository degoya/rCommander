use super::{LogBudget, MAX_LOG_LINES_PER_CALL};

#[test]
fn a_call_writes_fifty_lines_and_counts_the_rest() {
    let mut budget = LogBudget::new("plugin".to_owned(), "1.0.0".to_owned());
    budget.begin("open");
    let admitted = (0..10_000).filter(|_| budget.admit()).count();
    assert_eq!(admitted, MAX_LOG_LINES_PER_CALL as usize);
    assert_eq!(budget.written(), MAX_LOG_LINES_PER_CALL);
    assert_eq!(
        budget.suppressed(),
        10_000 - u64::from(MAX_LOG_LINES_PER_CALL)
    );
}

#[test]
fn the_next_call_starts_at_zero() {
    let mut budget = LogBudget::new("plugin".to_owned(), "1.0.0".to_owned());
    budget.begin("open");
    while budget.admit() {}
    budget.begin("text");
    assert_eq!((budget.written(), budget.suppressed()), (0, 0));
    assert_eq!(budget.origin().operation, "text");
    assert!(budget.admit());
}
