// HSM Implementation

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock, atomic::AtomicBool};
use std::time::Duration;

use crate::context::Context;
use crate::element::{AttributeValue, Behavior, Element, ElementVariant, Instance, State};
use crate::error::Result;
use crate::event::{Event, initial_event};
use crate::kind::{self, is_kind};
use crate::model::Model;
use crate::queue::EventQueue;

pub type SleepFn = Arc<dyn Fn(Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
// Queue hooks are intentionally synchronous: Push, Pop, and Len must return
// their result/error directly to the runtime, not a future or channel.
pub type QueuePushFn = Arc<dyn Fn(&Context, Event) -> Result<()> + Send + Sync>;
pub type QueuePopFn = Arc<dyn Fn(&Context) -> Result<Option<Event>> + Send + Sync>;
pub type QueueLenFn = Arc<dyn Fn(&Context) -> Result<usize> + Send + Sync>;

const PROCESSED_RESULT: u8 = 1;
const DEFERRED_RESULT: u8 = 2;

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
pub struct Snapshot {
    pub ID: String,
    pub QualifiedName: String,
    pub State: String,
    pub Attributes: HashMap<String, AttributeValue>,
    pub QueueLen: usize,
    pub Events: Vec<EventDetail>,
}

pub struct HSM<T: Instance> {
    model: Arc<Model<T>>,
    instance: Arc<RwLock<T>>,
    current_state: Arc<RwLock<String>>,
    queue: Arc<Mutex<EventQueue>>,
    deferred_events: Arc<Mutex<VecDeque<Event>>>,
    shallow_history: Arc<Mutex<HashMap<String, String>>>,
    deep_history: Arc<Mutex<HashMap<String, String>>>,
    attributes: Arc<Mutex<HashMap<String, AttributeValue>>>,
    processing: Arc<AtomicBool>,
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

    pub fn new_with_config(instance: T, mut model: Model<T>, config: RuntimeConfig) -> Self {
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
        let context = Arc::new(Context::new());
        let clock = config.Clock.clone().unwrap_or_default().with_defaults();
        let queue = config
            .Queue
            .clone()
            .map(EventQueue::with_regular_queue)
            .unwrap_or_else(EventQueue::new);

        let mut attributes = HashMap::new();
        for (name, attribute) in &model.attributes {
            if let Some(default_value) = &attribute.default_value {
                attributes.insert(name.clone(), default_value.clone());
            }
        }

        Self {
            model,
            instance,
            current_state: Arc::new(RwLock::new(initial_state)),
            queue: Arc::new(Mutex::new(queue)),
            deferred_events: Arc::new(Mutex::new(VecDeque::new())),
            shallow_history: Arc::new(Mutex::new(HashMap::new())),
            deep_history: Arc::new(Mutex::new(HashMap::new())),
            attributes: Arc::new(Mutex::new(attributes)),
            processing: Arc::new(AtomicBool::new(false)),
            context,
            clock,
            id,
            name,
            data: config.Data.clone(),
            state_contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn start(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let mut initial_event = initial_event();
        initial_event.data = self.data.clone();
        self.dispatch(&self.context, initial_event)
    }

    pub fn state(&self) -> String {
        self.current_state.read().unwrap().clone()
    }

    pub fn current_state(&self) -> String {
        self.current_state.read().unwrap().clone()
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

    pub fn take_snapshot(&self) -> Snapshot {
        let state = self.state();
        let attributes = self.attributes.lock().unwrap().clone();
        let queue_len = self.queue.lock().unwrap().len_with_context(&self.context);
        let transitions_by_event = self.model.transition_map.get(&state);
        let mut events = Vec::new();

        if let Some(transitions_by_event) = transitions_by_event {
            for (event_name, transition_names) in transitions_by_event {
                for transition_name in transition_names {
                    if let Some(transition) = self.model.get_transition(transition_name) {
                        events.push(EventDetail {
                            Name: event_name.clone(),
                            Kind: kind::EVENT,
                            Target: if transition.target.is_empty() {
                                None
                            } else {
                                Some(transition.target.clone())
                            },
                            Guard: !transition.guard.is_empty(),
                            Schema: None,
                        });
                    }
                }
            }
        }

        Snapshot {
            ID: self.id(),
            QualifiedName: self.qualified_name(),
            State: state,
            Attributes: attributes,
            QueueLen: queue_len,
            Events: events,
        }
    }

    #[allow(non_snake_case)]
    pub fn TakeSnapshot(&self) -> Snapshot {
        self.take_snapshot()
    }

    // Getter methods for testing access
    pub fn instance(&self) -> &Arc<RwLock<T>> {
        &self.instance
    }

    pub fn instance_mut(&self) -> std::sync::RwLockWriteGuard<T> {
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

    pub fn set<V: Into<AttributeValue>>(&self, name: &str, value: V) {
        let qualified_name = self.qualify_attribute_name(name);
        let Some(attribute) = self.model.get_attribute(&qualified_name) else {
            return;
        };
        let value = value.into();
        if let Some(expected_type) = &attribute.value_type {
            if &value.value_type() != expected_type {
                return;
            }
        }

        {
            let mut attributes = self.attributes.lock().unwrap();
            if attributes.get(&qualified_name) == Some(&value) {
                return;
            }
            attributes.insert(qualified_name.clone(), value.clone());
        }

        let event = Event {
            kind: kind::EVENT,
            qualified_name: qualified_name.clone(),
            name: qualified_name,
            data: None,
        };
        {
            let mut queue = self.queue.lock().unwrap();
            if queue.push_with_context(&self.context, event).is_err() {
                return;
            }
        }
        self.run_to_completion(&self.context);
    }

    #[allow(non_snake_case)]
    pub fn Set<V: Into<AttributeValue>>(&self, name: &str, value: V) {
        self.set(name, value)
    }

    pub fn call(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let operation_name = self.qualify_operation_name(name);
        let event = Event::call(operation_name.clone());
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            hsm.execute_operation_by_name(&operation_name, &event, &ctx)
                .await;
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

    // Dispatch following exact signature: hsm.dispatch(ctx, Event)
    pub fn dispatch(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        {
            let mut queue = self.queue.lock().unwrap();
            if queue.push_with_context(ctx, event).is_err() {
                let _ = queue.push_with_context(ctx, Event::error_event());
            }
        }

        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            hsm.drain_queue(&ctx).await;
            Ok(())
        })
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
                    hsm.drain_queue(&ctx).await;
                });
            }
        };

        if tokio::runtime::Handle::try_current().is_ok() {
            let _ = std::thread::spawn(drain).join();
        } else {
            drain();
        }
    }

    async fn drain_queue(&self, ctx: &Context) {
        loop {
            let event_result = {
                let mut queue = self.queue.lock().unwrap();
                queue.pop_with_context(ctx)
            };

            match event_result {
                Ok(Some(event)) => {
                    self.process_single_event(event, ctx).await;
                }
                Ok(None) => break,
                Err(_) => {
                    self.process_single_event(Event::error_event(), ctx).await;
                    break;
                }
            };
        }
    }

    async fn process_single_event(&self, event: Event, ctx: &Context) -> u8 {
        let current_state = self.current_state.read().unwrap().clone();

        if self.is_deferred_in_state(&current_state, &event.name) {
            self.deferred_events.lock().unwrap().push_back(event);
            return DEFERRED_RESULT;
        }

        // Use optimized transition table if available
        if let Some(event_transitions) = self.model.transition_map.get(&current_state) {
            if let Some(transition_names) = event_transitions.get(&event.name) {
                for transition_name in transition_names {
                    if let Some(transition) = self.model.get_transition(transition_name) {
                        // Check guard
                        let guard_ok = self.evaluate_guard(&transition.guard, &event, ctx);

                        if guard_ok {
                            if let Some(new_state) = self
                                .execute_transition(&current_state, transition, &event, ctx)
                                .await
                            {
                                *self.current_state.write().unwrap() = new_state;
                                if self.current_state.read().unwrap().as_str() != current_state {
                                    self.replay_deferred_events();
                                }
                                return PROCESSED_RESULT; // Event processed successfully
                            }
                        }
                    }
                }
            }
        }

        PROCESSED_RESULT
    }

    fn is_deferred_in_state(&self, state: &str, event_name: &str) -> bool {
        self.model
            .deferred_map
            .get(state)
            .and_then(|events| events.get(event_name))
            .copied()
            .unwrap_or(false)
    }

    fn replay_deferred_events(&self) {
        let events: Vec<_> = {
            let mut deferred = self.deferred_events.lock().unwrap();
            deferred.drain(..).collect()
        };

        if !events.is_empty() {
            let _ = self
                .queue
                .lock()
                .unwrap()
                .prepend_regular_with_context(&self.context, events);
        }
    }

    async fn execute_transition(
        &self,
        current_state: &str,
        transition: &crate::element::Transition,
        event: &Event,
        ctx: &Context,
    ) -> Option<String> {
        // Get the appropriate path for this transition
        let calculated_path;
        let path = if let Some(p) = transition.paths.get(current_state) {
            // We have a pre-calculated path for the current state
            p
        } else if transition.source != current_state {
            // Transition is defined on an ancestor, calculate path from current state
            use crate::path::is_ancestor_or_equal;
            if is_ancestor_or_equal(&transition.source, current_state) {
                // Calculate path from current state to target
                calculated_path = crate::model::Model::<T>::calculate_path_static(
                    current_state,
                    &transition.target,
                );
                &calculated_path
            } else {
                return None;
            }
        } else {
            return None;
        };

        if !path.exit.is_empty() {
            self.remember_history(current_state);
        }

        // Exit states
        for exiting in &path.exit {
            if let Some(state) = self.model.get_state(exiting) {
                self.exit_state(state, event, ctx).await;
            }
        }

        // Execute effects
        for effect_name in &transition.effect {
            if let Some(behavior) = self.model.get_behavior(effect_name) {
                self.execute_behavior(behavior, event, ctx).await;
            }
        }

        // Handle internal transitions
        if is_kind(transition.element.kind, kind::INTERNAL) {
            return Some(current_state.to_string());
        }

        // Enter states
        let mut result = current_state.to_string();
        for entering in &path.enter {
            if let Some(element) = self.model.members.get(entering) {
                let default_entry = entering == &transition.target;
                result = Box::pin(self.enter_state(element, event, default_entry, ctx)).await;
                if default_entry {
                    return Some(result);
                }
            }
        }

        Some(transition.target.clone())
    }

    async fn enter_state(
        &self,
        element: &ElementVariant<T>,
        event: &Event,
        default_entry: bool,
        ctx: &Context,
    ) -> String {
        match element {
            ElementVariant::State(state) => {
                // Execute entry actions
                for entry_name in &state.entry {
                    if let Some(behavior) = self.model.get_behavior(entry_name) {
                        self.execute_behavior(behavior, event, ctx).await;
                    }
                }

                // Start activities (spawn them concurrently) with a new context for this state
                if !state.activities.is_empty() {
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
                }

                // Handle initial transition
                if default_entry && !state.initial.is_empty() {
                    if let Some(initial_vertex) = self.model.get_vertex(&state.initial) {
                        if !initial_vertex.transitions.is_empty() {
                            if let Some(initial_transition) =
                                self.model.get_transition(&initial_vertex.transitions[0])
                            {
                                if let Some(target_state) = Box::pin(self.execute_transition(
                                    &state.qualified_name(),
                                    initial_transition,
                                    event,
                                    ctx,
                                ))
                                .await
                                {
                                    return target_state;
                                }
                            }
                        }
                    }
                }

                state.qualified_name().to_string()
            }
            ElementVariant::Vertex(vertex) => {
                if is_kind(vertex.kind(), kind::CHOICE) {
                    // Handle choice pseudostate
                    for transition_name in &vertex.transitions {
                        if let Some(transition) = self.model.get_transition(transition_name) {
                            let guard_ok = self.evaluate_guard(&transition.guard, event, ctx);

                            if guard_ok {
                                if let Some(target_state) = Box::pin(self.execute_transition(
                                    vertex.qualified_name(),
                                    transition,
                                    event,
                                    ctx,
                                ))
                                .await
                                {
                                    return target_state;
                                }
                            }
                        }
                    }
                }
                if is_kind(vertex.kind(), kind::SHALLOW_HISTORY)
                    || is_kind(vertex.kind(), kind::DEEP_HISTORY)
                {
                    return self.enter_history(vertex, event, ctx).await;
                }
                vertex.qualified_name().to_string()
            }
            _ => element.qualified_name().to_string(),
        }
    }

    async fn enter_history(
        &self,
        vertex: &crate::element::Vertex,
        event: &Event,
        ctx: &Context,
    ) -> String {
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

        if let Some(transition_name) = vertex.transitions.first() {
            if let Some(transition) = self.model.get_transition(transition_name) {
                if let Some(target_state) = Box::pin(self.execute_transition(
                    vertex.qualified_name(),
                    transition,
                    event,
                    ctx,
                ))
                .await
                {
                    return target_state;
                }
            }
        }

        if let Some(parent_state) = self.model.get_state(&parent) {
            if !parent_state.initial.is_empty() {
                if let Some(initial_vertex) = self.model.get_vertex(&parent_state.initial) {
                    if let Some(transition_name) = initial_vertex.transitions.first() {
                        if let Some(transition) = self.model.get_transition(transition_name) {
                            if let Some(target_state) =
                                Box::pin(self.execute_transition(&parent, transition, event, ctx))
                                    .await
                            {
                                return target_state;
                            }
                        }
                    }
                }
            }
        }

        vertex.qualified_name().to_string()
    }

    async fn enter_history_target(
        &self,
        parent: &str,
        target: &str,
        default_last_entry: bool,
        event: &Event,
        ctx: &Context,
    ) -> String {
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
                result = Box::pin(self.enter_state(
                    element,
                    event,
                    default_last_entry && index == last_index,
                    ctx,
                ))
                .await;
            }
        }
        result
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

    async fn exit_state(&self, state: &State, event: &Event, ctx: &Context) {
        // Cancel activities for this specific state
        let state_name = state.qualified_name();
        if let Some(state_ctx) = self.state_contexts.write().unwrap().remove(state_name) {
            state_ctx.cancel();
        }

        // Execute exit actions
        for exit_name in &state.exit {
            if let Some(behavior) = self.model.get_behavior(exit_name) {
                self.execute_behavior(behavior, event, ctx).await;
            }
        }
    }

    async fn execute_behavior(&self, behavior: &Behavior<T>, event: &Event, ctx: &Context) {
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
        } else if let Some(operation_name) = &behavior.operation {
            self.execute_operation_by_name(operation_name, event, ctx)
                .await;
            return;
        } else {
            return;
        };

        operation_future.await;
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
                    let instance = self.instance.read().unwrap();
                    return guard_fn(ctx, &*instance, event);
                }
            }
        }

        true
    }

    async fn execute_operation_by_name(&self, operation_name: &str, event: &Event, ctx: &Context) {
        let Some(operation) = self.model.get_operation(operation_name) else {
            return;
        };
        let Some(action) = operation.action else {
            return;
        };
        let operation_future = {
            let mut instance = self.instance.write().unwrap();
            action(ctx, &mut *instance, event)
        };
        operation_future.await;
    }

    fn spawn_activity(&self, behavior: &Behavior<T>, event: &Event, ctx: &Context) {
        if let Some(activity_fn) = behavior.activity {
            let mut instance = self.instance.write().unwrap();
            let future = activity_fn(ctx, &mut *instance, event);
            drop(instance); // Release the lock before spawning

            // Spawn the activity to run concurrently
            tokio::spawn(async move {
                future.await;
            });
        } else if let Some(operation_name) = &behavior.operation {
            let operation_name = operation_name.clone();
            let event = event.clone();
            let ctx = ctx.clone();
            let hsm = self.clone();
            tokio::spawn(async move {
                hsm.execute_operation_by_name(&operation_name, &event, &ctx)
                    .await;
            });
        }
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
            processing: Arc::new(AtomicBool::new(false)),
            context: self.context.clone(),
            clock: self.clock.clone(),
            id: self.id.clone(),
            name: self.name.clone(),
            data: self.data.clone(),
            state_contexts: self.state_contexts.clone(),
        }
    }
}
