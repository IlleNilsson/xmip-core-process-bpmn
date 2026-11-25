//! A process's data objects, the values each Instance keeps of its own,
//! and a task that sets them (ADR-0066, amendment 2026-11-25; open
//! problem 191).
//!
//! ```xml
//! <bpmn:dataObject id="DeliveredData" name="Delivered"/>
//! <bpmn:dataObjectReference id="DeliveredRef" dataObjectRef="DeliveredData"/>
//! <bpmn:task id="Count" name="Count the delivery">
//!   <bpmn:dataOutputAssociation id="CountDelivered">
//!     <bpmn:targetRef>DeliveredRef</bpmn:targetRef>
//!     <bpmn:assignment>
//!       <bpmn:from language="urn:xmip:expression">Delivered + Quantity</bpmn:from>
//!       <bpmn:to>Delivered</bpmn:to>
//!     </bpmn:assignment>
//!   </bpmn:dataOutputAssociation>
//! </bpmn:task>
//! ```
//!
//! A data object anywhere in the process declares one value, by its name —
//! its identifier where it has none — and a name is one value. A task's data
//! output association whose target is a data object, or a reference to one,
//! keeps that value, computed from its assignment's `from` in Xmip's
//! expression language over the current Message and the Instance's own
//! values; one whose target is no data object assigns the Message's
//! context as before, a new Message generation. A task does one or the
//! other.

use process::program::{Keep, Step, Work};

use super::{Builder, Shaped, pass};
use crate::document::Element;
use crate::extension;
use crate::leads::Lead;
use crate::schema::MODEL;

impl<'d> Builder<'d> {
    /// `object` declared: an Instance value, by its name, else its id.
    pub(super) fn data_object(&mut self, object: &'d Element) {
        let Some(id) = object.attribute("id") else {
            self.refuse(object, "a data object needs an id");
            return;
        };
        let name = object
            .attribute("name")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(id);
        let number = u32::try_from(self.values.len()).unwrap_or(u32::MAX);
        self.values.push(name.trim().to_string());
        self.data.insert(id, number);
    }

    /// `reference` to a data object, resolved once every object is known.
    pub(super) fn data_reference(&mut self, reference: &'d Element) {
        if let (Some(id), Some(object)) = (
            reference.attribute("id"),
            reference.attribute("dataObjectRef"),
        ) {
            self.references.insert(id, object);
        }
    }

    /// The value `target` names: a data object's, or a reference's to one.
    fn value_of(&self, target: &str) -> Option<u32> {
        self.data.get(target).copied().or_else(|| {
            let object = self.references.get(target)?;
            self.data.get(object).copied()
        })
    }

    /// A task: values the Instance keeps where its data outputs target data
    /// objects, an Assignment of the Message's context where they target
    /// none, and a step that only goes on where it has neither.
    pub(super) fn task(&mut self, element: &'d Element) -> Option<Shaped<'d>> {
        let associations: Vec<&Element> = element
            .children_named(MODEL, "dataOutputAssociation")
            .collect();
        let (kept, assigned): (Vec<&Element>, Vec<&Element>) = associations
            .into_iter()
            .partition(|association| target(association).and_then(|t| self.value_of(t)).is_some());
        if !kept.is_empty() && !assigned.is_empty() {
            self.refuse(
                element,
                "a task keeps the Instance's values or assigns the Message's, not both: draw two",
            );
            return None;
        }
        if !assigned.is_empty() {
            let assignments = assigned.into_iter().flat_map(extension::assigned).collect();
            let work = Work::Assign(assignments);
            return Some((Step::Call { work, next: 0 }, Lead::Out("")));
        }
        let mut values = Vec::new();
        for association in kept {
            let value = target(association)
                .and_then(|t| self.value_of(t))
                .unwrap_or_default();
            let name = self.values[value as usize].clone();
            let computed = match extension::computed(element, association) {
                Ok(computed) => computed,
                Err(refused) => {
                    self.refusals.push(refused);
                    return None;
                }
            };
            for (from, to) in computed {
                if !to.is_empty() && to != name {
                    let why = format!("its assignment names '{to}', not '{name}', its target");
                    self.refuse(association, why);
                }
                values.push(Keep { value, from });
            }
        }
        if values.is_empty() {
            return Some(pass());
        }
        Some((Step::Keep { values, next: 0 }, Lead::Out("")))
    }
}

/// What a data output association targets: its `targetRef`, written as an
/// element, as the schema has it, or as an attribute, as some tools do.
fn target(association: &Element) -> Option<&str> {
    association
        .child(MODEL, "targetRef")
        .map(|target| target.text.trim())
        .or_else(|| association.attribute("targetRef"))
        .filter(|target| !target.is_empty())
}
