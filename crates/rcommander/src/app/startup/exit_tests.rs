use std::time::{Duration, Instant};

use super::within;

/// A shutdown that never ends is given up once the limit has passed.
#[tokio::test]
async fn a_hanging_shutdown_is_given_up_after_the_limit() {
    let start = Instant::now();
    let stopped = within(Duration::from_millis(100), std::future::pending::<()>()).await;
    assert_eq!(stopped, None);
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(100), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
}

/// A shutdown that ends in time answers with its own result.
#[tokio::test]
async fn a_shutdown_in_time_keeps_its_result() {
    let stopped = within(Duration::from_secs(5), async { 7 }).await;
    assert_eq!(stopped, Some(7));
}
