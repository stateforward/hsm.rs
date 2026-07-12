// Builder functions for macros

use crate::builder::PartialElement;
use crate::builder::*;
use crate::element::{
    ActivityFn, DurationFn, EffectFn, EntryFn, ExitFn, GuardFn, Instance, ModelFinalizer,
    ModelValidator, OperationFn, TimepointFn,
};

/// Create a state element
pub fn create_state<T: Instance + 'static>(
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialState {
        name: name.to_string(),
        elements,
    })
}

/// Create an initial element
pub fn create_initial<T: Instance + 'static>(
    target: Box<dyn PartialElement<T>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialInitial {
        name: ".initial".to_string(),
        elements: vec![target],
    })
}

/// Create a final state element
pub fn create_final<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialFinalState {
        name: name.to_string(),
    })
}

/// Create a choice element
pub fn create_choice<T: Instance + 'static>(
    name: &str,
    transitions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialChoice {
        name: name.to_string(),
        elements: transitions,
    })
}

pub fn create_shallow_history<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialHistory {
        name: name.to_string(),
        kind: crate::kind::SHALLOW_HISTORY,
        elements: partials,
    })
}

pub fn create_deep_history<T: Instance + 'static>(
    name: &str,
    partials: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialHistory {
        name: name.to_string(),
        kind: crate::kind::DEEP_HISTORY,
        elements: partials,
    })
}

/// Create a transition element
pub fn create_transition<T: Instance + 'static>(
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTransition {
        name: String::new(),
        elements,
    })
}

/// Create a trigger element
pub fn create_trigger<T: Instance + 'static>(event: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTrigger {
        events: vec![event.to_string()],
    })
}

/// Create a source element
pub fn create_source<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialSource {
        source: path.to_string(),
    })
}

/// Create a target element
pub fn create_target<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTarget {
        target: path.to_string(),
    })
}

/// Create an entry action element
pub fn create_entry<T: Instance + 'static>(action: EntryFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialEntry {
        operations: vec![action],
    })
}

/// Create an exit action element
pub fn create_exit<T: Instance + 'static>(action: ExitFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialExit {
        operations: vec![action],
    })
}

/// Create an effect element
pub fn create_effect<T: Instance + 'static>(action: EffectFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialEffect {
        operations: vec![action],
    })
}

/// Create an observation element
pub fn create_observe<T: Instance + 'static>(
    observer: OperationFn<T>,
    targets: Vec<String>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialObserve { observer, targets })
}

pub fn create_validator<T, V>(validator: V) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    V: ModelValidator<T> + 'static,
{
    crate::builder::validator(validator)
}

pub fn create_finalizer<T, F>(finalizer: F) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    F: ModelFinalizer<T> + 'static,
{
    crate::builder::finalizer(finalizer)
}

/// Create a guard element
pub fn create_guard<T: Instance + 'static>(condition: GuardFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialGuard {
        expression: condition,
    })
}

/// Create an activity element
pub fn create_activity<T: Instance + 'static>(
    activity: ActivityFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialActivity {
        operations: vec![activity],
    })
}

/// Create an after timer element
pub fn create_after<T: Instance + 'static>(
    duration_fn: DurationFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialAfter { duration_fn })
}

/// Create an absolute timer element
pub fn create_at<T: Instance + 'static>(
    timepoint_fn: TimepointFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialAt { timepoint_fn })
}

/// Create an every timer element
pub fn create_every<T: Instance + 'static>(
    duration_fn: DurationFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialEvery { duration_fn })
}

/// Create a defer element
pub fn create_defer<T: Instance + 'static>(events: Vec<&str>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialDefer {
        events: events.into_iter().map(|s| s.to_string()).collect(),
    })
}
