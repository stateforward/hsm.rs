// HSM Library - Hierarchical State Machine Implementation in Rust
// Following the official Rust HSM Framework Reference

pub mod kind;
pub mod path;
pub mod context;
pub mod event;
pub mod element;
pub mod model;
pub mod queue;
pub mod hsm_impl;
pub mod builder;
pub mod macros;
pub mod error;
pub mod macro_builders;

// Re-export core types at the crate root
pub use kind::*;
pub use path::*;
pub use context::*;
pub use event::*;
pub use element::*;
pub use model::*;
pub use queue::*;
pub use hsm_impl::*;
pub use builder::*;
pub use error::*;

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

    model
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
pub fn start<T: Instance + 'static>(_ctx: &Context, instance: T, model: Model<T>) -> Result<HSM<T>> {
    Ok(HSM::new(instance, model))
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

pub fn final_state<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialFinalState {
        name: name.to_string(),
    })
}

// Alias for final_state to match other language APIs
pub fn r#final<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
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

// Package-level behavior definition functions (HSM Spec 5.2.3)
pub fn entry<T: Instance + 'static>(
    function: EntryFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEntry {
        operations: vec![function],
    })
}

pub fn exit<T: Instance + 'static>(
    function: ExitFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialExit {
        operations: vec![function],
    })
}

pub fn activity<T: Instance + 'static>(
    function: ActivityFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialActivity {
        operations: vec![function],
    })
}

pub fn transition<T: Instance + 'static>(
    conditions: Vec<Box<dyn PartialElement<T>>>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTransition {
        name: String::new(),
        elements: conditions,
    })
}

// Package-level condition definition functions (HSM Spec 5.2.4)
pub fn on<T: Instance + 'static>(event: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTrigger {
        events: vec![event.to_string()],
    })
}

pub fn guard<T: Instance + 'static>(function: GuardFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialGuard { expression: function })
}

pub fn effect<T: Instance + 'static>(
    function: EffectFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEffect {
        operations: vec![function],
    })
}

pub fn target<T: Instance + 'static>(path: &str) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialTarget {
        target: path.to_string(),
    })
}

// Timer functions for HSM Spec compliance
pub fn after<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialAfter { duration_fn })
}

pub fn every<T: Instance + 'static>(duration_fn: DurationFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(builder::PartialEvery { duration_fn })
}

// Core HSM namespace module for compatibility
pub mod hsm {
    pub use crate::*;
}

