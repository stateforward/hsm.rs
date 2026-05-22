// Event Queue (prioritizes completion events like JavaScript)

use std::collections::VecDeque;

use crate::event::Event;
use crate::kind::{self, is_kind};

#[derive(Debug)]
pub struct EventQueue {
    completion_events: VecDeque<Event>,
    events: VecDeque<Event>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            completion_events: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    pub fn push(&mut self, event: Event) {
        if is_kind(event.kind, kind::COMPLETION_EVENT) {
            self.completion_events.push_back(event);
        } else {
            self.events.push_back(event);
        }
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.completion_events
            .pop_front()
            .or_else(|| self.events.pop_front())
    }

    pub fn len(&self) -> usize {
        self.completion_events.len() + self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
