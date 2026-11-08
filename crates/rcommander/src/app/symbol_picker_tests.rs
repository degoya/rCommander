use iced::widget::Id;
use rc_engine::{
    Chord,
    user_commands::symbols::{self, Category},
};

use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

fn picker() -> SymbolPicker {
    SymbolPicker::open("", "Notepad", 24, [])
}

fn names(picker: &SymbolPicker) -> Vec<&'static str> {
    picker
        .shown
        .iter()
        .map(|at| symbols::all()[*at].name)
        .collect()
}

#[test]
fn it_opens_on_every_symbol_with_the_current_one_at_the_cursor() {
    let fresh = picker();
    assert_eq!(fresh.shelf, Shelf::All);
    assert_eq!(fresh.shown.len(), symbols::all().len());
    assert_eq!(fresh.cursor, Some(0));
    assert_eq!(fresh.tooltip, "Notepad");
    let on = SymbolPicker::open(" rc-symbol:folder_open ", "", 24, []);
    assert_eq!(on.chosen().map(|symbol| symbol.name), Some("folder_open"));
    let file = SymbolPicker::open("C:\\Tools\\WCMICONS.DLL", "", 24, []);
    assert_eq!(file.cursor, Some(0), "a file: the first symbol");
}

#[test]
fn recently_used_is_what_the_stored_commands_name_each_once() {
    let used = [
        "rc-symbol:folder",
        "C:\\Tools\\app.exe",
        "rc-symbol:no_such_symbol",
        "rc-symbol:folder",
        "rc-symbol:copy",
        "",
    ];
    let mut picker = SymbolPicker::open("", "", 24, used);
    let recent: Vec<&str> = picker
        .recent
        .iter()
        .map(|at| symbols::all()[*at].name)
        .collect();
    assert_eq!(recent, ["folder", "copy"]);
    picker.update(PickerMsg::Shelf(Shelf::Recent));
    assert_eq!(names(&picker), ["folder", "copy"]);

    let many: Vec<String> = symbols::all()
        .iter()
        .map(|symbol| symbol.reference())
        .collect();
    let full = SymbolPicker::open("", "", 24, many.iter().map(String::as_str));
    assert_eq!(full.recent.len(), MAX_RECENT);
}

#[test]
fn a_category_shows_its_symbols_and_the_search_its_matches() {
    let mut picker = picker();
    picker.update(PickerMsg::Shelf(Shelf::Category(Category::Arrows)));
    assert!(!picker.shown.is_empty());
    assert!(
        picker
            .shown
            .iter()
            .all(|at| symbols::all()[*at].category == Category::Arrows)
    );
    picker.update(PickerMsg::Shelf(Shelf::All));
    picker.update(PickerMsg::Query("Browse".into()));
    assert!(
        names(&picker).contains(&"folder_open"),
        "a keyword finds it"
    );
    picker.update(PickerMsg::Query("no symbol is called this".into()));
    assert!(picker.shown.is_empty());
    assert_eq!(picker.cursor, None);
    assert_eq!(
        picker.update(PickerMsg::Ok),
        Outcome::Open(false),
        "nothing to take"
    );
}

/// The acceptance's count: typing narrows what is shown already - after the first letter no
/// keystroke looks at more symbols than the one before left - and only a word made shorter or
/// changed looks at the shelf again.
#[test]
fn typing_narrows_without_reading_the_whole_collection_again() {
    let all = symbols::all().len();
    let mut picker = picker();
    let mut typed = String::new();
    let mut before = picker.shown.len();
    let mut total = 0;
    for letter in "folder".chars() {
        typed.push(letter);
        picker.update(PickerMsg::Query(typed.clone()));
        assert!(
            picker.examined <= before,
            "{typed}: looked at {}, {before} were shown",
            picker.examined
        );
        total += picker.examined;
        before = picker.shown.len();
    }
    assert!(before > 0 && before < all, "{before} folders");
    // Six letters looked at fewer symbols than reading the collection twice.
    assert!(total < 2 * all, "{total} looked at for {all} symbols");
    picker.update(PickerMsg::Query("folder open".into()));
    assert!(picker.examined <= before, "a second word narrows too");
    picker.update(PickerMsg::Query("fold".into()));
    assert_eq!(picker.examined, all, "a shorter word reads the shelf again");
    assert!(picker.shown.len() >= before);
}

#[test]
fn the_cursor_stays_on_its_symbol_while_the_search_keeps_it() {
    let mut picker = SymbolPicker::open("rc-symbol:folder_open", "", 24, []);
    picker.update(PickerMsg::Query("folder".into()));
    assert_eq!(
        picker.chosen().map(|symbol| symbol.name),
        Some("folder_open")
    );
    picker.update(PickerMsg::Query("printer".into()));
    assert_eq!(picker.cursor, Some(0), "gone: the first");
}

#[test]
fn the_cursor_moves_in_the_grid_and_ok_takes_its_reference() {
    let mut picker = picker();
    let last = picker.shown.len() - 1;
    assert_eq!(picker.update(PickerMsg::Move(1)), Outcome::Open(true));
    assert_eq!(picker.cursor, Some(1));
    picker.update(PickerMsg::Move(COLUMNS as isize));
    assert_eq!(picker.cursor, Some(1 + COLUMNS));
    picker.update(PickerMsg::Move(isize::MAX));
    assert_eq!(picker.cursor, Some(last));
    picker.update(PickerMsg::Move(1));
    assert_eq!(picker.cursor, Some(last), "the end holds");
    picker.update(PickerMsg::Move(isize::MIN));
    assert_eq!(picker.cursor, Some(0));
    assert_eq!(picker.update(PickerMsg::Move(-1)), Outcome::Open(false));
    assert_eq!(
        picker.cursor_row(),
        (0, picker.shown.len().div_ceil(COLUMNS))
    );

    assert_eq!(picker.update(PickerMsg::Select(3)), Outcome::Open(false));
    assert_eq!(picker.focus, PickFocus::Grid);
    let third = symbols::all()[picker.shown[3]].reference();
    assert_eq!(picker.update(PickerMsg::Ok), Outcome::Chosen(third));
    let fifth = symbols::all()[picker.shown[5]].reference();
    assert_eq!(picker.update(PickerMsg::Choose(5)), Outcome::Chosen(fifth));
    assert_eq!(picker.update(PickerMsg::Cancel), Outcome::Cancelled);
}

#[test]
fn the_keys_follow_the_ring() {
    let run = |focus, key: &str| super::key(focus, &chord(key), false);
    assert_eq!(
        run(PickFocus::Grid, "ArrowRight"),
        KeyAction::Run(PickerMsg::Move(1))
    );
    assert_eq!(
        run(PickFocus::Grid, "ArrowDown"),
        KeyAction::Run(PickerMsg::Move(COLUMNS as isize))
    );
    assert_eq!(
        run(PickFocus::Search, "ArrowUp"),
        KeyAction::Run(PickerMsg::Move(-(COLUMNS as isize)))
    );
    assert_eq!(
        run(PickFocus::Search, "ArrowLeft"),
        KeyAction::Nothing,
        "the field's"
    );
    assert_eq!(
        run(PickFocus::Grid, "End"),
        KeyAction::Run(PickerMsg::Move(isize::MAX))
    );
    assert_eq!(run(PickFocus::Grid, "Enter"), KeyAction::Run(PickerMsg::Ok));
    assert_eq!(
        run(PickFocus::Shelves, "Enter"),
        KeyAction::Run(PickerMsg::Ok)
    );
    assert_eq!(
        run(PickFocus::Ring, "Enter"),
        KeyAction::Nothing,
        "the button's"
    );
    assert_eq!(
        run(PickFocus::Ring, "Escape"),
        KeyAction::Run(PickerMsg::Cancel)
    );
    assert_eq!(run(PickFocus::Grid, "Tab"), KeyAction::Walk(false));
    assert_eq!(
        super::key(PickFocus::Grid, &chord("Shift+Tab"), false),
        KeyAction::Walk(true)
    );
    assert_eq!(
        super::key(PickFocus::Shelves, &chord("ArrowDown"), true),
        KeyAction::Nothing,
        "the shelves' stop took it"
    );
}

#[test]
fn the_ring_says_where_the_keys_go() {
    assert_eq!(focus_of(Some(&Id::new(SEARCH_FIELD))), PickFocus::Search);
    assert_eq!(
        focus_of(Some(&Id::new(SHELVES_STOP_ID))),
        PickFocus::Shelves
    );
    assert_eq!(focus_of(Some(&Id::new(GRID_STOP_ID))), PickFocus::Grid);
    assert_eq!(focus_of(None), PickFocus::Ring);
    let mut picker = picker();
    picker.update(PickerMsg::Focused(Some(Id::new(GRID_STOP_ID))));
    assert_eq!(picker.focus, PickFocus::Grid);
}

#[test]
fn every_shelf_is_listed_once() {
    let shelves = Shelf::list();
    assert_eq!(shelves.len(), 2 + Category::ALL.len());
    assert_eq!(shelves[0], Shelf::Recent);
    assert_eq!(shelves[1], Shelf::All);
}
