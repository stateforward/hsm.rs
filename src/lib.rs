// HSM Library - Hierarchical State Machine Implementation in Rust
// Following the official Rust HSM Framework Reference

pub mod builder;
pub mod context;
pub mod element;
pub mod error;
pub mod event;
pub mod hsm_impl;
pub mod kind;
pub mod macro_builders;
pub mod macros;
pub mod model;
pub mod path;
pub mod queue;

// Re-export core types at the crate root
pub use builder::*;
pub use context::*;
pub use element::*;
pub use error::*;
pub use event::*;
pub use hsm_impl::*;
pub use kind::*;
pub use model::*;
pub use path::*;
pub use queue::*;

// Re-export macro builders
pub use macro_builders::*;

// Define function to create models
pub fn define<T: Instance + 'static>(
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    let qualified_name = path::join("/", name);
    let mut model = Model::new(qualified_name.clone());
    let mut stack = vec![qualified_name];

    for element in elements {
        element.apply(&mut model, &mut stack);
    }

    // Build optimized lookup tables and calculate paths
    model.calculate_transition_paths();
    model.build_transition_table();
    model.build_deferred_table();
    model.build_history_table();

    model
}

#[allow(non_snake_case)]
pub fn Define<T: Instance + 'static>(
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    define(name, elements)
}

// Validation function
pub fn validate<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    // Check choice states have guardless fallback
    for (name, element) in &model.members {
        if let ElementVariant::Vertex(vertex) = element {
            if kind::is_kind(vertex.kind(), kind::CHOICE) {
                let mut has_guardless = false;
                for transition_name in &vertex.transitions {
                    if let Some(ElementVariant::Transition(transition)) =
                        model.members.get(transition_name)
                    {
                        // A transition is guardless if its guard field is empty
                        // (meaning no guard constraint was attached)
                        if transition.guard.is_empty() {
                            has_guardless = true;
                            break;
                        }
                    }
                }
                if !has_guardless {
                    return Err(HsmError::Validation(format!(
                        "Choice state '{}' must have a guardless fallback transition",
                        name
                    )));
                }
            }
        }
    }

    // Check final states don't have transitions
    for (name, element) in &model.members {
        if let ElementVariant::State(state) = element {
            if kind::is_kind(state.kind(), kind::FINAL_STATE) {
                if !state.vertex.transitions.is_empty()
                    || !state.entry.is_empty()
                    || !state.exit.is_empty()
                    || !state.activities.is_empty()
                {
                    return Err(HsmError::Validation(format!(
                        "Final state '{}' cannot have transitions, entry/exit actions, or activities",
                        name
                    )));
                }
            }
        }
    }

    Ok(())
}

// Start function following the reference pattern
pub fn start<T: Instance + 'static>(
    _ctx: &Context,
    instance: T,
    model: Model<T>,
) -> Result<HSM<T>> {
    Ok(HSM::new(instance, model))
}

pub fn start_with_config<T: Instance + 'static>(
    _ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Result<HSM<T>> {
    Ok(HSM::new_with_config(instance, model, config))
}

#[allow(non_snake_case)]
pub fn StartWithConfig<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Result<HSM<T>> {
    start_with_config(ctx, instance, model, config)
}

#[allow(non_snake_case)]
pub fn Config() -> RuntimeConfig {
    RuntimeConfig::default()
}

pub fn clock(sleep: Option<SleepFn>) -> Clock {
    Clock { Sleep: sleep }.with_defaults()
}

#[allow(non_snake_case)]
pub fn Clock(sleep: Option<SleepFn>) -> Clock {
    clock(sleep)
}

pub fn queue(push: QueuePushFn, pop: QueuePopFn, len: QueueLenFn) -> RuntimeQueue {
    RuntimeQueue::new(push, pop, len)
}

#[allow(non_snake_case)]
pub fn Queue(push: QueuePushFn, pop: QueuePopFn, len: QueueLenFn) -> RuntimeQueue {
    queue(push, pop, len)
}

#[allow(non_snake_case)]
pub fn ID<T: Instance + 'static>(machine: &HSM<T>) -> String {
    machine.id()
}

#[allow(non_snake_case)]
pub fn Name<T: Instance + 'static>(machine: &HSM<T>) -> String {
    machine.name()
}

#[allow(non_snake_case)]
pub fn QualifiedName<T: Instance + 'static>(machine: &HSM<T>) -> String {
    machine.qualified_name()
}

#[allow(non_snake_case)]
pub fn Data<T: Instance + 'static>(
    machine: &HSM<T>,
) -> Option<std::sync::Arc<dyn std::any::Any + Send + Sync>> {
    machine.data()
}

#[allow(non_snake_case)]
pub fn Call<T: Instance + 'static>(
    ctx: &Context,
    machine: &HSM<T>,
    name: &str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
    machine.call(ctx, name)
}

#[allow(non_snake_case)]
pub fn TakeSnapshot<T: Instance + 'static>(_ctx: &Context, machine: &HSM<T>) -> Snapshot {
    machine.take_snapshot()
}

// Package-level state definition functions (HSM Spec 5.2.2)
pub fn state<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialState {
        name: name.to_string(),
        elements: vec![],
    })
}

pub fn state_with_behaviors<T: Instance + 'static>(
    name: &str,
    behaviors: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialState {
        name: name.to_string(),
        elements: behaviors,
    })
}

#[allow(non_snake_case)]
pub fn State<T: Instance + 'static>(
    name: &str,
    behaviors: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    state_with_behaviors(name, behaviors)
}

pub fn initial<T: Instance + 'static>() -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialInitial {
        name: ".initial".to_string(),
        elements: vec![],
    })
}

pub fn initial_with_target<T: Instance + 'static>(
    target: Box<dyn PartialElement<T>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialInitial {
        name: ".initial".to_string(),
        elements: vec![target],
    })
}

#[allow(non_snake_case)]
pub fn Initial<T: Instance + 'static>(
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialInitial {
        name: ".initial".to_string(),
        elements,
    })
}

pub fn final_state<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialFinalState {
        name: name.to_string(),
    })
}

// Alias for final_state to match other language APIs
pub fn r#final<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    final_state(name)
}

#[allow(non_snake_case)]
pub fn Final<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    final_state(name)
}

pub fn choice<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialChoice {
        name: name.to_string(),
        elements: vec![],
    })
}

pub fn choice_with_transitions<T: Instance + 'static>(
    name: &str,
    transitions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialChoice {
        name: name.to_string(),
        elements: transitions,
    })
}

#[allow(non_snake_case)]
pub fn Choice<T: Instance + 'static>(
    name: &str,
    transitions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    choice_with_transitions(name, transitions)
}

pub fn shallow_history<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialHistory {
        name: name.to_string(),
        kind: kind::SHALLOW_HISTORY,
        elements: partials,
    })
}

#[allow(non_snake_case)]
pub fn ShallowHistory<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    shallow_history(name, partials)
}

pub fn deep_history<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialHistory {
        name: name.to_string(),
        kind: kind::DEEP_HISTORY,
        elements: partials,
    })
}

#[allow(non_snake_case)]
pub fn DeepHistory<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    deep_history(name, partials)
}

// Package-level behavior definition functions (HSM Spec 5.2.3)
pub fn entry<T: Instance + 'static>(function: EntryFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEntry {
        operations: vec![function],
    })
}

#[allow(non_snake_case)]
pub fn Entry<T: Instance + 'static>(function: EntryFn<T>) -> Box<dyn PartialElement<T>> {
    entry(function)
}

pub fn exit<T: Instance + 'static>(function: ExitFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialExit {
        operations: vec![function],
    })
}

#[allow(non_snake_case)]
pub fn Exit<T: Instance + 'static>(function: ExitFn<T>) -> Box<dyn PartialElement<T>> {
    exit(function)
}

pub fn activity<T: Instance + 'static>(function: ActivityFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialActivity {
        operations: vec![function],
    })
}

#[allow(non_snake_case)]
pub fn Activity<T: Instance + 'static>(function: ActivityFn<T>) -> Box<dyn PartialElement<T>> {
    activity(function)
}

pub fn transition<T: Instance + 'static>(
    conditions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTransition {
        name: String::new(),
        elements: conditions,
    })
}

#[allow(non_snake_case)]
pub fn Transition<T: Instance + 'static>(
    conditions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    transition(conditions)
}

// Package-level condition definition functions (HSM Spec 5.2.4)
pub fn on<T: Instance + 'static>(event: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTrigger {
        events: vec![event.to_string()],
    })
}

#[allow(non_snake_case)]
pub fn On<T: Instance + 'static>(event: &str) -> Box<dyn PartialElement<T>> {
    on(event)
}

pub fn on_call<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialOnCall {
        name: name.to_string(),
    })
}

#[allow(non_snake_case)]
pub fn OnCall<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    on_call(name)
}

pub fn on_set<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialOnSet {
        name: name.to_string(),
    })
}

#[allow(non_snake_case)]
pub fn OnSet<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    on_set(name)
}

pub fn when<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    on_set(name)
}

#[allow(non_snake_case)]
pub fn When<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    on_set(name)
}

pub fn attribute<T: Instance + 'static, V: Into<AttributeValue>>(
    name: &str,
    default_value: V,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialAttribute {
        name: name.to_string(),
        default_value: Some(default_value.into()),
    })
}

#[allow(non_snake_case)]
pub fn Attribute<T: Instance + 'static, V: Into<AttributeValue>>(
    name: &str,
    default_value: V,
) -> Box<dyn PartialElement<T>> {
    attribute(name, default_value)
}

pub fn guard<T: Instance + 'static>(function: GuardFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialGuard {
        expression: function,
    })
}

#[allow(non_snake_case)]
pub fn Guard<T: Instance + 'static>(function: GuardFn<T>) -> Box<dyn PartialElement<T>> {
    guard(function)
}

pub fn operation<T: Instance + 'static>(
    name: &str,
    function: OperationFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialOperation {
        name: name.to_string(),
        action: function,
    })
}

#[allow(non_snake_case)]
pub fn Operation<T: Instance + 'static>(
    name: &str,
    function: OperationFn<T>,
) -> Box<dyn PartialElement<T>> {
    operation(name, function)
}

pub fn guard_operation<T: Instance + 'static>(
    name: &str,
    function: GuardFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialGuardOperation {
        name: name.to_string(),
        guard: function,
    })
}

pub fn entry_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialBehaviorOperation {
        name: name.to_string(),
        role: builder::BehaviorRole::Entry,
    })
}

pub fn exit_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialBehaviorOperation {
        name: name.to_string(),
        role: builder::BehaviorRole::Exit,
    })
}

pub fn activity_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialBehaviorOperation {
        name: name.to_string(),
        role: builder::BehaviorRole::Activity,
    })
}

pub fn effect_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialBehaviorOperation {
        name: name.to_string(),
        role: builder::BehaviorRole::Effect,
    })
}

pub fn guard_operation_ref<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialGuardOperationRef {
        name: name.to_string(),
    })
}

pub fn effect<T: Instance + 'static>(function: EffectFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEffect {
        operations: vec![function],
    })
}

#[allow(non_snake_case)]
pub fn Effect<T: Instance + 'static>(function: EffectFn<T>) -> Box<dyn PartialElement<T>> {
    effect(function)
}

pub fn target<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTarget {
        target: path.to_string(),
    })
}

#[allow(non_snake_case)]
pub fn Target<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    target(path)
}

// Timer functions for HSM Spec compliance
pub fn after<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialAfter { duration_fn })
}

#[allow(non_snake_case)]
pub fn After<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    after(duration_fn)
}

pub fn at<T: Instance + 'static>(timepoint_fn: TimepointFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialAt { timepoint_fn })
}

#[allow(non_snake_case)]
pub fn At<T: Instance + 'static>(timepoint_fn: TimepointFn<T>) -> Box<dyn PartialElement<T>> {
    at(timepoint_fn)
}

pub fn every<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEvery { duration_fn })
}

#[allow(non_snake_case)]
pub fn Every<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    every(duration_fn)
}

#[allow(non_snake_case)]
pub fn Defer<T: Instance + 'static>(events: Vec<&str>) -> Box<dyn PartialElement<T>> {
    builder::defer(events)
}

// Core HSM namespace module for compatibility
pub mod hsm {
    pub use crate::*;
}
