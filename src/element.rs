// Element types and traits

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::context::Context;
use crate::event::Event;
use crate::kind::KindValue;
use crate::path::{basename, dirname};

// Core Types with Exact Function Signatures
pub trait Instance: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// Function signatures following guidelines: (ctx, inst, event) -> Pin<Box<dyn Future<Output = ()>>>
pub type EntryFn<T> = fn(&Context, &mut T, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>>;
pub type EffectFn<T> = fn(&Context, &mut T, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ExitFn<T> = fn(&Context, &mut T, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ActivityFn<T> = fn(&Context, &mut T, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>>;

// Guard is synchronous and returns bool
pub type GuardFn<T> = fn(&Context, &T, &Event) -> bool;

// Timer functions return Duration (not milliseconds)
pub type DurationFn<T> = fn(&Context, &T, &Event) -> Duration;

// Element trait
pub trait Element {
    fn kind(&self) -> KindValue;
    fn qualified_name(&self) -> &str;
    fn owner(&self) -> String {
        dirname(self.qualified_name()).to_string()
    }
    fn name(&self) -> String {
        basename(self.qualified_name()).to_string()
    }
}

#[derive(Debug, Clone)]
pub struct NamedElement {
    pub kind: KindValue,
    pub qualified_name: String,
}

impl Element for NamedElement {
    fn kind(&self) -> KindValue {
        self.kind
    }
    fn qualified_name(&self) -> &str {
        &self.qualified_name
    }
}

#[derive(Debug, Clone)]
pub struct Vertex {
    pub element: NamedElement,
    pub transitions: Vec<String>,
}

impl Element for Vertex {
    fn kind(&self) -> KindValue {
        self.element.kind
    }
    fn qualified_name(&self) -> &str {
        &self.element.qualified_name
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub vertex: Vertex,
    pub initial: String,
    pub entry: Vec<String>,
    pub exit: Vec<String>,
    pub activities: Vec<String>,
    pub deferred: Vec<String>,
}

impl Element for State {
    fn kind(&self) -> KindValue {
        self.vertex.element.kind
    }
    fn qualified_name(&self) -> &str {
        &self.vertex.element.qualified_name
    }
}

#[derive(Debug, Clone)]
pub struct TransitionPath {
    pub enter: Vec<String>,
    pub exit: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub element: NamedElement,
    pub source: String,
    pub target: String,
    pub guard: String,
    pub effect: Vec<String>,
    pub events: Vec<String>,
    pub paths: HashMap<String, TransitionPath>,
}

impl Element for Transition {
    fn kind(&self) -> KindValue {
        self.element.kind
    }
    fn qualified_name(&self) -> &str {
        &self.element.qualified_name
    }
}

#[derive(Debug)]
pub struct Behavior<T: Instance> {
    pub element: NamedElement,
    pub entry: Option<EntryFn<T>>,
    pub effect: Option<EffectFn<T>>,
    pub exit: Option<ExitFn<T>>,
    pub activity: Option<ActivityFn<T>>,
}

impl<T: Instance> Element for Behavior<T> {
    fn kind(&self) -> KindValue {
        self.element.kind
    }
    fn qualified_name(&self) -> &str {
        &self.element.qualified_name
    }
}

#[derive(Debug)]
pub struct Constraint<T: Instance> {
    pub element: NamedElement,
    pub guard: Option<GuardFn<T>>,
    pub duration: Option<DurationFn<T>>,
}

impl<T: Instance> Element for Constraint<T> {
    fn kind(&self) -> KindValue {
        self.element.kind
    }
    fn qualified_name(&self) -> &str {
        &self.element.qualified_name
    }
}

// Element storage using enum
#[derive(Debug)]
pub enum ElementVariant<T: Instance> {
    State(State),
    Vertex(Vertex),
    Transition(Transition),
    Behavior(Behavior<T>),
    Constraint(Constraint<T>),
    Event(Event),
}

impl<T: Instance> Element for ElementVariant<T> {
    fn kind(&self) -> KindValue {
        match self {
            ElementVariant::State(s) => s.kind(),
            ElementVariant::Vertex(v) => v.kind(),
            ElementVariant::Transition(t) => t.kind(),
            ElementVariant::Behavior(b) => b.kind(),
            ElementVariant::Constraint(c) => c.kind(),
            ElementVariant::Event(e) => e.kind,
        }
    }

    fn qualified_name(&self) -> &str {
        match self {
            ElementVariant::State(s) => s.qualified_name(),
            ElementVariant::Vertex(v) => v.qualified_name(),
            ElementVariant::Transition(t) => t.qualified_name(),
            ElementVariant::Behavior(b) => b.qualified_name(),
            ElementVariant::Constraint(c) => c.qualified_name(),
            ElementVariant::Event(e) => &e.qualified_name,
        }
    }
}
