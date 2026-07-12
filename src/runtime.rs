use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::context::Context;
use crate::element::AttributeValue;
use crate::error::Result;
use crate::event::Event;
use crate::kind;

pub type SleepFn = Arc<dyn Fn(Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
// Queue hooks are intentionally synchronous: Push, Pop, and Len must return
// their result/error directly to the runtime, not a future or channel.
pub type QueuePushFn = Arc<dyn Fn(&Context, Event) -> Result<()> + Send + Sync>;
pub type QueuePopFn = Arc<dyn Fn(&Context) -> Result<Option<Event>> + Send + Sync>;
pub type QueueLenFn = Arc<dyn Fn(&Context) -> Result<usize> + Send + Sync>;

#[allow(non_snake_case)]
#[derive(Clone, Default)]
pub struct Clock {
    pub Sleep: Option<SleepFn>,
}

impl Clock {
    pub fn with_defaults(&self) -> Self {
        let sleep = self.Sleep.clone().unwrap_or_else(default_sleep_fn);
        Self { Sleep: Some(sleep) }
    }

    pub fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let sleep = self.Sleep.clone().unwrap_or_else(default_sleep_fn);
        sleep(duration)
    }

    #[allow(non_snake_case)]
    pub fn Sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        self.sleep(duration)
    }
}

fn default_sleep_fn() -> SleepFn {
    Arc::new(|duration| Box::pin(tokio::time::sleep(duration)))
}

pub fn default_clock() -> Clock {
    Clock::default().with_defaults()
}

#[allow(non_snake_case)]
pub fn DefaultClock() -> Clock {
    default_clock()
}

#[allow(non_snake_case)]
#[derive(Clone)]
pub struct RuntimeQueue {
    pub Push: QueuePushFn,
    pub Pop: QueuePopFn,
    pub Len: QueueLenFn,
}

impl RuntimeQueue {
    pub fn new(push: QueuePushFn, pop: QueuePopFn, len: QueueLenFn) -> Self {
        Self {
            Push: push,
            Pop: pop,
            Len: len,
        }
    }

    pub fn push(&self, ctx: &Context, event: Event) -> Result<()> {
        (self.Push)(ctx, event)
    }

    pub fn pop(&self, ctx: &Context) -> Result<Option<Event>> {
        (self.Pop)(ctx)
    }

    pub fn len(&self, ctx: &Context) -> Result<usize> {
        (self.Len)(ctx)
    }
}

#[allow(non_snake_case)]
#[derive(Clone, Default)]
pub struct RuntimeConfig {
    pub ID: Option<String>,
    pub Name: Option<String>,
    pub Data: Option<Arc<dyn Any + Send + Sync>>,
    pub Clock: Option<Clock>,
    pub Queue: Option<RuntimeQueue>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDetail {
    pub Name: String,
    pub Kind: kind::KindValue,
    pub Target: Option<String>,
    pub Guard: bool,
    pub Schema: Option<AttributeValue>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionDetail {
    pub Name: String,
    pub Kind: kind::KindValue,
    pub Source: String,
    pub Target: Option<String>,
    pub Events: Vec<String>,
    pub Guard: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub ID: String,
    pub QualifiedName: String,
    pub State: String,
    pub Attributes: HashMap<String, AttributeValue>,
    pub QueueLen: usize,
    pub Transitions: Vec<TransitionDetail>,
    pub Events: Vec<EventDetail>,
}

pub trait SnapshotTarget {
    type Snapshot;

    fn take_snapshot_with_context(&self, ctx: &Context) -> Result<Self::Snapshot>;
}

pub trait DispatchTarget {
    fn dispatch_with_context(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait StopTarget {
    fn stop_with_context(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait RestartTarget {
    fn restart_with_context(
        &self,
        ctx: &Context,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait AttributeGetTarget {
    fn get_attribute(&self, name: &str) -> Option<AttributeValue>;
}

pub trait AttributeSetTarget<V> {
    fn set_attribute(&self, name: &str, value: V) -> Result<()>;
}

pub trait OperationCallTarget {
    fn call_operation(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

    fn call_operation_with_args(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait StartDataTarget<D> {
    fn start_with_data_target(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait RestartDataTarget<D> {
    fn restart_with_data_with_context(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

pub trait RuntimeIdentityTarget {
    fn runtime_id(&self) -> String;
    fn runtime_name(&self) -> String;
    fn runtime_qualified_name(&self) -> String;
}
