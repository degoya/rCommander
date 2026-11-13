use super::Known;

#[test]
fn a_holder_gives_what_it_was_made_with_until_it_is_set() {
    static LIST: Known<Vec<u8>> = Known::new("test.known_list", Vec::new());
    assert!(LIST.get().is_empty());
    assert_eq!(LIST.with(|list| list.len()), 0);
    LIST.set(vec![1, 2, 3]);
    assert_eq!(LIST.get(), [1, 2, 3]);
    assert_eq!(
        LIST.with(|list| list.iter().map(|n| u32::from(*n)).sum::<u32>()),
        6
    );
    LIST.set(Vec::new());
    assert!(LIST.get().is_empty());
}

#[test]
fn a_holder_of_an_option_starts_empty() {
    static START: Known<Option<u8>> = Known::new("test.known_start", None);
    assert_eq!(START.get(), None);
    START.set(Some(4));
    assert_eq!(START.get(), Some(4));
    assert!(START.with(Option::is_some));
}
