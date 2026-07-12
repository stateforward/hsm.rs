use crate::builder::{self, IntoObservationTarget, PartialElement};
use crate::element::{
    ActivityFn, AttributeValue, DurationFn, EffectFn, EntryFn, ExitFn, GuardFn, Instance,
    ModelFinalizer, ModelValidator, OperationFn, TimepointFn,
};
use crate::event::IntoEventName;
use crate::kind;
use crate::model::Model;

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

pub fn submachine_state<T: Instance + 'static>(
    name: &str,
    machine: Model<T>,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialSubmachineState {
        name: name.to_string(),
        machine,
        elements: partials,
    })
}

#[allow(non_snake_case)]
pub fn SubmachineState<T: Instance + 'static>(
    name: &str,
    machine: Model<T>,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    submachine_state(name, machine, partials)
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

pub fn entry_point<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEntryPoint {
        name: name.to_string(),
        elements: partials,
    })
}

#[allow(non_snake_case)]
pub fn EntryPoint<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    entry_point(name, partials)
}

pub fn exit_point<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialExitPoint {
        name: name.to_string(),
        elements: partials,
    })
}

#[allow(non_snake_case)]
pub fn ExitPoint<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    exit_point(name, partials)
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
pub fn on<T: Instance + 'static, E: IntoEventName>(event: E) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTrigger {
        events: vec![event.into_event_name()],
    })
}

#[allow(non_snake_case)]
pub fn On<T: Instance + 'static, E: IntoEventName>(event: E) -> Box<dyn PartialElement<T>> {
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

pub fn observe<T: Instance + 'static, E: IntoObservationTarget>(
    observer: OperationFn<T>,
    targets: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    builder::observe(observer, targets)
}

#[allow(non_snake_case)]
pub fn Observe<T: Instance + 'static, E: IntoObservationTarget>(
    observer: OperationFn<T>,
    targets: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    observe(observer, targets)
}

pub fn validator<T, V>(validator: V) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    V: ModelValidator<T> + 'static,
{
    builder::validator(validator)
}

#[allow(non_snake_case)]
pub fn Validator<T, V>(validator: V) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    V: ModelValidator<T> + 'static,
{
    builder::validator(validator)
}

pub fn finalizer<T, F>(finalizer: F) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    F: ModelFinalizer<T> + 'static,
{
    builder::finalizer(finalizer)
}

#[allow(non_snake_case)]
pub fn Finalizer<T, F>(finalizer: F) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    F: ModelFinalizer<T> + 'static,
{
    builder::finalizer(finalizer)
}

pub fn source<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialSource {
        source: path.to_string(),
    })
}

#[allow(non_snake_case)]
pub fn Source<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    source(path)
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
pub fn Defer<T: Instance + 'static, E: IntoEventName>(
    events: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    defer(events)
}

pub fn defer<T: Instance + 'static, E: IntoEventName>(
    events: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    builder::defer(events)
}
