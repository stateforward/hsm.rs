// Event Queue (prioritizes completion events like JavaScript)

use std::collections::VecDeque;
use std::fmt;

use crate::context::Context;
use crate::error::Result;
use crate::event::Event;
use crate::kind::{self, is_kind};
use crate::runtime::RuntimeQueue;

enum RegularQueue {
    Default(VecDeque<Event>),
    Custom(RuntimeQueue),
}

pub struct EventQueue {
    completion_events: VecDeque<Event>,
    regular_events: RegularQueue,
}

impl fmt::Debug for EventQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventQueue")
            .field("completion_events", &self.completion_events)
            .field("regular_len", &self.len())
            .finish()
    }
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            completion_events: VecDeque::new(),
            regular_events: RegularQueue::Default(VecDeque::new()),
        }
    }

    pub fn with_regular_queue(queue: RuntimeQueue) -> Self {
        Self {
            completion_events: VecDeque::new(),
            regular_events: RegularQueue::Custom(queue),
        }
    }

    pub fn push(&mut self, event: Event) {
        let _ = self.push_with_context(&Context::new(), event);
    }

    pub fn push_with_context(&mut self, ctx: &Context, event: Event) -> Result<()> {
        if is_kind(event.kind, kind::COMPLETION_EVENT) {
            self.completion_events.push_back(event);
        } else {
            match &mut self.regular_events {
                RegularQueue::Default(events) => events.push_back(event),
                RegularQueue::Custom(queue) => queue.push(ctx, event)?,
            }
        }
        Ok(())
    }

    pub fn prepend_regular(&mut self, events: Vec<Event>) {
        let _ = self.prepend_regular_with_context(&Context::new(), events);
    }

    pub fn prepend_regular_with_context(
        &mut self,
        ctx: &Context,
        events: Vec<Event>,
    ) -> Result<()> {
        if matches!(self.regular_events, RegularQueue::Custom(_)) {
            for event in events {
                self.push_with_context(ctx, event)?;
            }
            return Ok(());
        }

        for event in events.into_iter().rev() {
            if is_kind(event.kind, kind::COMPLETION_EVENT) {
                self.completion_events.push_back(event);
            } else if let RegularQueue::Default(regular_events) = &mut self.regular_events {
                regular_events.push_front(event);
            }
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.pop_with_context(&Context::new()).ok().flatten()
    }

    pub fn pop_with_context(&mut self, ctx: &Context) -> Result<Option<Event>> {
        if let Some(event) = self.completion_events.pop_back() {
            return Ok(Some(event));
        }

        match &mut self.regular_events {
            RegularQueue::Default(events) => Ok(events.pop_front()),
            RegularQueue::Custom(queue) => queue.pop(ctx),
        }
    }

    pub fn len(&self) -> usize {
        self.len_with_context(&Context::new())
    }

    pub fn len_with_context(&self, ctx: &Context) -> usize {
        let regular_len = match &self.regular_events {
            RegularQueue::Default(events) => events.len(),
            RegularQueue::Custom(queue) => queue.len(ctx).unwrap_or(0),
        };
        self.completion_events.len() + regular_len
    }

    pub fn clear_with_context(&mut self, ctx: &Context) {
        self.completion_events.clear();

        match &mut self.regular_events {
            RegularQueue::Default(events) => events.clear(),
            RegularQueue::Custom(queue) => {
                while queue.len(ctx).unwrap_or(0) > 0 {
                    match queue.pop(ctx) {
                        Ok(Some(_)) => {}
                        _ => break,
                    }
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.clear_with_context(&Context::new());
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
