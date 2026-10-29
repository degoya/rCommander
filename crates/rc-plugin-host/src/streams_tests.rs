use std::sync::Arc;

use super::{
    BytesSource, CREDITED_PASSES, FUEL_PER_BYTE, MAX_WINDOW_BYTES, MemorySink, Streams, read_window,
};
use crate::failure::codes;

#[tokio::test]
async fn a_read_serves_one_window_of_the_offered_source() {
    let mut streams = Streams::default();
    let bytes = vec![5_u8; MAX_WINDOW_BYTES as usize + 100];
    let handle = streams.add_source(Arc::new(BytesSource::new("big.bin", bytes)));
    let source = streams.source(handle).expect("offered");
    let window = read_window(source.clone(), 0, u32::MAX)
        .await
        .expect("read");
    assert_eq!(window.len(), MAX_WINDOW_BYTES as usize);
    let tail = read_window(source, u64::from(MAX_WINDOW_BYTES), 1000)
        .await
        .expect("read");
    assert_eq!(tail.len(), 100);
    assert_eq!(
        streams.source(handle + 1).err().and_then(|f| f.code),
        Some(codes::PLUGIN_UNKNOWN_HANDLE.to_owned())
    );
    assert_eq!(streams.info(handle).expect("info").name, "big.bin");
}

#[tokio::test]
async fn a_write_past_the_limit_is_refused_and_remembered() {
    let mut streams = Streams::default();
    let sink = MemorySink::new();
    let handle = streams.add_sink(Box::new(sink.clone()), 10);
    streams.write_at(handle, 0, b"hello").await.expect("within");
    let refused = streams.write_at(handle, 8, b"world").await;
    assert_eq!(
        refused.expect_err("past the limit").code(),
        Some(codes::PLUGIN_OUTPUT_TOO_LARGE)
    );
    assert_eq!(streams.committed(handle), 5, "the host's figure");
    // The plugin ignores the refusal and writes on; the output stays refused.
    streams
        .write_at(handle, 5, b"!")
        .await
        .expect("within again");
    let slot = streams.take_sink(handle).expect("the call's sink");
    assert_eq!(slot.refused, Some(codes::PLUGIN_OUTPUT_TOO_LARGE));
    assert_eq!(slot.committed, 6);
    assert_eq!(sink.contents(), b"hello!");
}

#[tokio::test]
async fn a_window_larger_than_a_mebibyte_is_refused() {
    let mut streams = Streams::default();
    let handle = streams.add_sink(Box::new(MemorySink::new()), u64::MAX);
    let big = vec![0_u8; MAX_WINDOW_BYTES as usize + 1];
    assert_eq!(
        streams
            .write_at(handle, 0, &big)
            .await
            .expect_err("too large")
            .code(),
        Some(codes::PLUGIN_WRITE_TOO_LARGE)
    );
}

#[test]
fn the_credit_ends_after_two_passes_and_starts_again_per_call() {
    let mut streams = Streams::default();
    streams.add_source(Arc::new(BytesSource::new("ten.bin", vec![0_u8; 10])));
    let ceiling = 10 * FUEL_PER_BYTE * CREDITED_PASSES;
    assert_eq!(streams.take_read_credit(10), 10 * FUEL_PER_BYTE);
    assert_eq!(streams.take_read_credit(1000), ceiling - 10 * FUEL_PER_BYTE);
    assert_eq!(streams.take_read_credit(1), 0, "a loop earns nothing more");
    streams.begin_call();
    assert_eq!(streams.take_read_credit(10), 10 * FUEL_PER_BYTE);
    assert_eq!(streams.take_write_credit(10), 0, "no sink, no write credit");
}
