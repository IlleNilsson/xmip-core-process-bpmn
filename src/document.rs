//! A BPMN document read into elements: each with its namespace resolved,
//! its attributes, its text and where it stands in the bytes, so the
//! compiler reads a tree and a designer's edit can be placed exactly.
//!
//! Read with the estate's one XML reader, `codec::xml::tree`, which
//! resolves every namespace as an element opens. A namespace is a name,
//! compared and never fetched; a document type declaration is refused, so
//! no entity of anyone else's is ever expanded or asked for. The namespaces
//! an element declares stand first among its attributes, as `xmlns` or in
//! the `xmlns` namespace, where the editor looks for a prefix.

use codec::xml::tree::{Document, Node};

/// The namespace a namespace declaration is in (Namespaces in XML 1.0).
const XMLNS: &str = "http://www.w3.org/2000/xmlns/";

/// One element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    /// Its namespace, empty where it has none.
    pub namespace: String,
    /// Its local name.
    pub name: String,
    /// Its attributes: namespace, local name and value, in order.
    pub attributes: Vec<(String, String, String)>,
    pub children: Vec<Element>,
    /// The text directly inside it, entities read.
    pub text: String,
    /// Where it starts and ends in the document's bytes.
    pub start: usize,
    pub end: usize,
    /// Where its start tag ends and where its end tag starts: what it holds
    /// lies between. Both are its end where it is empty.
    pub open_end: usize,
    pub close_start: usize,
}

impl Element {
    /// The attribute `name` without a namespace, or `None`.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(namespace, local, _)| namespace.is_empty() && local == name)
            .map(|(_, _, value)| value.as_str())
    }

    /// The attribute `name` in `namespace`, or `None`.
    #[must_use]
    pub fn attribute_in(&self, namespace: &str, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(space, local, _)| space == namespace && local == name)
            .map(|(_, _, value)| value.as_str())
    }

    /// Whether it is `name` in `namespace`.
    #[must_use]
    pub fn is(&self, namespace: &str, name: &str) -> bool {
        self.namespace == namespace && self.name == name
    }

    /// Its children that are `name` in `namespace`.
    pub fn children_named<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Element> {
        self.children
            .iter()
            .filter(move |child| child.is(namespace, name))
    }

    /// Its first child that is `name` in `namespace`.
    #[must_use]
    pub fn child(&self, namespace: &str, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.is(namespace, name))
    }

    /// Its identifier, or its name where it has none.
    #[must_use]
    pub fn label(&self) -> &str {
        self.attribute("id").unwrap_or(&self.name)
    }
}

/// `text` read into its root element.
///
/// # Errors
/// The text is not well-formed XML, holds a document type declaration, or
/// names a prefix it never bound — each said with where.
pub fn read(text: &str) -> Result<Element, String> {
    let document = Document::parse(text)
        .map_err(|refused| format!("at byte {}: {}", refused.at, refused.message))?;
    Ok(element(document.root_element()))
}

/// The element `node` is, with every element inside it.
fn element(node: Node<'_>) -> Element {
    let declared = node.declarations().map(|(prefix, namespace)| {
        if prefix.is_empty() {
            (String::new(), "xmlns".to_string(), namespace.to_string())
        } else {
            (XMLNS.to_string(), prefix.to_string(), namespace.to_string())
        }
    });
    let attributes = node.attributes().map(|attribute| {
        (
            attribute.namespace().unwrap_or("").to_string(),
            attribute.local().to_string(),
            attribute.value().to_string(),
        )
    });
    let range = node.range();
    Element {
        namespace: node.namespace().unwrap_or("").to_string(),
        name: node.local().to_string(),
        attributes: declared.chain(attributes).collect(),
        children: node.elements().map(element).collect(),
        text: node.texts().collect(),
        start: range.start,
        end: range.end,
        open_end: node.open_end(),
        close_start: node.close_start(),
    }
}
