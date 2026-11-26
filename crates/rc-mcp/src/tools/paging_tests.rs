use std::sync::Arc;

use super::{Kept, PAGE, Page, print};

/// Every page of `items` from the first on, as a client follows the cursors.
fn all_pages(kept: &Kept<Vec<u32>>, request: u64, items: &Arc<Vec<u32>>) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let start = kept.start(request, cursor.as_deref()).expect("starts");
        let listing = start.kept.clone().unwrap_or_else(|| Arc::clone(items));
        let page = kept
            .page(request, 7, &start, PAGE, (&listing, listing.len()))
            .expect("a page");
        cursor = page.next_cursor.clone();
        pages.push(page);
        if cursor.is_none() {
            return pages;
        }
    }
}

#[test]
fn twenty_five_hundred_items_come_in_three_pages_each_once() {
    let kept = Kept::default();
    let items: Arc<Vec<u32>> = Arc::new((0..2500).collect());
    let pages = all_pages(&kept, 1, &items);
    let spans: Vec<(usize, usize, bool)> = pages
        .iter()
        .map(|page| (page.from, page.to, page.truncated()))
        .collect();
    assert_eq!(
        spans,
        [(0, 1000, true), (1000, 2000, true), (2000, 2500, false)]
    );
    let seen: Vec<u32> = pages
        .iter()
        .flat_map(|page| items[page.from..page.to].iter().copied())
        .collect();
    assert_eq!(seen, *items, "complete, in order, nothing twice");
}

#[test]
fn a_page_goes_on_with_the_listing_its_cursor_kept() {
    let kept = Kept::default();
    let first: Arc<Vec<u32>> = Arc::new((0..1500).collect());
    let start = kept.start(1, None).expect("starts");
    let page = kept
        .page(1, 7, &start, PAGE, (&first, first.len()))
        .expect("a page");
    let cursor = page.next_cursor.expect("more follow");
    let next = kept.start(1, Some(&cursor)).expect("goes on");
    let held = next.kept.clone().expect("the first page's listing");
    assert!(
        Arc::ptr_eq(&held, &first),
        "the same listing, not a new one"
    );
    assert_eq!(next.offset, 1000);
    // Taken: a cursor goes on once with its kept listing; again it lists anew.
    assert!(kept.start(1, Some(&cursor)).expect("starts").kept.is_none());
}

#[test]
fn a_cursor_of_another_call_or_folder_is_refused() {
    let kept = Kept::default();
    let items: Arc<Vec<u32>> = Arc::new((0..1500).collect());
    let start = kept.start(1, None).expect("starts");
    let page = kept
        .page(1, print(&"/a"), &start, PAGE, (&items, items.len()))
        .expect("a page");
    let cursor = page.next_cursor.expect("more follow");

    // Another call (another folder named, another tool) does not take it.
    assert!(kept.start(2, Some(&cursor)).is_err());
    for broken in ["", "x", "1000-1", "1000-1-2-3-4", "-1-0000000000000001-1-1"] {
        assert!(kept.start(1, Some(broken)).is_err(), "{broken:?}");
    }

    // Its listing went (another store stands for that), and the folder listed now is another
    // one: refused rather than pages of two folders mixed.
    let elsewhere: Kept<Vec<u32>> = Kept::default();
    let start = elsewhere.start(1, Some(&cursor)).expect("starts");
    assert!(start.kept.is_none());
    assert!(
        elsewhere
            .page(1, print(&"/b"), &start, PAGE, (&items, items.len()))
            .is_err()
    );
    let page = elsewhere
        .page(1, print(&"/a"), &start, PAGE, (&items, items.len()))
        .expect("the same folder listed again goes on");
    assert_eq!((page.from, page.to, page.truncated()), (1000, 1500, false));
}

#[test]
fn a_limit_shortens_the_page() {
    let kept = Kept::default();
    let items: Arc<Vec<u32>> = Arc::new((0..10).collect());
    let start = kept.start(1, None).expect("starts");
    let page = kept
        .page(1, 7, &start, 4, (&items, items.len()))
        .expect("a page");
    assert_eq!((page.from, page.to, page.truncated()), (0, 4, true));
    let start = kept.start(1, None).expect("starts");
    let all = kept
        .page(1, 7, &start, PAGE, (&items, items.len()))
        .expect("a page");
    assert_eq!(
        all,
        Page {
            from: 0,
            to: 10,
            next_cursor: None
        }
    );
}
