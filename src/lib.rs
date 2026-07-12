// HSM Library - Hierarchical State Machine Implementation in Rust
// Following the official Rust HSM Framework Reference

pub mod api;
pub mod behavior_context;
pub mod builder;
pub mod context;
pub mod context_runtime;
pub mod dsl;
pub mod element;
pub mod error;
pub mod event;
pub mod group;
pub mod hsm_impl;
pub mod kind;
pub mod macro_builders;
pub mod macros;
pub mod model;
pub mod model_lifecycle;
pub mod model_validation;
pub mod path;
pub mod queue;
pub mod runtime;

// Re-export core types at the crate root
pub use api::*;
pub use builder::{
    BehaviorRole, IntoObservationTarget, PartialActivity, PartialAfter, PartialAt,
    PartialAttribute, PartialBehaviorOperation, PartialChoice, PartialDefer, PartialElement,
    PartialEntry, PartialEntryPoint, PartialEvery, PartialExit, PartialExitPoint,
    PartialFinalState, PartialFinalizer, PartialGuard, PartialGuardOperation,
    PartialGuardOperationRef, PartialHistory, PartialInitial, PartialObserve, PartialOnCall,
    PartialOnSet, PartialOperation, PartialSource, PartialState, PartialSubmachineState,
    PartialTarget, PartialTransition, PartialTrigger, PartialValidator,
};
pub use context::*;
pub use context_runtime::*;
pub use dsl::*;
pub use element::*;
pub use error::*;
pub use event::*;
pub use group::*;
pub use hsm_impl::*;
pub use kind::*;
pub use model::*;
pub use model_lifecycle::*;
pub use model_validation::*;
pub use path::*;
pub use queue::*;
pub use runtime::*;

// Re-export macro builders
pub use macro_builders::*;

// Core HSM namespace module for compatibility
pub mod hsm {
    pub use crate::*;
}
