//! Guards the rule that every control of a dialog is a stop of its focus ring (RC-080-16,
//! `crates/rcommander/src/theme/focus.rs`): Tab reaches it, it shows the ring, and Space or the
//! arrows work it as in a native Windows dialog. A view under `crates/rcommander/src/app/` wraps
//! each control in a stop, counted per kind and per file (RC-080-54), in the code without its
//! comments and texts:
//!
//! - every `checkbox(` and `toggler(` a toggle stop;
//! - every `pick_list(` a pick stop - a choice stop has no arrows that choose an option, so a
//!   list's or a tree's does not stand in for one;
//! - a group of `radio(` buttons, every `slider(` and, in a view with any stop, every
//!   `mouse_area(` that is pressed (a button drawn by hand) one stop more of the choice or
//!   button kind, or a pick stop no pick list takes. A pressed `mouse_area` that is no control
//!   of the ring - an overlay that closes a menu, a row of a list that is one stop - carries
//!   [`POINTER_ONLY`] with its reason on its line or the one above.
//!
//! Buttons are stops through `theme::layout`'s helpers; the dialogs' harness test walks the
//! rings (`app/focus_ring_tests.rs`). Test code is not checked (`*_tests.rs`).

use std::path::Path;

use crate::scan::{calls, chain_has, code_only, line_of};
use crate::{files_with, relative, workspace_root};

/// Views whose controls are no dialog's, each with its reason.
const EXEMPT: &[(&str, &str)] = &[(
    "crates/rcommander/src/app/viewer/draw.rs",
    "the Lister's search bar: no dialog card, its keys are the Lister's own",
)];

/// The marker of a pressed `mouse_area` that is no stop on purpose:
/// `// focus-ring: pointer only (<reason>)`.
const POINTER_ONLY: &str = "focus-ring: pointer only";

/// The calls that make each kind of stop.
const TOGGLE_STOPS: &[&str] = &["focus::toggle(", "Stop::toggle("];
const PICK_STOPS: &[&str] = &["focus::pick(", "Stop::pick("];
const CHOICE_STOPS: &[&str] = &["focus::choice(", "Stop::choice("];
const BUTTON_STOPS: &[&str] = &["focus::button(", "Stop::button("];

/// What a `mouse_area` is pressed with: a button drawn by hand.
const PRESSED: &[&str] = &[".on_press(", ".on_double_click("];

/// Where each of `kinds` is called in `code`, as one sorted list.
fn sites(code: &str, kinds: &[&str]) -> Vec<usize> {
    let mut sites: Vec<usize> = kinds.iter().flat_map(|kind| calls(code, kind)).collect();
    sites.sort_unstable();
    sites
}

/// The pressed `mouse_area`s of `code` that carry no [`POINTER_ONLY`] marker in `source`.
fn pressed_mouse_areas(source: &str, code: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let marked = |line: usize| {
        let at = |index: usize| {
            lines
                .get(index)
                .is_some_and(|line| line.contains(POINTER_ONLY))
        };
        at(line - 1) || (line >= 2 && at(line - 2))
    };
    calls(code, "mouse_area(")
        .into_iter()
        .filter(|at| chain_has(code, *at, code.len(), PRESSED))
        .filter(|at| !marked(line_of(code, *at)))
        .collect()
}

/// `path:line` of the first site and every line, for a message.
fn place(path: &str, code: &str, sites: &[usize]) -> String {
    let lines: Vec<String> = sites
        .iter()
        .map(|at| line_of(code, *at).to_string())
        .collect();
    let first = lines.first().map_or("1", String::as_str);
    format!("{path}:{first} (lines {})", lines.join(", "))
}

/// What is wrong with `source` at `path`, one line per kind short of stops.
fn findings(path: &str, source: &str) -> Vec<String> {
    let code = code_only(source);
    let toggled = sites(&code, &["checkbox(", "toggler("]);
    let lists = sites(&code, &["pick_list("]);
    let radios = sites(&code, &["radio("]);
    let sliders = sites(&code, &["slider(", "vertical_slider("]);
    let toggles = sites(&code, TOGGLE_STOPS).len();
    let picks = sites(&code, PICK_STOPS).len();
    let choices = sites(&code, CHOICE_STOPS).len();
    let buttons = sites(&code, BUTTON_STOPS).len();
    let in_a_ring = toggles + picks + choices + buttons > 0;
    let pressed = if in_a_ring {
        pressed_mouse_areas(source, &code)
    } else {
        Vec::new()
    };
    let mut wrong = Vec::new();
    if toggled.len() > toggles {
        wrong.push(format!(
            "{}: {} checkbox(es) and toggler(s), {toggles} toggle stop(s)",
            place(path, &code, &toggled),
            toggled.len()
        ));
    }
    if lists.len() > picks {
        wrong.push(format!(
            "{}: {} pick list(s), {picks} pick stop(s)",
            place(path, &code, &lists),
            lists.len()
        ));
    }
    let mut hand_made: Vec<usize> = radios.first().copied().into_iter().collect();
    hand_made.extend(&sliders);
    hand_made.extend(&pressed);
    hand_made.sort_unstable();
    let spare_picks = picks.saturating_sub(lists.len());
    let other_stops = choices + buttons + spare_picks;
    if hand_made.len() > other_stops {
        wrong.push(format!(
            "{}: {} radio group(s), {} slider(s), {} pressed mouse_area(s), {other_stops} choice, \
             button or spare pick stop(s)",
            place(path, &code, &hand_made),
            usize::from(!radios.is_empty()),
            sliders.len(),
            pressed.len()
        ));
    }
    wrong
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
}

#[test]
fn every_dialog_control_is_a_stop_of_the_focus_ring() {
    let root = workspace_root();
    let mut files = Vec::new();
    files_with(
        &root.join("crates/rcommander/src/app"),
        &[".rs"],
        &mut files,
    );
    assert!(!files.is_empty(), "no views found");
    let mut wrong = Vec::new();
    for path in files.iter().filter(|path| !is_test_file(path)) {
        let shown = relative(&root, path);
        if EXEMPT.iter().any(|(exempt, _)| *exempt == shown) {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("a readable source");
        wrong.extend(findings(&shown, &source));
    }
    assert!(
        wrong.is_empty(),
        "controls outside the focus ring - wrap each in theme::focus (toggle, pick, choice, \
         button), or mark a pressed mouse_area that is no control \
         `// {POINTER_ONLY} (<reason>)`:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_count_sees_calls_not_longer_names() {
    let source = "checkbox(on); my_checkbox(on); widget::checkbox(off); focus::toggle(x)";
    assert_eq!(calls(source, "checkbox(").len(), 2);
    assert!(!findings("a.rs", source).is_empty(), "two boxes, one stop");
    assert!(findings("b.rs", "radio(a); focus::pick(group)").is_empty());
    assert!(!findings("c.rs", "radio(a)").is_empty());
}

/// The gaps of the count per file (RC-080-54): a list's choice stop stood in for a pick list, a
/// stop named in a comment counted, and togglers, sliders and hand-drawn buttons were not seen.
#[test]
fn each_kind_needs_its_own_stop() {
    let list_for_pick = "pick_list(a); focus::pick(a); pick_list(b); focus::choice(rows)";
    let found = findings("v.rs", list_for_pick);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("v.rs:1 "), "{found:?}");
    assert!(!findings("v.rs", "pick_list(a); // focus::pick(a)").is_empty());
    assert!(!findings("v.rs", "toggler(on)").is_empty());
    assert!(!findings("v.rs", "slider(0..=9, 3, m)").is_empty());
    assert!(findings("v.rs", "slider(0..=9, 3, m); focus::choice(s)").is_empty());

    let button = "focus::toggle(x);\nlet b = mouse_area(face)\n    .on_press(m);\n";
    let found = findings("v.rs", button);
    assert!(found[0].starts_with("v.rs:2 "), "{found:?}");
    let bound = "focus::toggle(x);\nlet b = mouse_area(face);\nb.on_press(m);\n";
    assert!(
        !findings("v.rs", bound).is_empty(),
        "followed to its binding"
    );
    let marked = "focus::toggle(x);\n// focus-ring: pointer only (closes the menu)\n\
                  mouse_area(l).on_press(m)";
    assert!(findings("v.rs", marked).is_empty());
    let scrolled = "focus::choice(rows);\nmouse_area(list).on_scroll(s)";
    assert!(findings("v.rs", scrolled).is_empty(), "not pressed");
    let row = "focus::choice(rows);\nmouse_area(row).on_press(click)";
    assert!(
        findings("v.rs", row).is_empty(),
        "the row of a list that is one stop"
    );
    let no_ring = "mouse_area(cell).on_press(click)";
    assert!(
        findings("v.rs", no_ring).is_empty(),
        "a view outside any ring"
    );
}
