use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use rust::Context;
use rust::event::Event;
use rust::kind;
use rust::queue::*;
use rust::{Queue, RuntimeQueue};

#[test]
fn test_queue_creation() {
    let queue = EventQueue::new();
    assert!(queue.is_empty());
}

#[test]
fn test_queue_push_pop_regular_events() {
    let mut queue = EventQueue::new();

    // Add regular events
    queue.push(Event::new("event1"));
    queue.push(Event::new("event2"));
    queue.push(Event::new("event3"));

    assert!(!queue.is_empty());

    // Pop in FIFO order
    assert_eq!(queue.pop().unwrap().name, "event1");
    assert_eq!(queue.pop().unwrap().name, "event2");
    assert_eq!(queue.pop().unwrap().name, "event3");

    assert!(queue.is_empty());
    assert!(queue.pop().is_none());
}

#[test]
fn test_queue_completion_event_priority() {
    let mut queue = EventQueue::new();

    // Add mix of regular and completion events
    queue.push(Event::new("regular1"));
    queue.push(Event::completion("completion1"));
    queue.push(Event::new("regular2"));
    queue.push(Event::completion("completion2"));
    queue.push(Event::new("regular3"));

    // Completion events should come out first, in LIFO order
    assert_eq!(queue.pop().unwrap().name, "completion2");
    assert_eq!(queue.pop().unwrap().name, "completion1");

    // Then regular events in FIFO order
    assert_eq!(queue.pop().unwrap().name, "regular1");
    assert_eq!(queue.pop().unwrap().name, "regular2");
    assert_eq!(queue.pop().unwrap().name, "regular3");

    assert!(queue.is_empty());
}

#[test]
fn test_queue_error_event_priority() {
    let mut queue = EventQueue::new();

    // Error events are completion events, so should have priority
    queue.push(Event::new("regular1"));
    queue.push(Event::error_event());
    queue.push(Event::new("regular2"));

    // Error event should come first
    let first = queue.pop().unwrap();
    assert_eq!(first.name, "hsm_error");
    assert_eq!(first.kind, kind::ERROR_EVENT);

    // Then regular events
    assert_eq!(queue.pop().unwrap().name, "regular1");
    assert_eq!(queue.pop().unwrap().name, "regular2");
}

#[test]
fn test_custom_regular_queue_keeps_runtime_completion_priority() {
    let ctx = Context::new();
    let regular_events = Arc::new(Mutex::new(VecDeque::new()));
    let pushed = Arc::new(Mutex::new(Vec::new()));

    let push_regular = regular_events.clone();
    let push_log = pushed.clone();
    let pop_regular = regular_events.clone();
    let len_regular = regular_events.clone();
    let custom: RuntimeQueue = Queue(
        Arc::new(move |_ctx, event| {
            push_log.lock().unwrap().push(event.name.clone());
            push_regular.lock().unwrap().push_back(event);
            Ok(())
        }),
        Arc::new(move |_ctx| Ok(pop_regular.lock().unwrap().pop_front())),
        Arc::new(move |_ctx| Ok(len_regular.lock().unwrap().len())),
    );

    let mut queue = EventQueue::with_regular_queue(custom);
    queue
        .push_with_context(&ctx, Event::new("regular1"))
        .unwrap();
    queue
        .push_with_context(&ctx, Event::completion("completion1"))
        .unwrap();
    queue
        .push_with_context(&ctx, Event::new("regular2"))
        .unwrap();

    assert_eq!(*pushed.lock().unwrap(), vec!["regular1", "regular2"]);
    assert_eq!(queue.len_with_context(&ctx), 3);
    assert_eq!(
        queue.pop_with_context(&ctx).unwrap().unwrap().name,
        "completion1"
    );
    assert_eq!(
        queue.pop_with_context(&ctx).unwrap().unwrap().name,
        "regular1"
    );
    assert_eq!(
        queue.pop_with_context(&ctx).unwrap().unwrap().name,
        "regular2"
    );
    assert!(queue.pop_with_context(&ctx).unwrap().is_none());
}
