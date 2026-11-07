//! The count behind [`crate::budget::expanded`]: every element as often as usvg 0.49 builds
//! it, and with it the cubic curves of its arcs (`segments.rs`, RC-100-78). A `<use>` copies
//! what it references; a shape draws the content of its markers once at the start, once at the
//! end and once at each vertex between (`src/parser/marker.rs:41-72, 216-262`); a painted shape,
//! and each glyph run of a text, takes a copy of the content of each pattern it is painted with
//! (`Paint::to_user_coordinates`, `src/parser/paint_server.rs:964-1010`). Markers and patterns
//! inside those, and their `<use>`, count the same way.
//!
//! Which markers and patterns apply is [`crate::refs`]' over-count, inherited as usvg inherits
//! (`find_attribute`, `src/parser/svgtree/mod.rs:369-391`), through a `<use>` from where the
//! `<use>` stands. usvg skips a marker inside its own content (`parent_markers`, `marker.rs:
//! 63-67`) and clears a pattern's own link to itself (`fix_recursive_patterns`,
//! `src/parser/svgtree/parse.rs:720-774`); both count nothing. Any other ring of markers and
//! patterns is refused: usvg would recurse into it without end, or draw it to a depth this count
//! cannot follow.
//!
//! A `textPath` makes usvg convert the shape it names once more (`resolve_text_flow`,
//! `src/parser/text.rs:365-367`): it counts that shape's curves, the largest of every element of
//! the id, since usvg links the last one (`src/parser/svgtree/parse.rs:116-123`).

use std::collections::HashMap;

use roxmltree::{Document, Node, NodeId};

use crate::budget::{Cost, iri};
use crate::refs::{Own, Property, Styles, own};
use crate::segments::{Shape, shape};

const XLINK: &str = "http://www.w3.org/1999/xlink";

/// The shapes usvg turns into paths and may put markers on (`convert_element_impl`,
/// `src/parser/converter.rs:678-689`).
const SHAPES: [&str; 7] = [
    "rect", "circle", "ellipse", "line", "polyline", "polygon", "path",
];
/// The elements whose text is painted.
const TEXTS: [&str; 4] = ["text", "tspan", "textPath", "tref"];

/// The target elements each property may name in a context, by [`Property::index`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Context([Vec<NodeId>; 5]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Task {
    /// An element in a context; `inside` is the marker or pattern whose content it is.
    Element {
        node: NodeId,
        context: usize,
        inside: Option<NodeId>,
    },
    /// The content of a marker, drawn once.
    Marker(NodeId),
    /// The content a pattern takes, through its `href`, copied once.
    Pattern(NodeId),
}

enum State {
    /// Entered and not left. A `<use>` back to it is a cycle, which usvg skips: it counts 0.
    Open,
    Done(Cost),
}

/// What a task is made of: its own cost, and each part with how often it is drawn there.
type Parts = (Cost, Vec<(u64, Task)>);

/// One element; a marker or a pattern task counts one as well.
const ONE: Cost = Cost {
    elements: 1,
    curves: 0,
};

/// A count beyond the cap: the document is too complex.
struct TooComplex;

/// The elements of `doc` and the curves of their arcs as usvg builds them, each up to its
/// `limit + 1`: a result that exceeds `limit` means "too many". Iterative, so a deep document
/// cannot overflow the stack here; each element is visited once per context it is drawn in, and
/// the visits are capped as well.
#[must_use]
pub fn count(doc: &Document, limit: Cost) -> Cost {
    let cap = Cost {
        elements: limit.elements.saturating_add(1),
        curves: limit.curves.saturating_add(1),
    };
    let mut count = Count::new(doc, cap);
    count.run().unwrap_or(cap)
}

struct Count<'a, 'input> {
    doc: &'a Document<'input>,
    styles: Styles<'a>,
    /// `<use>` targets, the first element of an id (`src/parser/svgtree/parse.rs:76-83`).
    ids: HashMap<&'a str, Node<'a, 'input>>,
    /// Markers and patterns by id, every element of an id: usvg links the last one
    /// (`src/parser/svgtree/parse.rs:116-123`), counting all is never less.
    markers: HashMap<&'a str, Vec<NodeId>>,
    patterns: HashMap<&'a str, Vec<NodeId>>,
    /// The shapes a `textPath` may name, every element of an id.
    shapes_by_id: HashMap<&'a str, Vec<NodeId>>,
    /// What each shape makes, read once however often it is visited.
    shapes: HashMap<NodeId, Shape>,
    contexts: Vec<Context>,
    interned: HashMap<Context, usize>,
    states: HashMap<Task, State>,
    pending: HashMap<Task, Parts>,
    cap: Cost,
    work: u64,
}

impl<'a, 'input> Count<'a, 'input> {
    fn new(doc: &'a Document<'input>, cap: Cost) -> Self {
        let mut ids = HashMap::new();
        let mut markers: HashMap<&str, Vec<NodeId>> = HashMap::new();
        let mut patterns: HashMap<&str, Vec<NodeId>> = HashMap::new();
        let mut shapes_by_id: HashMap<&str, Vec<NodeId>> = HashMap::new();
        for node in doc.descendants() {
            if let Some(id) = node.attribute("id") {
                ids.entry(id).or_insert(node);
            }
            let name = node.tag_name().name();
            let by_id = match name {
                "marker" => &mut markers,
                "pattern" => &mut patterns,
                _ if SHAPES.contains(&name) => &mut shapes_by_id,
                _ => continue,
            };
            for attribute in node
                .attributes()
                .filter(|attribute| attribute.name() == "id")
            {
                by_id.entry(attribute.value()).or_default().push(node.id());
            }
        }
        let empty = Context::default();
        Self {
            doc,
            styles: Styles::of(doc),
            ids,
            markers,
            patterns,
            shapes_by_id,
            shapes: HashMap::new(),
            contexts: vec![empty.clone()],
            interned: HashMap::from([(empty, 0)]),
            states: HashMap::new(),
            pending: HashMap::new(),
            cap,
            work: 0,
        }
    }

    fn run(&mut self) -> Result<Cost, TooComplex> {
        let root = Task::Element {
            node: self.doc.root_element().id(),
            context: 0,
            inside: None,
        };
        let mut stack = vec![(root, false)];
        while let Some((task, leaving)) = stack.pop() {
            if leaving {
                let (own, dependencies) = self.pending.remove(&task).unwrap_or_default();
                let total = dependencies.iter().fold(own, |sum, (times, dependency)| {
                    let each = match self.states.get(dependency) {
                        Some(State::Done(count)) => *count,
                        _ => Cost::default(),
                    };
                    Cost {
                        elements: sum
                            .elements
                            .saturating_add(times.saturating_mul(each.elements)),
                        curves: sum.curves.saturating_add(times.saturating_mul(each.curves)),
                    }
                });
                if total.elements >= self.cap.elements || total.curves >= self.cap.curves {
                    // Everything counted here is reached from the root, which counts as much.
                    return Err(TooComplex);
                }
                self.states.insert(task, State::Done(total));
                continue;
            }
            if self.states.contains_key(&task) {
                continue;
            }
            // Each visit adds at least one to the count it is reached from.
            self.work += 1;
            if self.work > self.cap.elements.saturating_mul(4) {
                return Err(TooComplex);
            }
            self.states.insert(task, State::Open);
            let (own, dependencies) = self.dependencies(task)?;
            stack.push((task, true));
            for &(_, dependency) in &dependencies {
                match self.states.get(&dependency) {
                    None => stack.push((dependency, false)),
                    Some(State::Open) if !matches!(dependency, Task::Element { .. }) => {
                        return Err(TooComplex);
                    }
                    Some(_) => {}
                }
            }
            self.pending.insert(task, (own, dependencies));
        }
        match self.states.get(&root) {
            Some(State::Done(count)) => Ok(*count),
            _ => Ok(Cost::default()),
        }
    }

    fn node(&self, id: NodeId) -> Option<Node<'a, 'input>> {
        self.doc.get_node(id)
    }

    /// What `task` is made of: its own cost, and each part with how often it is drawn there.
    fn dependencies(&mut self, task: Task) -> Result<Parts, TooComplex> {
        match task {
            Task::Element {
                node,
                context,
                inside,
            } => match self.node(node) {
                Some(node) => self.element(node, context, inside),
                None => Ok((ONE, Vec::new())),
            },
            Task::Marker(marker) => Ok((ONE, self.content(marker, marker))),
            Task::Pattern(pattern) => Ok((
                ONE,
                self.owners(pattern)
                    .into_iter()
                    .flat_map(|owner| self.content(owner, owner))
                    .collect(),
            )),
        }
    }

    fn element(
        &mut self,
        node: Node<'a, 'input>,
        context: usize,
        inside: Option<NodeId>,
    ) -> Result<Parts, TooComplex> {
        let own = own(node, &self.styles);
        let context = self.apply(context, &own);
        let element = |node: Node| Task::Element {
            node: node.id(),
            context,
            inside,
        };
        let mut dependencies: Vec<(u64, Task)> = node
            .children()
            .filter(Node::is_element)
            .map(|child| (1, element(child)))
            .collect();
        if let Some(target) = self.use_target(node) {
            dependencies.push((1, element(target)));
        }
        let name = node.tag_name().name();
        let mut curves = 0;
        let painted = if SHAPES.contains(&name) {
            let shape = self.shape(node);
            curves = shape.curves;
            let vertices = shape.segments;
            for (property, times) in [
                (Property::MarkerStart, 1),
                (Property::MarkerMid, vertices),
                (Property::MarkerEnd, 1),
            ] {
                for &marker in &self.contexts[context].0[property.index()] {
                    // usvg skips a marker inside its own content.
                    if Some(marker) != inside && times > 0 {
                        dependencies.push((times, Task::Marker(marker)));
                    }
                }
            }
            1
        } else if TEXTS.contains(&name) {
            if name == "textPath" {
                curves = self.followed_curves(node);
            }
            // A run of glyphs and up to three decoration lines per character, at most.
            let characters: usize = node
                .children()
                .filter_map(|child| child.text())
                .map(|text| text.chars().count())
                .sum();
            (characters as u64).saturating_add(1).saturating_mul(4)
        } else {
            0
        };
        if curves >= self.cap.curves {
            // One arc beyond its own limit, or more curves than the whole budget.
            return Err(TooComplex);
        }
        if painted > 0 {
            for property in [Property::Fill, Property::Stroke] {
                for &pattern in &self.contexts[context].0[property.index()] {
                    if Some(pattern) == inside {
                        // usvg clears a link of the pattern's own content to the pattern, but
                        // not one inherited from outside it, which it would follow forever.
                        if self.declares(&own, property, pattern) {
                            continue;
                        }
                        return Err(TooComplex);
                    }
                    dependencies.push((painted, Task::Pattern(pattern)));
                }
            }
        }
        Ok((
            Cost {
                elements: 1,
                curves,
            },
            dependencies,
        ))
    }

    /// What the shape `node` makes, read once.
    fn shape(&mut self, node: Node) -> Shape {
        *self.shapes.entry(node.id()).or_insert_with(|| shape(node))
    }

    /// The curves of the shape a `textPath` follows: of every element its `href` or `xlink:href`
    /// may name, the most.
    fn followed_curves(&mut self, node: Node<'a, 'input>) -> u64 {
        let targets: Vec<NodeId> = node
            .attributes()
            .filter(|attribute| {
                attribute.name() == "href" && matches!(attribute.namespace(), None | Some(XLINK))
            })
            .filter_map(|attribute| iri(attribute.value()))
            .flat_map(|id| self.shapes_by_id.get(id).into_iter().flatten().copied())
            .collect();
        let mut most = 0;
        for id in targets {
            if let Some(target) = self.node(id) {
                most = most.max(self.shape(target).curves);
            }
        }
        most
    }

    /// The element children of `owner` (a marker, or the pattern whose content a pattern takes),
    /// in the context its own place gives them.
    fn content(&mut self, owner: NodeId, inside: NodeId) -> Vec<(u64, Task)> {
        let Some(node) = self.node(owner) else {
            return Vec::new();
        };
        let mut context = 0;
        let mut chain: Vec<Node> = node.ancestors().filter(Node::is_element).collect();
        chain.reverse();
        for ancestor in chain {
            let own = own(ancestor, &self.styles);
            context = self.apply(context, &own);
        }
        node.children()
            .filter(Node::is_element)
            .map(|child| {
                (
                    1,
                    Task::Element {
                        node: child.id(),
                        context,
                        inside: Some(inside),
                    },
                )
            })
            .collect()
    }

    /// The patterns whose content `pattern` takes: the first of its `href` chain with element
    /// children (`find_pattern_with_children`, `src/parser/paint_server.rs:260-279`), every one
    /// that may be it when an id names several.
    fn owners(&self, pattern: NodeId) -> Vec<NodeId> {
        let mut owners = Vec::new();
        let mut seen = vec![pattern];
        let mut next = vec![pattern];
        while let Some(id) = next.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if node.children().any(|child| child.is_element()) {
                owners.push(id);
                continue;
            }
            let links = node
                .attributes()
                .filter(|attribute| {
                    attribute.name() == "href"
                        && matches!(attribute.namespace(), None | Some(XLINK))
                })
                .filter_map(|attribute| iri(attribute.value()));
            for link in links {
                for &target in self.patterns.get(link).into_iter().flatten() {
                    if !seen.contains(&target) {
                        seen.push(target);
                        next.push(target);
                    }
                }
            }
        }
        owners
    }

    /// Whether `own` names `target` for `property` itself, or a context paint, which has no
    /// element to take its paint from inside a pattern and stands for any pattern here.
    fn declares(&self, own: &Own, property: Property, target: NodeId) -> bool {
        own.declared[property.index()]
            .as_ref()
            .is_some_and(|declared| {
                declared.context
                    || declared
                        .ids
                        .iter()
                        .any(|id| self.targets(property, id).contains(&target))
            })
    }

    fn targets(&self, property: Property, id: &str) -> &[NodeId] {
        let by_id = if property.is_paint() {
            &self.patterns
        } else {
            &self.markers
        };
        by_id.get(id).map_or(&[], Vec::as_slice)
    }

    /// The context of the children of an element that declares `own` in `context`.
    fn apply(&mut self, context: usize, own: &Own) -> usize {
        if own.declared.iter().all(Option::is_none) {
            return context;
        }
        let mut next = self.contexts[context].clone();
        for property in Property::ALL {
            let Some(declared) = &own.declared[property.index()] else {
                continue;
            };
            let mut set: Vec<NodeId> = declared
                .ids
                .iter()
                .flat_map(|id| self.targets(property, id).iter().copied())
                .collect();
            if declared.inherits {
                set.extend_from_slice(&self.contexts[context].0[property.index()]);
            }
            if declared.context && property.is_paint() {
                // The paint of whatever the marker is drawn for: any pattern.
                set.extend(self.patterns.values().flatten().copied());
            }
            set.sort_unstable_by_key(|id| id.get());
            set.dedup();
            next.0[property.index()] = set;
        }
        if let Some(&id) = self.interned.get(&next) {
            return id;
        }
        self.contexts.push(next.clone());
        self.interned.insert(next, self.contexts.len() - 1);
        self.contexts.len() - 1
    }

    /// For a `<use>`, the element its `href` or `xlink:href` names in this document. As usvg
    /// resolves it (`resolve_href`, `src/parser/svgtree/parse.rs:558-578`): an unprefixed `href`
    /// wins over `xlink:href` in any order, and an `href` in another namespace is none.
    fn use_target(&self, node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
        if node.tag_name().name() != "use" {
            return None;
        }
        let href = |namespace: Option<&str>| {
            node.attributes()
                .find(|attribute| attribute.name() == "href" && attribute.namespace() == namespace)
                .map(|attribute| attribute.value())
        };
        href(None)
            .or_else(|| href(Some(XLINK)))
            .and_then(iri)
            .and_then(|id| self.ids.get(id).copied())
    }
}

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
