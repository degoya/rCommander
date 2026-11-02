//! Which markers and patterns an element names, gathered as usvg 0.49 gathers its presentation
//! attributes (`src/parser/svgtree/parse.rs:233-415`): the element's own attributes, then the
//! rules of the document's style sheets that match it, then its `style` attribute. The budget
//! does not resolve the cascade: every value any of the three declares counts, which can only
//! count more than usvg draws, never less.

use roxmltree::{Document, Node};
use simplecss::{AttributeOperator, Declaration, DeclarationTokenizer, Element, PseudoClass};
use simplecss::{Selector, StyleSheet};

pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// The properties that name a marker or a paint server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    Fill,
    Stroke,
    MarkerStart,
    MarkerMid,
    MarkerEnd,
}

impl Property {
    pub const ALL: [Property; 5] = [
        Property::Fill,
        Property::Stroke,
        Property::MarkerStart,
        Property::MarkerMid,
        Property::MarkerEnd,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn is_paint(self) -> bool {
        matches!(self, Property::Fill | Property::Stroke)
    }

    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "fill" => Property::Fill,
            "stroke" => Property::Stroke,
            "marker-start" => Property::MarkerStart,
            "marker-mid" => Property::MarkerMid,
            "marker-end" => Property::MarkerEnd,
            _ => return None,
        })
    }
}

/// What the values of one property on one element name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declared<'a> {
    /// The ids of `url(#id)` values.
    pub ids: Vec<&'a str>,
    /// A value was `inherit`: the parent's value applies too.
    pub inherits: bool,
    /// `context-fill` or `context-stroke`: the paint of the element a marker is drawn for.
    pub context: bool,
}

/// The properties one element declares, by [`Property::index`].
#[derive(Clone, Debug, Default)]
pub struct Own<'a> {
    pub declared: [Option<Declared<'a>>; 5],
}

impl<'a> Own<'a> {
    fn declare(&mut self, property: Property, value: &'a str) {
        let declared = self.declared[property.index()].get_or_insert_with(Declared::default);
        if property.is_paint() {
            // As usvg reads a paint (`svgtypes::Paint::from_str`).
            match svgtypes::Paint::from_str(value) {
                Ok(svgtypes::Paint::FuncIRI(id, _)) => declared.ids.push(id),
                Ok(svgtypes::Paint::Inherit) => declared.inherits = true,
                Ok(svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke) => {
                    declared.context = true;
                }
                _ => {}
            }
        } else if value.trim() == "inherit" {
            declared.inherits = true;
        } else if let Ok(link) = svgtypes::FuncIRI::from_str(value) {
            // A marker link (`FromValue for SvgNode`, `src/parser/svgtree/mod.rs:1073-1082`).
            declared.ids.push(link.0);
        }
    }

    /// A declaration as usvg writes it (`write_declaration`, `src/parser/svgtree/parse.rs:
    /// 346-398`): `marker` sets all three marker properties.
    fn write(&mut self, name: &str, value: &'a str) {
        if name == "marker" {
            for property in [
                Property::MarkerStart,
                Property::MarkerMid,
                Property::MarkerEnd,
            ] {
                self.declare(property, value);
            }
        } else if let Some(property) = Property::named(name) {
            self.declare(property, value);
        }
    }
}

/// The rules of the document's style sheets that declare one of the [`Property`]s or `marker`.
pub struct Styles<'a> {
    rules: Vec<(Selector<'a>, Vec<Declaration<'a>>)>,
}

impl<'a> Styles<'a> {
    /// The style sheets as usvg collects them (`resolve_css`, `src/parser/svgtree/parse.rs:
    /// 659-687`): every `<style>` without a `type` or of `text/css`, its first text.
    #[must_use]
    pub fn of(doc: &'a Document<'_>) -> Self {
        let mut sheet = StyleSheet::new();
        for node in doc.descendants().filter(|node| node.has_tag_name("style")) {
            match node.attribute("type") {
                Some("text/css") | None => {}
                Some(_) => continue,
            }
            if let Some(text) = node.text() {
                sheet.parse_more(text);
            }
        }
        let rules = sheet
            .rules
            .into_iter()
            .filter(|rule| {
                rule.declarations.iter().any(|declaration| {
                    declaration.name == "marker" || Property::named(declaration.name).is_some()
                })
            })
            .map(|rule| (rule.selector, rule.declarations))
            .collect();
        Self { rules }
    }
}

/// What `node` declares itself, from all three sources.
#[must_use]
pub fn own<'a>(node: Node<'a, '_>, styles: &Styles<'a>) -> Own<'a> {
    let mut own = Own::default();
    for attribute in node.attributes() {
        // The namespaces usvg copies attributes from (`src/parser/svgtree/parse.rs:235-238`).
        if matches!(
            attribute.namespace(),
            None | Some(SVG_NS) | Some(XLINK_NS) | Some(XML_NS)
        ) && let Some(property) = Property::named(attribute.name())
        {
            own.declare(property, attribute.value());
        }
    }
    for (selector, declarations) in &styles.rules {
        if selector.matches(&XmlNode(node)) {
            for declaration in declarations {
                own.write(declaration.name, declaration.value);
            }
        }
    }
    if let Some(style) = node.attribute("style") {
        for declaration in DeclarationTokenizer::from(style) {
            own.write(declaration.name, declaration.value);
        }
    }
    own
}

/// A node as usvg's selectors see it (`XmlNode`, `src/parser/svgtree/parse.rs:689-718`).
struct XmlNode<'a, 'input>(Node<'a, 'input>);

impl Element for XmlNode<'_, '_> {
    fn parent_element(&self) -> Option<Self> {
        self.0.parent_element().map(XmlNode)
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.0.prev_sibling_element().map(XmlNode)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.0.tag_name().name() == local_name
    }

    fn attribute_matches(&self, local_name: &str, operator: AttributeOperator<'_>) -> bool {
        self.0
            .attribute(local_name)
            .is_some_and(|value| operator.matches(value))
    }

    fn pseudo_class_matches(&self, class: PseudoClass<'_>) -> bool {
        matches!(class, PseudoClass::FirstChild) && self.prev_sibling_element().is_none()
    }
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod tests;
