// HSM Implementation

use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};

use crate::context::Context;
use crate::element::{Element, ElementVariant, Instance, State, Behavior};
use crate::event::{Event, initial_event};
use crate::kind::{self, is_kind};
use crate::model::Model;
use crate::queue::EventQueue;
use crate::error::{HsmError, Result};

pub struct HSM<T: Instance> {
    model: Arc<Model<T>>,
    instance: Arc<RwLock<T>>,
    current_state: Arc<RwLock<String>>,
    queue: Arc<Mutex<EventQueue>>,
    processing: Arc<AtomicBool>,
    context: Arc<Context>,
    pub state_contexts: Arc<RwLock<std::collections::HashMap<String, Context>>>,
}

impl<T: Instance> HSM<T> {
    pub fn new(instance: T, model: Model<T>) -> Self {
        let model = Arc::new(model);
        let instance = Arc::new(RwLock::new(instance));
        let initial_state = model.state.qualified_name().to_string();
        let context = Arc::new(Context::new());

        Self {
            model,
            instance,
            current_state: Arc::new(RwLock::new(initial_state)),
            queue: Arc::new(Mutex::new(EventQueue::new())),
            processing: Arc::new(AtomicBool::new(false)),
            context,
            state_contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn start(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let initial_event = initial_event();
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
    

    // Dispatch following exact signature: hsm.dispatch(ctx, Event)
    pub fn dispatch(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        // Add event to queue
        {
            let mut queue = self.queue.lock().unwrap();
            queue.push(event);
        }

        // Always process events when dispatch is called
        let hsm = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            // Simple approach: always process events synchronously
            while !hsm.queue.lock().unwrap().is_empty() {
                let event = {
                    let mut queue = hsm.queue.lock().unwrap();
                    queue.pop()
                };
                
                if let Some(event) = event {
                    hsm.process_single_event(event, &ctx).await;
                }
            }
            Ok(())
        })
    }

    async fn process_single_event(&self, event: Event, ctx: &Context) {
        let current_state = self.current_state.read().unwrap().clone();

        // Use optimized transition table if available
        if let Some(event_transitions) = self.model.transition_map.get(&current_state) {
            if let Some(transition_names) = event_transitions.get(&event.name) {
                for transition_name in transition_names {
                    if let Some(transition) = self.model.get_transition(transition_name) {
                        // Check guard
                        let guard_ok = if !transition.guard.is_empty() {
                            if let Some(constraint) =
                                self.model.get_constraint(&transition.guard)
                            {
                                if let Some(guard_fn) = constraint.guard {
                                    let instance = self.instance.read().unwrap();
                                    guard_fn(ctx, &*instance, &event)
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        } else {
                            true
                        };

                        if guard_ok {
                            if let Some(new_state) = self
                                .execute_transition(&current_state, transition, &event, ctx)
                                .await
                            {
                                *self.current_state.write().unwrap() = new_state;
                                return; // Event processed successfully
                            }
                        }
                    }
                }
            }
        }

        // Check if event is deferred (for future implementation)
        // For now, just ignore unknown events
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
                calculated_path = crate::model::Model::<T>::calculate_path_static(current_state, &transition.target);
                &calculated_path
            } else {
                return None;
            }
        } else {
            return None;
        };

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
                    self.state_contexts.write().unwrap().insert(state.qualified_name().to_string(), state_ctx.clone());
                    
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
                            let mut guard_ok = true;
                            if !transition.guard.is_empty() {
                                if let Some(constraint) =
                                    self.model.get_constraint(&transition.guard)
                                {
                                    if let Some(guard_fn) = constraint.guard {
                                        let instance = self.instance.read().unwrap();
                                        guard_ok = guard_fn(ctx, &*instance, event);
                                    }
                                }
                            }

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
                vertex.qualified_name().to_string()
            }
            _ => element.qualified_name().to_string(),
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
        } else {
            return;
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
            processing: Arc::new(AtomicBool::new(false)),
            context: self.context.clone(),
            state_contexts: self.state_contexts.clone(),
        }
    }
}
