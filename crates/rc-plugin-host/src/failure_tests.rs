use super::{codes, readable, stopped};

#[test]
fn control_and_direction_characters_are_spelled_out() {
    assert_eq!(readable("evil\u{202e}txt.exe", 100), "evil\\u{202e}txt.exe");
    assert_eq!(readable("a\nb\u{7}", 100), "a\\nb\\u{7}");
    assert_eq!(readable("caf\u{e9}", 100), "caf\u{e9}");
    assert_eq!(readable("abcdef", 3), "abc\u{2026}");
    assert_eq!(readable("abc", 3), "abc");
}

#[test]
fn a_trap_becomes_its_code() {
    let fuel = wasmtime::Error::new(wasmtime::Trap::OutOfFuel);
    assert_eq!(
        stopped(Some(&fuel), false).code(),
        Some(codes::PLUGIN_OUT_OF_FUEL)
    );
    let interrupt = wasmtime::Error::new(wasmtime::Trap::Interrupt);
    assert_eq!(
        stopped(Some(&interrupt), false).code(),
        Some(codes::PLUGIN_TIMED_OUT)
    );
    let unreachable = wasmtime::Error::new(wasmtime::Trap::UnreachableCodeReached);
    assert_eq!(
        stopped(Some(&unreachable), false).code(),
        Some(codes::PLUGIN_CRASHED)
    );
    let grow = wasmtime::Error::msg("forcing trap when growing memory to 1048576 bytes");
    assert_eq!(
        stopped(Some(&grow), false).code(),
        Some(codes::PLUGIN_OUT_OF_MEMORY)
    );
    assert_eq!(stopped(None, false).code(), Some(codes::PLUGIN_TIMED_OUT));
    assert_eq!(
        stopped(Some(&unreachable), true).code(),
        Some(codes::PLUGIN_CANCELLED),
        "a cancelled call is the caller's doing, whatever stopped it"
    );
}
