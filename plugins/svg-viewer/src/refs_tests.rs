use roxmltree::Document;

use super::*;

/// `check` on what the first element named `name` declares.
fn with(svg: &str, name: &str, check: impl FnOnce(&Own)) {
    let doc = Document::parse(svg).expect("the test SVG parses");
    let styles = Styles::of(&doc);
    let node = doc
        .descendants()
        .find(|node| node.has_tag_name(name))
        .expect("the element");
    check(&own(node, &styles));
}

fn ids<'a>(own: &Own<'a>, property: Property) -> Vec<&'a str> {
    own.declared[property.index()]
        .as_ref()
        .map(|declared| declared.ids.clone())
        .unwrap_or_default()
}

#[test]
fn attributes_style_sheets_and_the_style_attribute_all_count() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <style>.a { marker-mid: url(#css) } path { fill: url(#sheet) }</style>
        <path class="a" fill="url(#attr) red" style="marker: url(#style); stroke: inherit"/>
    </svg>"##;
    with(svg, "path", |own| {
        assert_eq!(ids(own, Property::Fill), ["attr", "sheet"]);
        assert_eq!(ids(own, Property::MarkerStart), ["style"]);
        assert_eq!(ids(own, Property::MarkerMid), ["css", "style"]);
        assert_eq!(ids(own, Property::MarkerEnd), ["style"]);
        assert!(
            own.declared[Property::Stroke.index()]
                .as_ref()
                .is_some_and(|stroke| stroke.inherits)
        );
    });
}

#[test]
fn a_rule_counts_only_where_its_selector_matches_and_its_sheet_is_css() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <style>#x { marker-end: url(#m) }</style>
        <style type="text/plain">line { marker-end: url(#n) }</style>
        <line id="y"/><line id="x"/>
    </svg>"##;
    let doc = Document::parse(svg).expect("the test SVG parses");
    let styles = Styles::of(&doc);
    let line = |id: &str| {
        doc.descendants()
            .find(|node| node.attribute("id") == Some(id))
            .expect("the line")
    };
    let end = Property::MarkerEnd.index();
    assert!(own(line("y"), &styles).declared[end].is_none());
    assert_eq!(ids(&own(line("x"), &styles), Property::MarkerEnd), ["m"]);
}

#[test]
fn context_paints_and_values_without_a_link_name_nothing() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <path fill="context-stroke" stroke="red" marker-start="none" marker-end="#m"/>
    </svg>"##;
    with(svg, "path", |own| {
        assert!(
            own.declared[Property::Fill.index()]
                .as_ref()
                .is_some_and(|fill| fill.context)
        );
        assert!(ids(own, Property::Stroke).is_empty());
        assert!(ids(own, Property::MarkerStart).is_empty());
        // A marker is linked by `url(#m)` only.
        assert!(ids(own, Property::MarkerEnd).is_empty());
    });
}
