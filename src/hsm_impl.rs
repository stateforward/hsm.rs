// HSM Implementation

use std::any::Any;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, SystemTime};

use crate::behavior_context;
use crate::context::Context;
use crate::context_runtime::{
    ContextMachine, register_context_machine, unregister_context_machine,
};
use crate::element::{
    AttributeValue, Behavior, BehaviorOperation, Element, ElementVariant, Instance, OperationFn,
    State,
};
use crate::error::{HsmError, Result};
use crate::event::{
    ANY_EVENT_NAME, AttributeChange, Event, call_trigger_name, final_event, initial_event,
};
use crate::kind::{self, is_kind};
use crate::model::Model;
use crate::queue::EventQueue;
use crate::runtime::{
    AttributeGetTarget, AttributeSetTarget, Clock, DispatchTarget, EventDetail,
    OperationCallTarget, RestartDataTarget, RestartTarget, RuntimeConfig, RuntimeIdentityTarget,
    Snapshot, SnapshotTarget, StartDataTarget, StopTarget, TransitionDetail,
};

#[derive(Clone)]
struct DeferredEvent {
    event: Event,
    owner: String,
}

enum EventOutcome {
    Processed,
    Deferred,
    Transition { source: String },
}

type TransitionOutcome = std::result::Result<Option<String>, HsmError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerKind {
    After,
    At,
    Every,
}

struct BehaviorFuture {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for BehaviorFuture {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        match catch_unwind(AssertUnwindSafe(|| self.future.as_mut().poll(cx))) {
            Ok(Poll::Ready(())) => Poll::Ready(true),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(false),
        }
    }
}

async fn await_behavior_future(future: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
    BehaviorFuture { future }.await && !behavior_context::take_abort()
}

fn event_names(event: &Event) -> Vec<String> {
    let mut names = vec![event.name.clone()];
    if kind::is_kind(event.kind, kind::CALL_EVENT) {
        names.push(call_trigger_name(&event.name));
    }
    names
}

fn snapshot_event_name(name: &str) -> String {
    name.strip_prefix("hsm_call:").unwrap_or(name).to_string()
}

fn snapshot_event_kind(name: &str) -> kind::KindValue {
    if name.starts_with("hsm_call:") {
        kind::CALL_EVENT
    } else {
        kind::EVENT
    }
}

fn default_attribute_values<T: Instance>(model: &Model<T>) -> HashMap<String, AttributeValue> {
    let mut attributes = HashMap::new();
    for (name, attribute) in &model.attributes {
        if let Some(default_value) = &attribute.default_value {
            attributes.insert(name.clone(), default_value.clone());
        }
    }
    attributes
}

static ACTIVE_DRAINS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();

fn active_drains() -> &'static Mutex<HashSet<usize>> {
    ACTIVE_DRAINS.get_or_init(|| Mutex::new(HashSet::new()))
}

static PENDING_REENTRANT_EVENTS: OnceLock<Mutex<HashMap<usize, VecDeque<(Context, Event)>>>> =
    OnceLock::new();

fn pending_reentrant_events() -> &'static Mutex<HashMap<usize, VecDeque<(Context, Event)>>> {
    PENDING_REENTRANT_EVENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

static ACTIVE_OPERATIONS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();

fn active_operations() -> &'static Mutex<HashMap<usize, usize>> {
    ACTIVE_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

static ACTIVE_ACTIVITIES: OnceLock<Mutex<HashMap<usize, Vec<String>>>> = OnceLock::new();

fn active_activities() -> &'static Mutex<HashMap<usize, Vec<String>>> {
    ACTIVE_ACTIVITIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_activity(ctx: &Context, behavior: &str) {
    active_activities()
        .lock()
        .unwrap()
        .entry(ctx.registry_key())
        .or_default()
        .push(behavior.to_string());
}

fn is_active_activity(ctx: &Context, behavior: &str) -> bool {
    active_activities()
        .lock()
        .unwrap()
        .get(&ctx.registry_key())
        .is_some_and(|activities| activities.iter().any(|active| active == behavior))
}

fn finish_activity(ctx: &Context, behavior: &str) -> bool {
    let mut active = active_activities().lock().unwrap();
    let Some(activities) = active.get_mut(&ctx.registry_key()) else {
        return false;
    };
    let Some(index) = activities.iter().position(|active| active == behavior) else {
        return false;
    };
    activities.remove(index);
    if activities.is_empty() {
        active.remove(&ctx.registry_key());
    }
    true
}

fn cancel_activities(ctx: &Context) -> Vec<String> {
    active_activities()
        .lock()
        .unwrap()
        .remove(&ctx.registry_key())
        .unwrap_or_default()
}

fn running_activity(ctx: &Context) -> Option<String> {
    behavior_context::current_activity(ctx)
}

struct DrainGuard {
    key: usize,
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        active_drains().lock().unwrap().remove(&self.key);
        pending_reentrant_events().lock().unwrap().remove(&self.key);
    }
}

pub struct HSM<T: Instance> {
    model: Arc<Model<T>>,
    instance: Arc<RwLock<T>>,
    current_state: Arc<RwLock<String>>,
    queue: Arc<Mutex<EventQueue>>,
    deferred_events: Arc<Mutex<VecDeque<DeferredEvent>>>,
    shallow_history: Arc<Mutex<HashMap<String, String>>>,
    deep_history: Arc<Mutex<HashMap<String, String>>>,
    attributes: Arc<Mutex<HashMap<String, AttributeValue>>>,
    context: Arc<Context>,
    clock: Clock,
    id: String,
    name: String,
    data: Option<Arc<dyn Any + Send + Sync>>,
    pub state_contexts: Arc<RwLock<std::collections::HashMap<String, Context>>>,
}

impl<T: Instance> HSM<T> {
    pub fn new(instance: T, model: Model<T>) -> Self {
        Self::new_with_config(instance, model, RuntimeConfig::default())
    }

    pub fn new_with_config(instance: T, model: Model<T>, config: RuntimeConfig) -> Self {
        Self::new_with_config_and_context(instance, model, config, Context::new())
    }

    pub(crate) fn new_with_config_and_context(
        instance: T,
        mut model: Model<T>,
        config: RuntimeConfig,
        context: Context,
    ) -> Self {
        if model.history_updates.is_empty() {
            model.build_history_table();
        }

        let model = Arc::new(model);
        let instance = Arc::new(RwLock::new(instance));
        let initial_state = model.state.qualified_name().to_string();
        let name = config
            .Name
            .clone()
            .unwrap_or_else(|| model.state.qualified_name().to_string());
        let id = config.ID.clone().unwrap_or_default();
        let context = Arc::new(context);
        let clock = config.Clock.clone().unwrap_or_default().with_defaults();
        let queue = config
            .Queue
            .clone()
            .map(EventQueue::with_regular_queue)
            .unwrap_or_else(EventQueue::new);

        let attributes = default_attribute_values(&model);

        Self {
            model,
            instance,
            current_state: Arc::new(RwLock::new(initial_state)),
            queue: Arc::new(Mutex::new(queue)),
            deferred_events: Arc::new(Mutex::new(VecDeque::new())),
            shallow_history: Arc::new(Mutex::new(HashMap::new())),
            deep_history: Arc::new(Mutex::new(HashMap::new())),
            attributes: Arc::new(Mutex::new(attributes)),
            context,
            clock,
            id,
            name,
            data: config.Data.clone(),
            state_contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    fn start_with_runtime_data(
        &self,
        data: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let hsm = self.clone();
        Box::pin(async move {
            crate::validate(&hsm.model)?;

            if hsm.state() != hsm.model.state.qualified_name() {
                return Err(HsmError::Validation("already started HSM".to_string()));
            }

            *hsm.attributes.lock().unwrap() = default_attribute_values(&hsm.model);
            register_context_machine(&hsm.context, &hsm);
            let mut initial_event = initial_event();
            initial_event.data = data.or_else(|| hsm.data.clone());
            if let Err(error) = hsm.dispatch_queued(&hsm.context, initial_event).await {
                unregister_context_machine(&hsm.context, &hsm);
                return Err(error);
            }
            Ok(())
        })
    }

    pub fn start(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.start_with_runtime_data(None)
    }

    pub fn start_with_data<D>(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Send + Sync + 'static,
    {
        self.start_with_runtime_data(Some(Arc::new(data)))
    }

    #[allow(non_snake_case)]
    pub fn StartWithData<D>(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Send + Sync + 'static,
    {
        self.start_with_data(data)
    }

    pub fn stop(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            if ctx.is_cancelled() {
                return Ok(());
            }

            let current_state = hsm.current_state.read().unwrap().clone();
            let root = hsm.model.state.qualified_name().to_string();
            if current_state == root {
                return Ok(());
            }

            register_context_machine(&ctx, &hsm);
            let event = Event::completion("hsm_stop");
            for state_name in hsm.active_exit_path(&current_state) {
                if let Some(state) = hsm.model.get_state(&state_name).cloned() {
                    hsm.exit_state(&state, &event, &ctx).await;
                }
            }

            hsm.queue.lock().unwrap().clear_with_context(&ctx);
            hsm.deferred_events.lock().unwrap().clear();
            hsm.shallow_history.lock().unwrap().clear();
            hsm.deep_history.lock().unwrap().clear();
            hsm.state_contexts.write().unwrap().clear();
            *hsm.current_state.write().unwrap() = root;
            unregister_context_machine(&hsm.context, &hsm);
            if ctx.registry_key() != hsm.context.registry_key() {
                unregister_context_machine(&ctx, &hsm);
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.stop(ctx)
    }

    pub fn restart(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart_with_runtime_data(ctx, None)
    }

    fn restart_with_runtime_data(
        &self,
        ctx: &Context,
        data: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            let current_state = hsm.current_state.read().unwrap().clone();
            let root = hsm.model.state.qualified_name().to_string();
            if current_state == root {
                return Err(HsmError::Validation(
                    "restart requires a started HSM".to_string(),
                ));
            }

            hsm.stop(&ctx).await?;
            hsm.start_with_runtime_data(data).await
        })
    }

    pub fn restart_with_data<D>(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Send + Sync + 'static,
    {
        self.restart_with_runtime_data(ctx, Some(Arc::new(data)))
    }

    #[allow(non_snake_case)]
    pub fn Restart(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart(ctx)
    }

    #[allow(non_snake_case)]
    pub fn RestartWithData<D>(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Send + Sync + 'static,
    {
        self.restart_with_data(ctx, data)
    }

    pub fn state(&self) -> String {
        self.current_state.read().unwrap().clone()
    }

    pub fn current_state(&self) -> String {
        self.current_state.read().unwrap().clone()
    }

    pub fn is_started(&self) -> bool {
        self.state() != self.model.state.qualified_name()
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn clock(&self) -> Clock {
        self.clock.clone()
    }

    #[allow(non_snake_case)]
    pub fn Clock(&self) -> Clock {
        self.clock()
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[allow(non_snake_case)]
    pub fn ID(&self) -> String {
        self.id()
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[allow(non_snake_case)]
    pub fn Name(&self) -> String {
        self.name()
    }

    pub fn qualified_name(&self) -> String {
        self.name()
    }

    #[allow(non_snake_case)]
    pub fn QualifiedName(&self) -> String {
        self.qualified_name()
    }

    pub fn data(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        self.data.clone()
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        self.data()
    }

    pub fn take_snapshot(&self) -> Result<Snapshot> {
        let state = self.state();
        if state == self.model.state.qualified_name() && !self.is_draining() {
            return Err(HsmError::Runtime(
                "take snapshot requires a started HSM".to_string(),
            ));
        }

        let attributes = self.attributes.lock().unwrap().clone();
        let queue_len = self.queue.lock().unwrap().len_with_context(&self.context);
        let transitions_by_event = self.model.transition_map.get(&state);
        let mut events = Vec::new();
        let mut transitions = Vec::new();
        let mut seen_transitions = HashSet::new();

        if let Some(transitions_by_event) = transitions_by_event {
            for (event_name, transition_names) in transitions_by_event {
                for transition_name in transition_names {
                    if let Some(transition) = self.model.get_transition(transition_name) {
                        events.push(EventDetail {
                            Name: snapshot_event_name(event_name),
                            Kind: snapshot_event_kind(event_name),
                            Target: if transition.target.is_empty() {
                                None
                            } else {
                                Some(transition.target.clone())
                            },
                            Guard: !transition.guard.is_empty(),
                            Schema: None,
                        });
                        if seen_transitions.insert(transition_name.clone()) {
                            transitions.push(TransitionDetail {
                                Name: transition.qualified_name().to_string(),
                                Kind: transition.kind(),
                                Source: transition.source.clone(),
                                Target: if transition.target.is_empty() {
                                    None
                                } else {
                                    Some(transition.target.clone())
                                },
                                Events: transition
                                    .events
                                    .iter()
                                    .map(|event| snapshot_event_name(event))
                                    .collect(),
                                Guard: !transition.guard.is_empty(),
                            });
                        }
                    }
                }
            }
        }

        Ok(Snapshot {
            ID: self.id(),
            QualifiedName: self.qualified_name(),
            State: state,
            Attributes: attributes,
            QueueLen: queue_len,
            Transitions: transitions,
            Events: events,
        })
    }

    #[allow(non_snake_case)]
    pub fn TakeSnapshot(&self) -> Result<Snapshot> {
        self.take_snapshot()
    }

    // Getter methods for testing access
    pub fn instance(&self) -> &Arc<RwLock<T>> {
        &self.instance
    }

    pub fn instance_mut(&self) -> std::sync::RwLockWriteGuard<'_, T> {
        self.instance.write().unwrap()
    }

    pub fn current_state_ref(&self) -> &Arc<RwLock<String>> {
        &self.current_state
    }

    pub fn get(&self, name: &str) -> Option<AttributeValue> {
        let qualified_name = self.qualify_attribute_name(name);
        if !self.model.attributes.contains_key(&qualified_name) {
            return None;
        }
        self.attributes
            .lock()
            .unwrap()
            .get(&qualified_name)
            .cloned()
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, name: &str) -> Option<AttributeValue> {
        self.get(name)
    }

    pub fn set<V: Into<AttributeValue>>(&self, name: &str, value: V) -> Result<()> {
        self.set_with_context(&self.context, name, value)
    }

    fn set_with_context<V: Into<AttributeValue>>(
        &self,
        ctx: &Context,
        name: &str,
        value: V,
    ) -> Result<()> {
        if self.state() == self.model.state.qualified_name() && !self.is_draining() {
            return Err(HsmError::Runtime("set requires a started HSM".to_string()));
        }

        let qualified_name = self.qualify_attribute_name(name);
        let Some(attribute) = self.model.get_attribute(&qualified_name) else {
            return Err(HsmError::Runtime(format!(
                "missing attribute \"{}\"",
                qualified_name
            )));
        };
        let value = value.into();
        if let Some(expected_type) = &attribute.value_type {
            if &value.value_type() != expected_type {
                return Err(HsmError::Runtime(format!(
                    "attribute \"{}\" rejected value",
                    qualified_name
                )));
            }
        }

        let old_value = {
            let mut attributes = self.attributes.lock().unwrap();
            let old_value = attributes.get(&qualified_name).cloned();
            if old_value.as_ref() == Some(&value) {
                return Ok(());
            }
            attributes.insert(qualified_name.clone(), value.clone());
            old_value
        };

        let event = Event {
            kind: kind::CHANGE_EVENT,
            qualified_name: qualified_name.clone(),
            name: qualified_name.clone(),
            data: Some(Arc::new(AttributeChange {
                Name: qualified_name,
                Old: old_value,
                Value: value,
            })),
        };
        if running_activity(ctx).is_some() {
            self.queue_reentrant_event(ctx, event);
            return Ok(());
        }
        {
            let mut queue = self.queue.lock().unwrap();
            if queue.push_with_context(ctx, event).is_err() {
                return Ok(());
            }
        }
        self.run_to_completion(ctx);
        Ok(())
    }

    #[allow(non_snake_case)]
    pub fn Set<V: Into<AttributeValue>>(&self, name: &str, value: V) -> Result<()> {
        self.set(name, value)
    }

    pub fn call(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, Vec::new())
    }

    pub fn call_with_args(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        if self.state() == self.model.state.qualified_name() && !self.is_draining() {
            return Box::pin(async {
                Err(HsmError::Runtime(
                    "operation requires a started HSM".to_string(),
                ))
            });
        }

        if name.is_empty() {
            return Box::pin(async {
                Err(HsmError::Runtime(
                    "operation name cannot be empty".to_string(),
                ))
            });
        }

        let operation_name = self.qualify_operation_name(name);
        let Some(operation) = self.model.get_operation(&operation_name) else {
            let name = name.to_string();
            return Box::pin(async move {
                Err(HsmError::Runtime(format!("missing operation \"{name}\"")))
            });
        };
        if operation.action.is_none() {
            let name = name.to_string();
            return Box::pin(async move {
                Err(HsmError::Runtime(format!("missing operation \"{name}\"")))
            });
        }

        let event = Event::call_with_args(operation_name.clone(), args);
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            if !hsm
                .execute_operation_by_name(&operation_name, &event, &ctx)
                .await
            {
                return Ok(());
            }
            hsm.dispatch(&ctx, event).await
        })
    }

    #[allow(non_snake_case)]
    pub fn Call(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call(ctx, name)
    }

    #[allow(non_snake_case)]
    pub fn CallWithArgs(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, args)
    }

    fn qualify_attribute_name(&self, name: &str) -> String {
        if name.starts_with('/') {
            name.to_string()
        } else {
            crate::path::join(self.model.state.qualified_name(), name)
        }
    }

    fn qualify_operation_name(&self, name: &str) -> String {
        if name.starts_with('/') {
            name.to_string()
        } else {
            crate::path::join(self.model.state.qualified_name(), name)
        }
    }

    fn drain_key(&self) -> usize {
        Arc::as_ptr(&self.queue) as usize
    }

    fn begin_drain(&self) -> Option<DrainGuard> {
        let key = self.drain_key();
        let mut active = active_drains().lock().unwrap();
        if active.contains(&key) {
            return None;
        }
        active.insert(key);
        Some(DrainGuard { key })
    }

    fn is_draining(&self) -> bool {
        active_drains().lock().unwrap().contains(&self.drain_key())
    }

    fn begin_operation(&self) {
        let mut active = active_operations().lock().unwrap();
        *active.entry(self.drain_key()).or_insert(0) += 1;
    }

    fn end_operation(&self) -> bool {
        let key = self.drain_key();
        let mut active = active_operations().lock().unwrap();
        let Some(depth) = active.get_mut(&key) else {
            return true;
        };
        *depth -= 1;
        if *depth == 0 {
            active.remove(&key);
            return true;
        }
        false
    }

    fn is_operation_active(&self) -> bool {
        active_operations()
            .lock()
            .unwrap()
            .contains_key(&self.drain_key())
    }

    fn queue_reentrant_event(&self, ctx: &Context, event: Event) {
        pending_reentrant_events()
            .lock()
            .unwrap()
            .entry(self.drain_key())
            .or_default()
            .push_back((ctx.clone(), event));
    }

    fn flush_reentrant_events(&self, fallback_ctx: &Context) -> Result<bool> {
        let events = pending_reentrant_events()
            .lock()
            .unwrap()
            .remove(&self.drain_key());
        let Some(mut events) = events else {
            return Ok(false);
        };

        let mut queue = self.queue.lock().unwrap();
        while let Some((ctx, event)) = events.pop_front() {
            if queue.push_with_context(&ctx, event).is_err() {
                let _ = queue.push_with_context(fallback_ctx, Event::error_event());
            }
        }
        Ok(true)
    }

    // Dispatch following exact signature: hsm.dispatch(ctx, Event)
    pub fn dispatch(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        if self.state() == self.model.state.qualified_name() && !self.is_draining() {
            return Box::pin(async {
                Err(HsmError::Runtime(
                    "dispatch requires a started HSM".to_string(),
                ))
            });
        }

        self.dispatch_queued(ctx, event)
    }

    /// Dispatches an event while borrowing the machine and context for the
    /// returned future. This has the same queue and lifecycle semantics as
    /// [`Self::dispatch`], without cloning either value into that future.
    pub fn dispatch_borrowed<'a>(
        &'a self,
        ctx: &'a Context,
        event: Event,
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        async move {
            if self.state() == self.model.state.qualified_name() && !self.is_draining() {
                return Err(HsmError::Runtime(
                    "dispatch requires a started HSM".to_string(),
                ));
            }

            self.dispatch_queued_borrowed(ctx, event).await
        }
    }

    fn dispatch_queued(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move { hsm.dispatch_queued_borrowed(&ctx, event).await })
    }

    async fn dispatch_queued_borrowed(&self, ctx: &Context, event: Event) -> Result<()> {
        if kind::is_kind(event.kind, kind::CALL_EVENT)
            && running_activity(ctx).is_none()
            && (self.is_draining() || self.is_operation_active())
        {
            self.queue_reentrant_event(ctx, event);
            return Ok(());
        }

        {
            let mut queue = self.queue.lock().unwrap();
            if queue.push_with_context(ctx, event).is_err() {
                let _ = queue.push_with_context(ctx, Event::error_event());
            }
        }

        self.drain_queue(ctx).await
    }

    fn run_to_completion(&self, ctx: &Context) {
        let hsm = self.clone();
        let ctx = ctx.clone();
        let drain = move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(async move {
                    let _ = hsm.drain_queue(&ctx).await;
                });
            }
        };

        if tokio::runtime::Handle::try_current().is_ok() {
            let _ = std::thread::spawn(drain).join();
        } else {
            drain();
        }
    }

    fn active_exit_path(&self, current_state: &str) -> Vec<String> {
        let root = self.model.state.qualified_name();
        let mut path = Vec::new();
        let mut current = current_state.to_string();

        while !current.is_empty() && current != root {
            if self.model.get_state(&current).is_some() {
                path.push(current.clone());
            }

            let parent = crate::path::dirname(&current);
            if parent == current {
                break;
            }
            current = parent.to_string();
        }

        path
    }

    async fn drain_queue(&self, ctx: &Context) -> Result<()> {
        register_context_machine(ctx, self);
        let Some(_guard) = self.begin_drain() else {
            return Ok(());
        };
        let mut deferred_events = Vec::new();
        loop {
            let event_result = {
                let mut queue = self.queue.lock().unwrap();
                queue.pop_with_context(ctx)
            };

            match event_result {
                Ok(Some(event)) => {
                    if let Some(deferred) = self.take_deferred_event(&event) {
                        let current_state = self.current_state.read().unwrap().clone();
                        if self.deferred_owner_still_active(&deferred.owner, &current_state) {
                            deferred_events.push(deferred);
                            continue;
                        }
                    }

                    match self.process_single_event(&event, ctx).await? {
                        EventOutcome::Deferred => {
                            if let Some(owner) = self.deferred_owner(&event) {
                                deferred_events.push(DeferredEvent { event, owner });
                            }
                        }
                        EventOutcome::Transition { source } => {
                            self.requeue_deferred_events(ctx, &mut deferred_events, &source);
                        }
                        EventOutcome::Processed => {}
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let event = Event::error_event();
                    if let EventOutcome::Transition { source } =
                        self.process_single_event(&event, ctx).await?
                    {
                        self.requeue_deferred_events(ctx, &mut deferred_events, &source);
                    }
                }
            };
            self.flush_reentrant_events(ctx)?;
        }
        self.requeue_all_deferred_events(ctx, &mut deferred_events);
        Ok(())
    }

    async fn process_single_event(&self, event: &Event, ctx: &Context) -> Result<EventOutcome> {
        let current_state = self.current_state.read().unwrap().clone();

        let call_event_name =
            kind::is_kind(event.kind, kind::CALL_EVENT).then(|| call_trigger_name(&event.name));
        let mut lookup_names = [""; 3];
        let mut lookup_name_count = 0;
        lookup_names[lookup_name_count] = event.name.as_str();
        lookup_name_count += 1;
        if let Some(call_event_name) = call_event_name.as_deref() {
            lookup_names[lookup_name_count] = call_event_name;
            lookup_name_count += 1;
        }
        if event.name != ANY_EVENT_NAME {
            lookup_names[lookup_name_count] = ANY_EVENT_NAME;
            lookup_name_count += 1;
        }

        let event_transitions = self.model.transition_map.get(&current_state);
        let root_state = self.model.state.qualified_name().to_string();
        let mut selection_state = current_state.clone();

        loop {
            if let Some(event_transitions) = event_transitions {
                for event_name in &lookup_names[..lookup_name_count] {
                    let Some(transition_names) = event_transitions.get(*event_name) else {
                        continue;
                    };
                    for transition_name in transition_names {
                        if let Some(transition) = self.model.get_transition(transition_name) {
                            let selection_initial = if selection_state == root_state {
                                self.model.state.initial.as_str()
                            } else {
                                self.model
                                    .get_state(&selection_state)
                                    .map(|state| state.initial.as_str())
                                    .unwrap_or("")
                            };
                            let source_matches_selection_state =
                                if transition.source == selection_state {
                                    let transition_owner =
                                        crate::path::dirname(transition.qualified_name());
                                    let handles_at_or_below = transition_owner == selection_state
                                        || crate::path::dirname(&selection_state) == root_state
                                        || crate::path::is_ancestor_or_equal(
                                            &selection_state,
                                            transition_owner,
                                        );
                                    handles_at_or_below
                                        || !self.is_deferred_in_state(&selection_state, event_name)
                                } else {
                                    selection_initial == transition.source
                                };
                            if !source_matches_selection_state {
                                continue;
                            }

                            let guard_ok = self.evaluate_guard(&transition.guard, &event, ctx);
                            if behavior_context::take_abort() {
                                return Ok(EventOutcome::Processed);
                            }

                            if guard_ok {
                                if let Some(new_state) = self
                                    .execute_transition(&current_state, transition, &event, ctx)
                                    .await?
                                {
                                    let state_changed = new_state.as_str() != current_state;
                                    *self.current_state.write().unwrap() = new_state.clone();
                                    if state_changed {
                                        if self.model.get_state(&new_state).is_some_and(|state| {
                                            is_kind(state.kind(), kind::FINAL_STATE)
                                        }) {
                                            let _ = self
                                                .queue
                                                .lock()
                                                .unwrap()
                                                .push_with_context(ctx, final_event());
                                        }
                                    }
                                    return Ok(EventOutcome::Transition {
                                        source: transition.source.clone(),
                                    });
                                }
                                return Ok(EventOutcome::Processed);
                            }
                        }
                    }
                }
            }

            if lookup_names[..lookup_name_count]
                .iter()
                .any(|event_name| self.is_deferred_in_state(&selection_state, event_name))
            {
                return Ok(EventOutcome::Deferred);
            }

            if selection_state == root_state {
                break;
            }
            let parent = crate::path::dirname(&selection_state);
            if parent == selection_state || parent == "/" {
                break;
            }
            selection_state = parent.to_string();
        }

        Ok(EventOutcome::Processed)
    }

    fn is_deferred_in_state(&self, state: &str, event_name: &str) -> bool {
        let deferred = if state == self.model.state.qualified_name() {
            &self.model.state.deferred
        } else {
            let Some(state) = self.model.get_state(state) else {
                return false;
            };
            &state.deferred
        };

        deferred
            .iter()
            .any(|deferred| deferred == event_name || deferred == ANY_EVENT_NAME)
    }

    fn deferred_owner(&self, event: &Event) -> Option<String> {
        let current_state = self.current_state.read().unwrap().clone();
        let root_state = self.model.state.qualified_name().to_string();
        let event_names = event_names(event);
        let mut selection_state = current_state;

        loop {
            if event_names
                .iter()
                .any(|event_name| self.is_deferred_in_state(&selection_state, event_name))
            {
                return Some(selection_state);
            }

            if selection_state == root_state {
                return None;
            }
            let parent = crate::path::dirname(&selection_state);
            if parent == selection_state || parent == "/" {
                return None;
            }
            selection_state = parent.to_string();
        }
    }

    fn requeue_deferred_events(
        &self,
        ctx: &Context,
        deferred_events: &mut Vec<DeferredEvent>,
        transition_source: &str,
    ) {
        if deferred_events.is_empty() {
            return;
        }

        let current_state = self.current_state.read().unwrap().clone();
        let mut replay = Vec::new();
        for deferred in deferred_events.drain(..) {
            if self.deferred_event_survives(&deferred.owner, &current_state, transition_source) {
                replay.push(deferred);
            }
        }

        self.push_deferred_records(ctx, replay);
    }

    fn requeue_all_deferred_events(&self, ctx: &Context, deferred_events: &mut Vec<DeferredEvent>) {
        if deferred_events.is_empty() {
            return;
        }

        let replay = deferred_events.drain(..).collect();
        self.push_deferred_records(ctx, replay);
    }

    fn push_deferred_records(&self, ctx: &Context, events: Vec<DeferredEvent>) {
        if events.is_empty() {
            return;
        }

        let mut requeued = VecDeque::new();
        {
            let mut queue = self.queue.lock().unwrap();
            for deferred in events {
                if queue.push_with_context(ctx, deferred.event.clone()).is_ok() {
                    requeued.push_back(deferred);
                }
            }
        }
        self.deferred_events.lock().unwrap().extend(requeued);
    }

    fn take_deferred_event(&self, event: &Event) -> Option<DeferredEvent> {
        let mut deferred = self.deferred_events.lock().unwrap();
        let index = deferred.iter().position(|deferred| {
            deferred.event.name == event.name
                && deferred.event.qualified_name == event.qualified_name
                && deferred.event.kind == event.kind
        })?;
        deferred.remove(index)
    }

    fn deferred_owner_still_active(&self, owner: &str, current_state: &str) -> bool {
        crate::path::is_ancestor_or_equal(owner, current_state)
    }

    fn deferred_event_survives(
        &self,
        owner: &str,
        current_state: &str,
        transition_source: &str,
    ) -> bool {
        let root_state = self.model.state.qualified_name();
        let mut ancestor = crate::path::dirname(owner).to_string();

        while ancestor != root_state && ancestor != "/" && !ancestor.is_empty() {
            if self
                .model
                .get_state(&ancestor)
                .is_some_and(|state| is_kind(state.kind(), kind::SUBMACHINE_STATE))
                && !crate::path::is_ancestor_or_equal(&ancestor, current_state)
            {
                return crate::path::is_ancestor_or_equal(&ancestor, transition_source)
                    && ancestor != transition_source;
            }

            let parent = crate::path::dirname(&ancestor);
            if parent == ancestor {
                break;
            }
            ancestor = parent.to_string();
        }
        true
    }

    fn entry_point_reentry_boundary(
        &self,
        transition: &crate::element::Transition,
    ) -> Option<String> {
        let vertex = self.model.get_vertex(&transition.target)?;
        if !is_kind(vertex.kind(), kind::ENTRY_POINT) {
            return None;
        }

        let boundary = crate::path::dirname(vertex.qualified_name()).to_string();
        if !self
            .model
            .get_state(&boundary)
            .is_some_and(|state| is_kind(state.kind(), kind::SUBMACHINE_STATE))
        {
            return None;
        }

        (transition.source == boundary
            || crate::path::is_ancestor_or_equal(&boundary, &transition.source))
        .then_some(boundary)
    }

    async fn execute_transition(
        &self,
        current_state: &str,
        transition: &crate::element::Transition,
        event: &Event,
        ctx: &Context,
    ) -> TransitionOutcome {
        // Get the appropriate path for this transition
        let calculated_path;
        let path = if let Some(p) = transition.paths.get(current_state) {
            // We have a pre-calculated path for the current state
            p
        } else if transition.source != current_state {
            // Transition is defined on an ancestor, calculate path from current state
            use crate::path::is_ancestor_or_equal;
            if is_ancestor_or_equal(&transition.source, current_state) {
                let base_path = transition
                    .paths
                    .get(&transition.source)
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::model::Model::<T>::calculate_path_static(
                            &transition.source,
                            &transition.target,
                        )
                    });
                calculated_path = if is_kind(transition.element.kind, kind::INTERNAL) {
                    base_path
                } else {
                    let mut exit = Vec::new();
                    let mut exiting = current_state.to_string();
                    while exiting != transition.source && !exiting.is_empty() {
                        if self.model.get_state(&exiting).is_some() {
                            exit.push(exiting.clone());
                        }
                        let parent = crate::path::dirname(&exiting);
                        if parent == exiting || parent == "/" {
                            break;
                        }
                        exiting = parent.to_string();
                    }
                    exit.extend(base_path.exit);
                    crate::element::TransitionPath {
                        exit,
                        enter: base_path.enter,
                    }
                };
                &calculated_path
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };
        let reentry_path;
        let path = if let Some(boundary) = self.entry_point_reentry_boundary(transition) {
            reentry_path = {
                let mut path = path.clone();
                if !path.exit.iter().any(|state| state == &boundary) {
                    path.exit.push(boundary.clone());
                }
                if !path.enter.iter().any(|state| state == &boundary) {
                    path.enter.insert(0, boundary);
                }
                path
            };
            &reentry_path
        } else {
            path
        };

        let target_is_history = self
            .model
            .get_vertex(&transition.target)
            .is_some_and(|vertex| {
                is_kind(vertex.kind(), kind::SHALLOW_HISTORY)
                    || is_kind(vertex.kind(), kind::DEEP_HISTORY)
            });

        if !path.exit.is_empty() && !target_is_history {
            self.remember_history(current_state);
        }

        // Exit states
        for exiting in &path.exit {
            if let Some(state) = self.model.get_state(exiting) {
                if !self.exit_state(state, event, ctx).await {
                    return Ok(None);
                }
            }
        }

        // Execute effects
        for effect_name in &transition.effect {
            if let Some(behavior) = self.model.get_behavior(effect_name) {
                if !self.execute_behavior(behavior, event, ctx).await {
                    return Ok(None);
                }
            }
        }

        let mut enter_path = path.enter.clone();
        let mut effective_target = transition.target.clone();

        if let Some(vertex) = self.model.get_vertex(&transition.target) {
            if is_kind(vertex.kind(), kind::ENTRY_POINT) {
                let mut selected_transition = None;
                for transition_name in &vertex.transitions {
                    let Some(entry_transition) = self.model.get_transition(transition_name) else {
                        continue;
                    };
                    if entry_transition.target.is_empty() {
                        continue;
                    }
                    if !self.evaluate_guard(&entry_transition.guard, event, ctx) {
                        if behavior_context::take_abort() {
                            return Ok(None);
                        }
                        continue;
                    }
                    if behavior_context::take_abort() {
                        return Ok(None);
                    }
                    selected_transition = Some((
                        vertex.qualified_name().to_string(),
                        entry_transition.target.clone(),
                        entry_transition.effect.clone(),
                    ));
                    break;
                }

                if let Some((entry_point, entry_target, entry_effects)) = selected_transition {
                    for effect_name in &entry_effects {
                        if let Some(behavior) = self.model.get_behavior(effect_name) {
                            if !self.execute_behavior(behavior, event, ctx).await {
                                return Ok(None);
                            }
                        }
                    }

                    let boundary = crate::path::dirname(&entry_point).to_string();
                    let mut rewritten_enter = Vec::new();
                    for entering in &enter_path {
                        if entering == &entry_point {
                            break;
                        }
                        rewritten_enter.push(entering.clone());
                        if entering == &boundary {
                            break;
                        }
                    }
                    rewritten_enter.extend(
                        crate::model::Model::<T>::calculate_path_static(&boundary, &entry_target)
                            .enter,
                    );
                    enter_path = rewritten_enter;
                    effective_target = entry_target;
                }
            }
        }

        // Handle internal transitions
        if is_kind(transition.element.kind, kind::INTERNAL) {
            return Ok(Some(current_state.to_string()));
        }

        // Enter states
        for entering in &enter_path {
            if let Some(element) = self.model.members.get(entering) {
                let default_entry = entering == &effective_target;
                let Some(result) =
                    Box::pin(self.enter_state(element, event, default_entry, ctx)).await?
                else {
                    return Ok(None);
                };
                if default_entry {
                    return Ok(Some(result));
                }
            }
        }

        Ok(Some(effective_target))
    }

    async fn enter_state(
        &self,
        element: &ElementVariant<T>,
        event: &Event,
        default_entry: bool,
        ctx: &Context,
    ) -> TransitionOutcome {
        match element {
            ElementVariant::State(state) => {
                // Execute entry actions
                for entry_name in &state.entry {
                    if let Some(behavior) = self.model.get_behavior(entry_name) {
                        if !self.execute_behavior(behavior, event, ctx).await {
                            return Ok(None);
                        }
                    }
                }

                let timer_transitions = self.timer_transitions_for_state(state.qualified_name());
                // Start activities and timers with a new cancellation context for this state.
                if !state.activities.is_empty() || !timer_transitions.is_empty() {
                    let state_ctx = Context::new();
                    self.state_contexts
                        .write()
                        .unwrap()
                        .insert(state.qualified_name().to_string(), state_ctx.clone());

                    for activity_name in &state.activities {
                        if let Some(behavior) = self.model.get_behavior(activity_name) {
                            self.spawn_activity(behavior, event, &state_ctx);
                        }
                    }

                    for (transition_name, timer_kind, constraint_name) in timer_transitions {
                        if let Some(transition) = self.model.get_transition(&transition_name) {
                            self.spawn_timer_transition(
                                state.qualified_name(),
                                transition,
                                timer_kind,
                                &constraint_name,
                                event,
                                &state_ctx,
                            );
                        }
                    }
                }

                // Handle initial transition
                if default_entry && !state.initial.is_empty() {
                    if let Some(initial_vertex) = self.model.get_vertex(&state.initial) {
                        if !initial_vertex.transitions.is_empty() {
                            if let Some(initial_transition) =
                                self.model.get_transition(&initial_vertex.transitions[0])
                            {
                                return Box::pin(self.execute_transition(
                                    &state.qualified_name(),
                                    initial_transition,
                                    event,
                                    ctx,
                                ))
                                .await;
                            }
                        }
                    }
                }

                Ok(Some(state.qualified_name().to_string()))
            }
            ElementVariant::Vertex(vertex) => {
                if is_kind(vertex.kind(), kind::CHOICE) {
                    // Handle choice pseudostate
                    for transition_name in &vertex.transitions {
                        if let Some(transition) = self.model.get_transition(transition_name) {
                            let guard_ok = self.evaluate_guard(&transition.guard, event, ctx);
                            if behavior_context::take_abort() {
                                return Ok(None);
                            }

                            if guard_ok {
                                return Box::pin(self.execute_transition(
                                    vertex.qualified_name(),
                                    transition,
                                    event,
                                    ctx,
                                ))
                                .await;
                            }
                        }
                    }
                }
                if is_kind(vertex.kind(), kind::SHALLOW_HISTORY)
                    || is_kind(vertex.kind(), kind::DEEP_HISTORY)
                {
                    return self.enter_history(vertex, event, ctx).await;
                }
                if is_kind(vertex.kind(), kind::ENTRY_POINT)
                    || is_kind(vertex.kind(), kind::EXIT_POINT)
                {
                    return self.enter_connection_point(vertex, event, ctx).await;
                }
                Ok(Some(vertex.qualified_name().to_string()))
            }
            _ => Ok(Some(element.qualified_name().to_string())),
        }
    }

    async fn enter_connection_point(
        &self,
        vertex: &crate::element::Vertex,
        event: &Event,
        ctx: &Context,
    ) -> TransitionOutcome {
        let mut result = vertex.qualified_name().to_string();

        for transition_name in &vertex.transitions {
            let Some(transition) = self.model.get_transition(transition_name) else {
                continue;
            };

            if !self.evaluate_guard(&transition.guard, event, ctx) {
                if behavior_context::take_abort() {
                    return Ok(None);
                }
                continue;
            }
            if behavior_context::take_abort() {
                return Ok(None);
            }

            let target_state =
                Box::pin(self.execute_transition(vertex.qualified_name(), transition, event, ctx))
                    .await?;
            let Some(target_state) = target_state else {
                return Ok(None);
            };

            if transition.target.is_empty() {
                result = target_state;
                continue;
            }

            return Ok(Some(target_state));
        }

        if is_kind(vertex.kind(), kind::EXIT_POINT) {
            let exit_point = crate::path::basename(vertex.qualified_name());
            return Err(HsmError::Runtime(format!(
                "unhandled_exit_point\0unhandled exit point \"{exit_point}\""
            )));
        }

        Ok(Some(result))
    }

    async fn enter_history(
        &self,
        vertex: &crate::element::Vertex,
        event: &Event,
        ctx: &Context,
    ) -> TransitionOutcome {
        let parent = crate::path::dirname(vertex.qualified_name()).to_string();
        let is_shallow = is_kind(vertex.kind(), kind::SHALLOW_HISTORY);
        let remembered = if is_shallow {
            self.shallow_history.lock().unwrap().get(&parent).cloned()
        } else {
            self.deep_history.lock().unwrap().get(&parent).cloned()
        };

        if let Some(target) = remembered {
            return self
                .enter_history_target(&parent, &target, is_shallow, event, ctx)
                .await;
        }

        for transition_name in &vertex.transitions {
            if let Some(transition) = self.model.get_transition(transition_name) {
                if !self.evaluate_guard(&transition.guard, event, ctx) {
                    if behavior_context::take_abort() {
                        return Ok(None);
                    }
                    continue;
                }
                if behavior_context::take_abort() {
                    return Ok(None);
                }
                return Box::pin(self.execute_transition(
                    vertex.qualified_name(),
                    transition,
                    event,
                    ctx,
                ))
                .await;
            }
        }

        if let Some(parent_state) = self.model.get_state(&parent) {
            if !parent_state.initial.is_empty() {
                if let Some(initial_vertex) = self.model.get_vertex(&parent_state.initial) {
                    if let Some(transition_name) = initial_vertex.transitions.first() {
                        if let Some(transition) = self.model.get_transition(transition_name) {
                            return Box::pin(
                                self.execute_transition(&parent, transition, event, ctx),
                            )
                            .await;
                        }
                    }
                }
            }
        }

        Ok(Some(vertex.qualified_name().to_string()))
    }

    async fn enter_history_target(
        &self,
        parent: &str,
        target: &str,
        default_last_entry: bool,
        event: &Event,
        ctx: &Context,
    ) -> TransitionOutcome {
        let mut enter_path = Vec::new();
        let mut current = target.to_string();
        while current != parent && !current.is_empty() && current != "/" {
            enter_path.push(current.clone());
            current = crate::path::dirname(&current).to_string();
        }
        enter_path.reverse();

        let mut result = parent.to_string();
        let last_index = enter_path.len().saturating_sub(1);
        for (index, entering) in enter_path.iter().enumerate() {
            if let Some(element) = self.model.members.get(entering) {
                let next = Box::pin(self.enter_state(
                    element,
                    event,
                    default_last_entry && index == last_index,
                    ctx,
                ))
                .await?;
                let Some(state) = next else {
                    return Ok(None);
                };
                result = state;
            }
        }
        Ok(Some(result))
    }

    fn remember_history(&self, leaf: &str) {
        let Some(updates) = self.model.history_updates.get(leaf) else {
            return;
        };

        let mut shallow_history = self.shallow_history.lock().unwrap();
        let mut deep_history = self.deep_history.lock().unwrap();
        for update in updates {
            shallow_history.insert(update.parent.clone(), update.shallow_child.clone());
            deep_history.insert(update.parent.clone(), update.deep_leaf.clone());
        }
    }

    fn timer_transitions_for_state(&self, state_name: &str) -> Vec<(String, TimerKind, String)> {
        let Some(state) = self.model.get_state(state_name) else {
            return Vec::new();
        };

        let transitions = state
            .vertex
            .transitions
            .iter()
            .filter_map(|transition_name| {
                let transition = self.model.get_transition(transition_name)?;
                self.timer_kind_for_transition(transition)
                    .map(|(kind, constraint)| (transition_name.clone(), kind, constraint))
            })
            .collect::<Vec<_>>();
        transitions
    }

    fn timer_kind_for_transition(
        &self,
        transition: &crate::element::Transition,
    ) -> Option<(TimerKind, String)> {
        for (name, kind) in [
            ("after", TimerKind::After),
            ("at", TimerKind::At),
            ("every", TimerKind::Every),
        ] {
            let constraint_name = crate::path::join(transition.qualified_name(), name);
            if self
                .model
                .get_constraint(&constraint_name)
                .is_some_and(|constraint| {
                    constraint.duration.is_some() || constraint.timepoint.is_some()
                })
            {
                return Some((kind, constraint_name));
            }
        }
        None
    }

    fn timer_delay(&self, constraint_name: &str, event: &Event, ctx: &Context) -> Option<Duration> {
        let constraint = self.model.get_constraint(constraint_name)?;
        if let Some(duration_fn) = constraint.duration {
            let instance = self.instance.read().unwrap();
            let duration = duration_fn(ctx, &*instance, event);
            if behavior_context::take_abort() {
                return None;
            }
            return Some(duration);
        }
        if let Some(timepoint_fn) = constraint.timepoint {
            let instance = self.instance.read().unwrap();
            let timepoint = timepoint_fn(ctx, &*instance, event);
            if behavior_context::take_abort() {
                return None;
            }
            return Some(
                timepoint
                    .duration_since(SystemTime::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        None
    }

    fn spawn_timer_transition(
        &self,
        source_state: &str,
        transition: &crate::element::Transition,
        kind: TimerKind,
        constraint_name: &str,
        _event: &Event,
        ctx: &Context,
    ) {
        let Some(event_name) = transition.events.first().cloned() else {
            return;
        };

        let hsm = self.clone();
        let source_state = source_state.to_string();
        let constraint_name = constraint_name.to_string();
        let timer_event = Event::time_event(event_name.clone());
        let ctx = ctx.clone();
        register_context_machine(&ctx, self);
        let Some(delay) = self.timer_delay(&constraint_name, &timer_event, &ctx) else {
            return;
        };
        let first_sleep = behavior_context::with_timer_registration(
            &self.id(),
            &source_state,
            &event_name,
            &ctx,
            || self.clock.sleep(delay),
        );

        tokio::spawn(async move {
            let mut sleep = Some(first_sleep);
            loop {
                if ctx.is_cancelled() {
                    break;
                }
                if let Some(current_sleep) = sleep.take() {
                    current_sleep.await;
                } else {
                    let Some(delay) = hsm.timer_delay(&constraint_name, &timer_event, &ctx) else {
                        break;
                    };
                    let current_sleep = behavior_context::with_timer_registration(
                        &hsm.id(),
                        &source_state,
                        &event_name,
                        &ctx,
                        || hsm.clock.sleep(delay),
                    );
                    current_sleep.await;
                }
                if ctx.is_cancelled() {
                    break;
                }
                let _ = hsm.dispatch(&ctx, timer_event.clone()).await;
                if kind != TimerKind::Every {
                    break;
                }
            }
        });
    }

    async fn exit_state(&self, state: &State, event: &Event, ctx: &Context) -> bool {
        // Cancel activities for this specific state
        let state_name = state.qualified_name();
        if let Some(state_ctx) = self.state_contexts.write().unwrap().remove(state_name) {
            state_ctx.cancel();
            let running = running_activity(&state_ctx);
            let cancelled = cancel_activities(&state_ctx);
            if !cancelled.is_empty() {
                let instance = self.instance.read().unwrap();
                for behavior in cancelled {
                    if running.as_deref() == Some(behavior.as_str()) {
                        continue;
                    }
                    instance.activity_cancelled(&behavior);
                }
            }
        }

        // Execute exit actions
        for exit_name in &state.exit {
            if let Some(behavior) = self.model.get_behavior(exit_name) {
                if !self.execute_behavior(behavior, event, ctx).await {
                    return false;
                }
            }
        }
        true
    }

    async fn execute_behavior(&self, behavior: &Behavior<T>, event: &Event, ctx: &Context) -> bool {
        // Determine which operation to execute based on behavior type
        let operation_future = if let Some(entry_fn) = behavior.entry {
            let mut instance = self.instance.write().unwrap();
            entry_fn(ctx, &mut *instance, event)
        } else if let Some(effect_fn) = behavior.effect {
            let mut instance = self.instance.write().unwrap();
            effect_fn(ctx, &mut *instance, event)
        } else if let Some(exit_fn) = behavior.exit {
            let mut instance = self.instance.write().unwrap();
            exit_fn(ctx, &mut *instance, event)
        } else if let Some(activity_fn) = behavior.activity {
            let mut instance = self.instance.write().unwrap();
            activity_fn(ctx, &mut *instance, event)
        } else if let Some(operation) = &behavior.operation {
            match operation {
                BehaviorOperation::Operation(operation_name) => {
                    return self
                        .execute_operation_by_name(operation_name, event, ctx)
                        .await;
                }
                BehaviorOperation::Observation {
                    observer,
                    source,
                    occurrence,
                } => {
                    return self
                        .execute_observation(*observer, source, occurrence, event, ctx)
                        .await;
                }
            }
        } else {
            return true;
        };

        await_behavior_future(operation_future).await
    }

    async fn execute_observation(
        &self,
        observer: OperationFn<T>,
        source: &str,
        occurrence: &str,
        event: &Event,
        ctx: &Context,
    ) -> bool {
        let observation =
            Event::observation(source.to_string(), occurrence.to_string(), event.clone());
        let observation_future = {
            let mut instance = self.instance.write().unwrap();
            observer(ctx, &mut *instance, &observation)
        };
        await_behavior_future(observation_future).await
    }

    fn evaluate_guard(&self, constraint_name: &str, event: &Event, ctx: &Context) -> bool {
        if constraint_name.is_empty() {
            return true;
        }

        let Some(constraint) = self.model.get_constraint(constraint_name) else {
            return true;
        };

        if let Some(guard_fn) = constraint.guard {
            let instance = self.instance.read().unwrap();
            return guard_fn(ctx, &*instance, event);
        }

        if let Some(operation_name) = &constraint.operation {
            if let Some(operation) = self.model.get_operation(operation_name) {
                if let Some(guard_fn) = operation.guard {
                    return behavior_context::with_operation_name(operation_name, || {
                        let instance = self.instance.read().unwrap();
                        guard_fn(ctx, &*instance, event)
                    });
                }
            }
        }

        true
    }

    async fn execute_operation_by_name(
        &self,
        operation_name: &str,
        event: &Event,
        ctx: &Context,
    ) -> bool {
        let Some(operation) = self.model.get_operation(operation_name) else {
            return true;
        };
        let Some(action) = operation.action else {
            return true;
        };
        let activity_behavior = crate::path::basename(operation_name).to_string();
        let mark_activity = is_active_activity(ctx, &activity_behavior);
        if mark_activity {
            behavior_context::begin_activity(ctx, &activity_behavior);
        }
        let operation_future = behavior_context::with_operation_name(operation_name, || {
            let mut instance = self.instance.write().unwrap();
            action(ctx, &mut *instance, event)
        });
        self.begin_operation();
        let ok = await_behavior_future(operation_future).await;
        let operation_finished = self.end_operation();
        if operation_finished && !self.is_draining() {
            if self.flush_reentrant_events(ctx).unwrap_or(false) {
                let _ = Box::pin(self.drain_queue(ctx)).await;
            }
        }
        if mark_activity {
            behavior_context::end_activity(ctx);
        }
        ok
    }

    fn spawn_activity(&self, behavior: &Behavior<T>, event: &Event, ctx: &Context) {
        if let Some(activity_fn) = behavior.activity {
            let behavior_id = crate::path::basename(behavior.qualified_name()).to_string();
            register_activity(ctx, &behavior_id);
            let mut instance = self.instance.write().unwrap();
            let future = activity_fn(ctx, &mut *instance, event);
            drop(instance); // Release the lock before spawning

            // Spawn the activity to run concurrently
            let hsm = self.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                register_context_machine(&ctx, &hsm);
                future.await;
                unregister_context_machine(&ctx, &hsm);
                if finish_activity(&ctx, &behavior_id) && !ctx.is_cancelled() {
                    let instance = hsm.instance.read().unwrap();
                    instance.activity_done(&behavior_id);
                }
            });
        } else if let Some(operation) = &behavior.operation {
            let operation = operation.clone();
            let event = event.clone();
            let ctx = ctx.clone();
            let hsm = self.clone();
            let behavior_id = match &operation {
                BehaviorOperation::Operation(operation_name) => {
                    crate::path::basename(operation_name).to_string()
                }
                BehaviorOperation::Observation { .. } => {
                    crate::path::basename(behavior.qualified_name()).to_string()
                }
            };
            register_activity(&ctx, &behavior_id);
            tokio::spawn(async move {
                register_context_machine(&ctx, &hsm);
                let completed = match operation {
                    BehaviorOperation::Operation(operation_name) => {
                        hsm.execute_operation_by_name(&operation_name, &event, &ctx)
                            .await
                    }
                    BehaviorOperation::Observation {
                        observer,
                        source,
                        occurrence,
                    } => {
                        hsm.execute_observation(observer, &source, &occurrence, &event, &ctx)
                            .await
                    }
                };
                unregister_context_machine(&ctx, &hsm);
                if completed && finish_activity(&ctx, &behavior_id) && !ctx.is_cancelled() {
                    let instance = hsm.instance.read().unwrap();
                    instance.activity_done(&behavior_id);
                }
            });
        }
    }
}

impl<T: Instance> ContextMachine for HSM<T> {
    fn id(&self) -> String {
        HSM::id(self)
    }

    fn qualified_name(&self) -> String {
        HSM::qualified_name(self)
    }

    fn state(&self) -> String {
        HSM::state(self)
    }

    fn is_started(&self) -> bool {
        HSM::is_started(self)
    }

    fn can_receive_dispatch(&self) -> bool {
        self.is_started() || self.is_draining()
    }

    fn dispatch_event(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.dispatch(ctx, event)
    }

    fn get_attribute_value(&self, name: &str) -> Option<AttributeValue> {
        self.get(name)
    }

    fn set_attribute_value(&self, name: &str, value: AttributeValue) -> Result<()> {
        self.set(name, value)
    }

    fn set_attribute_value_with_context(
        &self,
        ctx: &Context,
        name: &str,
        value: AttributeValue,
    ) -> Result<()> {
        self.set_with_context(ctx, name, value)
    }

    fn call_operation_value(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, args)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl<T: Instance> SnapshotTarget for HSM<T> {
    type Snapshot = Snapshot;

    fn take_snapshot_with_context(&self, _ctx: &Context) -> Result<Self::Snapshot> {
        self.take_snapshot()
    }
}

impl<T: Instance> DispatchTarget for HSM<T> {
    fn dispatch_with_context(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.dispatch(ctx, event)
    }
}

impl<T: Instance> StopTarget for HSM<T> {
    fn stop_with_context(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.stop(ctx)
    }
}

impl<T: Instance> RestartTarget for HSM<T> {
    fn restart_with_context(
        &self,
        ctx: &Context,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart(ctx)
    }
}

impl<T: Instance> AttributeGetTarget for HSM<T> {
    fn get_attribute(&self, name: &str) -> Option<AttributeValue> {
        self.get(name)
    }
}

impl<T: Instance, V: Into<AttributeValue>> AttributeSetTarget<V> for HSM<T> {
    fn set_attribute(&self, name: &str, value: V) -> Result<()> {
        self.set(name, value)
    }
}

impl<T: Instance> OperationCallTarget for HSM<T> {
    fn call_operation(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call(ctx, name)
    }

    fn call_operation_with_args(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, args)
    }
}

impl<T, D> StartDataTarget<D> for HSM<T>
where
    T: Instance,
    D: Any + Send + Sync + 'static,
{
    fn start_with_data_target(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.start_with_data(data)
    }
}

impl<T, D> RestartDataTarget<D> for HSM<T>
where
    T: Instance,
    D: Any + Send + Sync + 'static,
{
    fn restart_with_data_with_context(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart_with_data(ctx, data)
    }
}

impl<T: Instance> RuntimeIdentityTarget for HSM<T> {
    fn runtime_id(&self) -> String {
        self.id()
    }

    fn runtime_name(&self) -> String {
        self.name()
    }

    fn runtime_qualified_name(&self) -> String {
        self.qualified_name()
    }
}

impl<T: Instance> Clone for HSM<T> {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            instance: self.instance.clone(),
            current_state: self.current_state.clone(),
            queue: self.queue.clone(),
            deferred_events: self.deferred_events.clone(),
            shallow_history: self.shallow_history.clone(),
            deep_history: self.deep_history.clone(),
            attributes: self.attributes.clone(),
            context: self.context.clone(),
            clock: self.clock.clone(),
            id: self.id.clone(),
            name: self.name.clone(),
            data: self.data.clone(),
            state_contexts: self.state_contexts.clone(),
        }
    }
}
