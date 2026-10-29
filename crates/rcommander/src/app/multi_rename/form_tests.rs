use rc_engine::{CaseMode, CounterSettings, RenameRule, SearchReplace};

use super::{CaseChoice, Field, Flag, Form, Placeholder};

fn full_rule() -> RenameRule {
    RenameRule {
        name_mask: "[Y]-[M]-[D]_[C:3]".to_owned(),
        extension_mask: "[L][E]".to_owned(),
        case: CaseMode::FirstUpper,
        search: Some(SearchReplace {
            find: "(\\d+)".to_owned(),
            replace: "#$1".to_owned(),
            regex: true,
            case_sensitive: true,
            first_only: true,
            include_extension: true,
            substitute: true,
        }),
        counter: CounterSettings {
            start: -5,
            step: 10,
            width: 4,
        },
    }
}

#[test]
fn the_default_form_is_total_commanders_default() {
    let rule = Form::default().rule().expect("reads");
    assert_eq!(rule.name_mask, "[N]");
    assert_eq!(rule.extension_mask, "[E]");
    assert_eq!(rule.case, CaseMode::Unchanged);
    assert_eq!(rule.counter, CounterSettings::default());
    assert_eq!(rule.search.map(|search| search.find), Some(String::new()));
}

#[test]
fn a_rule_restores_every_field_and_reads_back_the_same() {
    let form = Form::from_rule(&full_rule());
    assert_eq!(form.name_mask, "[Y]-[M]-[D]_[C:3]");
    assert_eq!(form.counter_start, "-5");
    assert!(form.regex && form.first_only && form.with_extension && form.substitute);
    assert_eq!(form.case, CaseChoice(CaseMode::FirstUpper));
    assert_eq!(form.rule().expect("reads"), full_rule());
}

#[test]
fn every_input_changes_the_rule() {
    let mut form = Form::default();
    *form.text_mut(Field::Find).expect("a rule field") = "ae|oe".to_owned();
    *form.text_mut(Field::Replace).expect("a rule field") = "a|o".to_owned();
    *form.flag_mut(Flag::FirstOnly) = true;
    form.case = CaseChoice(CaseMode::Upper);
    *form.text_mut(Field::CounterWidth).expect("a rule field") = " 3 ".to_owned();
    let rule = form.rule().expect("reads");
    let search = rule.search.expect("a search");
    assert_eq!(
        (search.find.as_str(), search.replace.as_str()),
        ("ae|oe", "a|o")
    );
    assert!(search.first_only);
    assert_eq!(rule.case, CaseMode::Upper);
    assert_eq!(rule.counter.width, 3);
    assert!(form.text_mut(Field::PresetName).is_none());
}

#[test]
fn a_counter_that_is_no_number_names_its_field() {
    for (field, text) in [
        (Field::CounterStart, "one"),
        (Field::CounterStep, ""),
        (Field::CounterWidth, "-1"),
    ] {
        let mut form = Form::default();
        *form.text_mut(field).expect("a rule field") = text.to_owned();
        assert_eq!(form.rule(), Err(field), "{text:?}");
    }
}

#[test]
fn a_placeholder_goes_into_the_mask_typed_in_last() {
    let mut form = Form::default();
    form.insert(Placeholder::Counter);
    assert_eq!(form.name_mask, "[N][C]");
    form.text_mut(Field::ExtensionMask);
    form.insert(Placeholder::Upper);
    assert_eq!(form.extension_mask, "[E][U]");
    form.insert(Placeholder::NameRange);
    assert_eq!(form.extension_mask, "[E][U][N1-]");
}

#[test]
fn the_case_cycles() {
    assert_eq!(
        CaseChoice(CaseMode::WordsUpper).next(),
        CaseChoice(CaseMode::Unchanged)
    );
}
