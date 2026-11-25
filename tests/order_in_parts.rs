//! An order delivered in parts, the example design
//! `design/order-in-parts.bpmn`, compiled and turned Message by Message
//! (open problems 191 and 194): its Instance keeps what was ordered,
//! delivered and accepted across every Message, its subprocess loops back
//! to its wait for the next delivery or the client's answer — accept,
//! refuse or return — until every line is accepted, a Message of another
//! order is not its, and thirty days after the order its deadline raises it
//! overdue.

use process::program::Step;
use process::{Act, Arrival, Compiled, DesignFormat, Event, Flow, Instance, NoResources, Outcome};
use process::{Refusal, State};
use xmip_core_process_bpmn::Bpmn;

const ORDER_IN_PARTS: &str = include_str!("design/order-in-parts.bpmn");

/// Thirty days, the design's deadline, in nanoseconds.
const THIRTY_DAYS: i128 = 30 * 24 * 3_600 * 1_000_000_000;

fn flow() -> Compiled {
    let program = Bpmn
        .compile(ORDER_IN_PARTS, &NoResources)
        .expect("compiles");
    Compiled::new(program).expect("opens")
}

/// A Message of the order `order`, carrying `values`.
fn message(id: u128, order: &str, values: &[(&str, &str)]) -> Event {
    Event::Arrived(Arrival {
        message: id,
        values: values
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect(),
        keys: vec![(0, order.to_string())],
    })
}

fn delivery(id: u128, lines: &str) -> Event {
    message(
        id,
        "A-1001",
        &[("MessageType", "Delivery"), ("Lines", lines)],
    )
}

fn answer(id: u128, said: &str, lines: &str) -> Event {
    message(
        id,
        "A-1001",
        &[
            ("MessageType", "Answer"),
            ("Answer", said),
            ("Lines", lines),
        ],
    )
}

/// What `instance` keeps, Ordered, Delivered and Accepted.
fn counted(instance: &Instance) -> [Option<&str>; 3] {
    ["Ordered", "Delivered", "Accepted"].map(|name| instance.value(name))
}

#[test]
fn the_design_declares_its_values_and_reads_only_the_messages_own() {
    let flow = flow();
    let program = flow.program();
    assert_eq!(program.values, ["Ordered", "Delivered", "Accepted"]);
    assert_eq!(program.sets, ["Order"]);
    let mut reads = flow.declared().reads.clone();
    reads.sort();
    assert_eq!(reads, ["Answer", "Lines", "MessageType"]);
    let keeps = program
        .nodes
        .iter()
        .filter(|node| matches!(node.step, Step::Keep { .. }))
        .count();
    assert_eq!(keeps, 5, "the order noted, and four counts");
}

#[test]
fn an_order_loops_back_to_its_wait_until_every_line_is_delivered_and_accepted() {
    let flow = flow();
    let mut instance = Instance::new();
    let order = message(1, "A-1001", &[("MessageType", "Order"), ("Lines", "3")]);
    let acts = flow.advance(&mut instance, &order, 0).expect("starts");
    assert!(acts.is_empty(), "it waits: {acts:?}");
    assert_eq!(counted(&instance), [Some("3"), Some("0"), Some("0")]);
    assert_eq!(
        instance.state(),
        State::Waiting {
            message: true,
            until: Some(THIRTY_DAYS)
        },
        "for the next Message of its order, until its deadline"
    );

    let mut turn = |event: &Event| flow.advance(&mut instance, event, 1).expect("taken");
    assert!(turn(&delivery(2, "2")).is_empty(), "two lines delivered");
    assert_eq!(
        turn(&answer(3, "refuse", "1")),
        vec![Act::Publish { message: 3 }],
        "one refused is delivered again"
    );
    assert!(turn(&delivery(4, "2")).is_empty());
    assert!(turn(&answer(5, "accept", "2")).is_empty());
    assert_eq!(
        turn(&answer(6, "return", "1")),
        vec![Act::Publish { message: 6 }],
        "one returned is credited"
    );
    assert!(turn(&message(7, "A-1001", &[("MessageType", "Note")])).is_empty());
    assert!(turn(&delivery(8, "1")).is_empty());
    assert_eq!(
        counted(&instance),
        [Some("3"), Some("3"), Some("1")],
        "counted across every Message"
    );
    assert_eq!(
        flow.advance(&mut instance, &message(9, "B-2002", &[]), 1),
        Err(process::Refused::NotWaiting),
        "another order's Message is not its"
    );
    let acts = flow
        .advance(&mut instance, &answer(10, "accept", "2"), 1)
        .expect("the last accepted");
    assert_eq!(
        acts,
        vec![Act::Publish { message: 10 }, Act::Ended(Outcome::Completed)],
        "every line accepted: the order confirmed"
    );
    assert_eq!(instance.state(), State::Completed);
    assert!(instance.timers.is_empty(), "its deadline stopped with it");
}

#[test]
fn thirty_days_after_the_order_its_deadline_raises_it_overdue() {
    let flow = flow();
    let mut instance = Instance::new();
    let order = message(1, "A-1001", &[("MessageType", "Order"), ("Lines", "3")]);
    flow.advance(&mut instance, &order, 0).expect("starts");
    flow.advance(&mut instance, &delivery(2, "1"), 5)
        .expect("a part");
    assert_eq!(
        instance.next_due(),
        Some(THIRTY_DAYS),
        "the deadline is the order's, not each wait's"
    );
    assert!(
        flow.advance(&mut instance, &Event::Due, THIRTY_DAYS - 1)
            .expect("early")
            .is_empty()
    );
    let acts = flow
        .advance(&mut instance, &Event::Due, THIRTY_DAYS)
        .expect("due");
    let [Act::Ended(Outcome::Failed { code, reason })] = acts.as_slice() else {
        panic!("{acts:?}");
    };
    assert_eq!(code, "overdue");
    assert!(reason.contains("Raised"), "{reason}");
    assert_eq!(instance.state().word(), "failed");
}

/// A design of one process, `body` its content, and its data objects.
fn wrapped(body: &str) -> String {
    format!(
        "<bpmn:definitions xmlns:bpmn=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" \
         id=\"D\" targetNamespace=\"urn:example\">\
         <bpmn:process id=\"P\" isExecutable=\"true\">\
         <bpmn:dataObject id=\"CountData\" name=\"Count\"/>\
         <bpmn:dataObjectReference id=\"CountRef\" dataObjectRef=\"CountData\"/>\
         <bpmn:startEvent id=\"S\"/><bpmn:endEvent id=\"E\"/>\
         <bpmn:sequenceFlow id=\"F1\" sourceRef=\"S\" targetRef=\"T\"/>\
         <bpmn:sequenceFlow id=\"F2\" sourceRef=\"T\" targetRef=\"E\"/>{body}\
         </bpmn:process></bpmn:definitions>"
    )
}

fn refused(body: &str) -> Vec<Refusal> {
    Bpmn.compile(&wrapped(body), &NoResources)
        .expect_err("refused")
}

#[test]
fn a_task_keeps_values_or_assigns_the_messages_and_says_what_it_cannot_keep() {
    let keeping = |from: &str, to: &str| {
        format!(
            "<bpmn:dataOutputAssociation id=\"K\"><bpmn:targetRef>CountRef</bpmn:targetRef>\
             <bpmn:assignment><bpmn:from>{from}</bpmn:from><bpmn:to>{to}</bpmn:to>\
             </bpmn:assignment></bpmn:dataOutputAssociation>"
        )
    };
    let assigning = "<bpmn:dataOutputAssociation id=\"A\"><bpmn:assignment>\
                     <bpmn:from>'x'</bpmn:from><bpmn:to>Status</bpmn:to>\
                     </bpmn:assignment></bpmn:dataOutputAssociation>";
    let task = |inner: &str| format!("<bpmn:task id=\"T\">{inner}</bpmn:task>");

    let program = Bpmn
        .compile(
            &wrapped(&task(&keeping("coalesce(Count, 0) + 1", "Count"))),
            &NoResources,
        )
        .expect("compiles");
    assert_eq!(program.values, ["Count"]);

    let both = refused(&task(&format!("{}{assigning}", keeping("1", "Count"))));
    assert!(
        both.iter()
            .any(|r| r.at == "T" && r.why.contains("not both")),
        "{both:?}"
    );
    let named = refused(&task(&keeping("1", "Total")));
    assert!(
        named
            .iter()
            .any(|r| r.why.contains("names 'Total', not 'Count'")),
        "{named:?}"
    );
    let broken = refused(&task(&keeping("Count +", "Count")));
    assert!(
        broken.iter().any(|r| r.why.contains("keeps a value that")),
        "{broken:?}"
    );
}
