use crate::element::*;
use crate::error::{HsmError, Result};
use crate::event::ANY_EVENT_NAME;
use crate::kind;
use crate::model::Model;
use crate::model_lifecycle::selected_model_validator;
use crate::path;

#[derive(Debug, Clone, Copy)]
pub struct DefaultModelValidator;

impl DefaultModelValidator {
    pub fn validate<T: Instance + 'static>(&self, model: &Model<T>) -> Result<()> {
        validate_model(model)
    }
}

impl<T: Instance + 'static> ModelValidator<T> for DefaultModelValidator {
    fn validate(&self, model: &Model<T>) -> Result<()> {
        validate_model(model)
    }
}

pub fn validate<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    if let Some(validator) = selected_model_validator(model) {
        return validator.validate(model);
    }
    validate_model(model)
}

// Validation function
fn validate_model<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    validate_model_name(model)?;

    if model.state.initial.is_empty() {
        return Err(HsmError::Validation(format!(
            "Initial state is required for state machine '{}'",
            model.qualified_name()
        )));
    }

    if !model.members.contains_key(&model.state.initial) {
        return Err(HsmError::Validation(format!(
            "Initial vertex '{}' not found for state machine '{}'",
            model.state.initial,
            model.qualified_name()
        )));
    }

    for (name, element) in &model.members {
        if let ElementVariant::State(state) = element {
            if !state.initial.is_empty() && !model.members.contains_key(&state.initial) {
                return Err(HsmError::Validation(format!(
                    "Initial vertex '{}' not found for state '{}'",
                    state.initial, name
                )));
            }
        }
    }

    validate_composite_transition_targets(model)?;

    for (name, element) in &model.members {
        match element {
            ElementVariant::Attribute(attribute) => {
                validate_attribute_declaration(model, name, attribute)?
            }
            ElementVariant::Operation(operation) => {
                validate_operation_declaration(model, name, operation)?
            }
            ElementVariant::Behavior(behavior) => {
                validate_behavior_owner(model, name, behavior)?;
                validate_behavior_operation(model, name, behavior)?
            }
            ElementVariant::Constraint(constraint) => {
                validate_constraint_owner(model, name)?;
                validate_constraint_operation(model, name, constraint)?
            }
            _ => {}
        }
    }

    for (name, element) in &model.members {
        validate_member_owner(model, name, element)?;
    }

    validate_state_history_cardinality(model)?;

    for (name, element) in &model.members {
        let ElementVariant::Transition(transition) = element else {
            continue;
        };

        if !is_model_endpoint(model, &transition.source) {
            return Err(HsmError::Validation(format!(
                "Transition '{}' source '{}' not found",
                name, transition.source
            )));
        }

        if !transition.target.is_empty() && !is_model_endpoint(model, &transition.target) {
            return Err(HsmError::Validation(format!(
                "Transition '{}' target '{}' not found",
                name, transition.target
            )));
        }

        validate_on_call_operations(model, name, transition)?;
        validate_top_level_transition_source(model, transition)?;
        validate_transition_event_names(model, name, transition)?;
        validate_transition_target_or_effect(model, name, transition)?;
        validate_transition_trigger(model, name, transition)?;
        validate_timer_transition_source(model, name, transition)?;
        validate_initial_transition(model, name, transition)?;
        validate_pseudostate_transition_triggers(model, name, transition)?;
        validate_entry_point_target(model, name, transition)?;
        validate_exit_point_handler(model, name, transition)?;
        validate_submachine_boundary(model, name, transition)?;
    }

    for (name, element) in &model.members {
        let ElementVariant::Vertex(vertex) = element else {
            continue;
        };

        if kind::is_kind(vertex.kind(), kind::INITIAL) {
            validate_initial_vertex_declaration(name, vertex)?;
        }
        if kind::is_kind(vertex.kind(), kind::ENTRY_POINT) {
            validate_entry_point_declaration(model, name, vertex)?;
        }
        if kind::is_kind(vertex.kind(), kind::SHALLOW_HISTORY)
            || kind::is_kind(vertex.kind(), kind::DEEP_HISTORY)
        {
            validate_history_declaration(model, name, vertex)?;
        }
    }

    // Check choice states have guardless fallback
    for (name, element) in &model.members {
        if let ElementVariant::Vertex(vertex) = element {
            if kind::is_kind(vertex.kind(), kind::CHOICE) {
                let Some(last_transition_name) = vertex.transitions.last() else {
                    return Err(HsmError::Validation(format!(
                        "Choice state '{}' must have a guardless fallback transition",
                        name
                    )));
                };
                let last_is_guardless = model
                    .get_transition(last_transition_name)
                    .is_some_and(|transition| transition.guard.is_empty());
                if !last_is_guardless {
                    return Err(HsmError::Validation(format!(
                        "Choice state '{}' must have a guardless fallback transition",
                        name
                    )));
                }

                for transition_name in vertex.transitions.iter().take(vertex.transitions.len() - 1)
                {
                    if model
                        .get_transition(transition_name)
                        .is_some_and(|transition| transition.guard.is_empty())
                    {
                        return Err(HsmError::Validation(format!(
                            "Choice state '{}' guardless fallback must be last",
                            name
                        )));
                    }
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
                    || !state.initial.is_empty()
                    || !state.deferred.is_empty()
                    || has_direct_child_state(model, name)
                {
                    return Err(HsmError::Validation(format!(
                        "Final state '{}' cannot have transitions, entry/exit actions, activities, initial transitions, defer events, or child states",
                        name
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_composite_transition_targets<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    for element in model.members.values() {
        let ElementVariant::Transition(transition) = element else {
            continue;
        };

        if transition.target.is_empty() {
            continue;
        }

        let Some(target) = model.get_state(&transition.target) else {
            continue;
        };

        if kind::is_kind(target.kind(), kind::FINAL_STATE) {
            continue;
        }

        if target.initial.is_empty() && has_direct_child_state(model, &transition.target) {
            return Err(HsmError::Validation(format!(
                "Composite state '{}' requires initial transition",
                transition.target
            )));
        }
    }

    Ok(())
}

fn has_direct_child_state<T: Instance + 'static>(model: &Model<T>, qualified_name: &str) -> bool {
    model.members.iter().any(|(member_name, element)| {
        matches!(element, ElementVariant::State(_)) && path::dirname(member_name) == qualified_name
    })
}

fn validate_state_history_cardinality<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    for (state_name, element) in &model.members {
        let ElementVariant::State(_) = element else {
            continue;
        };

        let mut has_shallow_history = false;
        let mut has_deep_history = false;
        for (member_name, member) in &model.members {
            let ElementVariant::Vertex(vertex) = member else {
                continue;
            };
            if path::dirname(member_name) != state_name {
                continue;
            }

            if kind::is_kind(vertex.kind(), kind::SHALLOW_HISTORY) {
                if has_shallow_history {
                    return Err(HsmError::Validation(format!(
                        "State '{}' has more than one shallow history vertex",
                        state_name
                    )));
                }
                has_shallow_history = true;
            }

            if kind::is_kind(vertex.kind(), kind::DEEP_HISTORY) {
                if has_deep_history {
                    return Err(HsmError::Validation(format!(
                        "State '{}' has more than one deep history vertex",
                        state_name
                    )));
                }
                has_deep_history = true;
            }
        }
    }

    Ok(())
}

fn validate_model_name<T: Instance + 'static>(model: &Model<T>) -> Result<()> {
    let model_name = model.qualified_name().trim_start_matches('/');
    if model_name.is_empty() {
        return Err(HsmError::Validation(
            "model name cannot be empty".to_string(),
        ));
    }

    if model_name.contains('/') {
        return Err(HsmError::Validation(format!(
            "model name '{}' cannot contain '/'",
            model_name
        )));
    }

    Ok(())
}

fn validate_attribute_declaration<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    attribute: &Attribute,
) -> Result<()> {
    if attribute.qualified_name() != name {
        return Err(HsmError::Validation(format!(
            "Attribute '{}' qualified name mismatch '{}'",
            name,
            attribute.qualified_name()
        )));
    }

    let attribute_name = model_member_name(model, name);
    if attribute_name.is_empty() {
        return Err(HsmError::Validation(
            "attribute name cannot be empty".to_string(),
        ));
    }

    if attribute_name.contains('/') {
        return Err(HsmError::Validation(format!(
            "attribute name \"{}\" cannot contain \"/\"",
            attribute_name
        )));
    }

    if let (Some(value_type), Some(default_value)) =
        (&attribute.value_type, &attribute.default_value)
    {
        if &default_value.value_type() != value_type {
            return Err(HsmError::Validation(format!(
                "attribute \"{}\" default does not match declared type",
                attribute_name
            )));
        }
    }

    Ok(())
}

fn validate_operation_declaration<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    operation: &Operation<T>,
) -> Result<()> {
    if operation.qualified_name() != name {
        return Err(HsmError::Validation(format!(
            "Operation '{}' qualified name mismatch '{}'",
            name,
            operation.qualified_name()
        )));
    }

    let operation_name = model_member_name(model, name);
    if operation_name.is_empty() {
        return Err(HsmError::Validation(
            "operation name cannot be empty".to_string(),
        ));
    }

    if operation_name.contains('/') {
        return Err(HsmError::Validation(format!(
            "operation name \"{}\" cannot contain \"/\"",
            operation_name
        )));
    }

    Ok(())
}

fn model_member_name<T: Instance + 'static>(model: &Model<T>, qualified_name: &str) -> String {
    let prefix = format!("{}/", model.qualified_name().trim_end_matches('/'));
    if let Some(relative) = qualified_name.strip_prefix(&prefix) {
        relative.to_string()
    } else {
        qualified_name.to_string()
    }
}

fn validate_member_owner<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    element: &ElementVariant<T>,
) -> Result<()> {
    let owner = path::dirname(name);
    if owner == model.qualified_name() || model.members.contains_key(owner) {
        return Ok(());
    }

    let member_name = member_name_from_existing_owner(model, name);
    if member_name.contains('/') {
        return Err(HsmError::Validation(format!(
            "{} name \"{}\" cannot contain \"/\"",
            declaration_kind(element),
            member_name
        )));
    }

    Err(HsmError::Validation(format!(
        "{} '{}' owner '{}' not found",
        declaration_kind(element),
        name,
        owner
    )))
}

fn member_name_from_existing_owner<T: Instance + 'static>(
    model: &Model<T>,
    qualified_name: &str,
) -> String {
    let mut current = path::dirname(qualified_name).to_string();
    while !current.is_empty() {
        if current == model.qualified_name() || model.members.contains_key(&current) {
            let prefix = format!("{}/", current.trim_end_matches('/'));
            if let Some(relative) = qualified_name.strip_prefix(&prefix) {
                return relative.to_string();
            }
        }

        let parent = path::dirname(&current);
        if parent == current {
            break;
        }
        current = parent.to_string();
    }

    model_member_name(model, qualified_name)
}

fn declaration_kind<T: Instance + 'static>(element: &ElementVariant<T>) -> &'static str {
    match element {
        ElementVariant::State(state) if kind::is_kind(state.kind(), kind::FINAL_STATE) => {
            "final state"
        }
        ElementVariant::State(state) if kind::is_kind(state.kind(), kind::SUBMACHINE_STATE) => {
            "submachine state"
        }
        ElementVariant::State(_) => "state",
        ElementVariant::Vertex(vertex) if kind::is_kind(vertex.kind(), kind::CHOICE) => "choice",
        ElementVariant::Vertex(vertex) if kind::is_kind(vertex.kind(), kind::SHALLOW_HISTORY) => {
            "shallow history"
        }
        ElementVariant::Vertex(vertex) if kind::is_kind(vertex.kind(), kind::DEEP_HISTORY) => {
            "deep history"
        }
        ElementVariant::Vertex(vertex) if kind::is_kind(vertex.kind(), kind::ENTRY_POINT) => {
            "entry point"
        }
        ElementVariant::Vertex(vertex) if kind::is_kind(vertex.kind(), kind::EXIT_POINT) => {
            "exit point"
        }
        ElementVariant::Vertex(_) => "vertex",
        ElementVariant::Transition(_) => "transition",
        ElementVariant::Behavior(_) => "behavior",
        ElementVariant::Observation(_) => "observation",
        ElementVariant::Validator(_) => "validator",
        ElementVariant::Finalizer(_) => "finalizer",
        ElementVariant::Constraint(_) => "constraint",
        ElementVariant::Operation(_) => "operation",
        ElementVariant::Attribute(_) => "attribute",
        ElementVariant::Event(_) => "event",
    }
}

fn validate_behavior_owner<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    behavior: &Behavior<T>,
) -> Result<()> {
    let owner = path::dirname(name);

    if behavior.entry.is_some() {
        return validate_state_behavior_owner(model, name, owner, "Entry", |state| &state.entry);
    }
    if behavior.exit.is_some() {
        return validate_state_behavior_owner(model, name, owner, "Exit", |state| &state.exit);
    }
    if behavior.activity.is_some() {
        return validate_state_behavior_owner(model, name, owner, "Activity", |state| {
            &state.activities
        });
    }
    if behavior.effect.is_some() {
        return validate_transition_behavior_owner(model, name, owner, "Effect");
    }

    if behavior.operation.is_some() {
        if let Some(state) = model.get_state(owner) {
            if state
                .entry
                .iter()
                .any(|behavior_name| behavior_name == name)
                || state.exit.iter().any(|behavior_name| behavior_name == name)
                || state
                    .activities
                    .iter()
                    .any(|behavior_name| behavior_name == name)
            {
                return Ok(());
            }
            return Err(HsmError::Validation(format!(
                "Behavior '{}' is not attached to state '{}'",
                name, owner
            )));
        }

        if let Some(transition) = model.get_transition(owner) {
            if transition
                .effect
                .iter()
                .any(|behavior_name| behavior_name == name)
            {
                return Ok(());
            }
            return Err(HsmError::Validation(format!(
                "Behavior '{}' is not attached to transition '{}'",
                name, owner
            )));
        }

        return Err(HsmError::Validation(format!(
            "Behavior '{}' must be declared inside a state or transition",
            name
        )));
    }

    Ok(())
}

fn validate_state_behavior_owner<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    owner: &str,
    behavior_kind: &str,
    references: fn(&State) -> &Vec<String>,
) -> Result<()> {
    if owner == model.qualified_name() {
        return Err(HsmError::Validation(format!(
            "{} actions are not allowed on top level state machine",
            behavior_kind
        )));
    }

    let Some(state) = model.get_state(owner) else {
        return Err(HsmError::Validation(format!(
            "{} behavior '{}' must be declared inside a state",
            behavior_kind, name
        )));
    };

    if !references(state)
        .iter()
        .any(|behavior_name| behavior_name == name)
    {
        return Err(HsmError::Validation(format!(
            "{} behavior '{}' is not attached to state '{}'",
            behavior_kind, name, owner
        )));
    }

    Ok(())
}

fn validate_transition_behavior_owner<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    owner: &str,
    behavior_kind: &str,
) -> Result<()> {
    let Some(transition) = model.get_transition(owner) else {
        return Err(HsmError::Validation(format!(
            "{} behavior '{}' must be declared inside a transition",
            behavior_kind, name
        )));
    };

    if !transition
        .effect
        .iter()
        .any(|behavior_name| behavior_name == name)
    {
        return Err(HsmError::Validation(format!(
            "{} behavior '{}' is not attached to transition '{}'",
            behavior_kind, name, owner
        )));
    }

    Ok(())
}

fn validate_constraint_owner<T: Instance + 'static>(model: &Model<T>, name: &str) -> Result<()> {
    let owner = path::dirname(name);
    let Some(transition) = model.get_transition(owner) else {
        return Err(HsmError::Validation(format!(
            "Constraint '{}' must be declared inside a transition",
            name
        )));
    };

    let timer_constraint = matches!(path::basename(name), "after" | "at" | "every")
        && path::dirname(name) == owner
        && model.get_constraint(name).is_some_and(|constraint| {
            constraint.duration.is_some() || constraint.timepoint.is_some()
        });

    if transition.guard != name && !timer_constraint {
        return Err(HsmError::Validation(format!(
            "Constraint '{}' is not attached to transition '{}'",
            name, owner
        )));
    }

    Ok(())
}

fn validate_behavior_operation<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    behavior: &Behavior<T>,
) -> Result<()> {
    let Some(operation_name) = &behavior.operation else {
        return Ok(());
    };
    let Some(operation_name) = operation_name.operation_name() else {
        return Ok(());
    };
    validate_operation_reference_name(model, operation_name)?;
    let Some(operation) = model.get_operation(operation_name) else {
        return Err(HsmError::Validation(format!(
            "Behavior '{}' missing operation '{}'",
            name, operation_name
        )));
    };
    if operation.action.is_none() {
        return Err(HsmError::Validation(format!(
            "Behavior '{}' missing operation '{}'",
            name, operation_name
        )));
    }
    Ok(())
}

fn validate_constraint_operation<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    constraint: &Constraint<T>,
) -> Result<()> {
    let Some(operation_name) = &constraint.operation else {
        return Ok(());
    };
    validate_operation_reference_name(model, operation_name)?;
    let Some(operation) = model.get_operation(operation_name) else {
        return Err(HsmError::Validation(format!(
            "Guard '{}' missing operation '{}'",
            name, operation_name
        )));
    };
    if operation.guard.is_none() {
        return Err(HsmError::Validation(format!(
            "Guard '{}' missing operation '{}'",
            name, operation_name
        )));
    }
    Ok(())
}

fn validate_operation_reference_name<T: Instance + 'static>(
    model: &Model<T>,
    qualified_name: &str,
) -> Result<()> {
    let operation_name = model_member_name(model, qualified_name);
    if operation_name.is_empty() {
        return Err(HsmError::Validation(
            "operation name cannot be empty".to_string(),
        ));
    }

    if operation_name.contains('/') {
        return Err(HsmError::Validation(format!(
            "operation name \"{}\" cannot contain \"/\"",
            operation_name
        )));
    }

    Ok(())
}

fn validate_on_call_operations<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    for event_name in &transition.events {
        let Some(operation_name) = event_name.strip_prefix("hsm_call:") else {
            continue;
        };

        validate_operation_reference_name(model, operation_name)?;
        let Some(operation) = model.get_operation(operation_name) else {
            return Err(HsmError::Validation(format!(
                "OnCall '{}' missing operation '{}'",
                name, operation_name
            )));
        };
        if operation.action.is_none() {
            return Err(HsmError::Validation(format!(
                "OnCall '{}' missing operation '{}'",
                name, operation_name
            )));
        }
    }

    Ok(())
}

fn validate_top_level_transition_source<T: Instance + 'static>(
    model: &Model<T>,
    transition: &Transition,
) -> Result<()> {
    if transition.source != model.qualified_name()
        || transition.target.is_empty()
        || !transition.events.is_empty()
    {
        return Ok(());
    }

    Err(HsmError::Validation(
        "Triggerless top level transitions with a target must also define a source".to_string(),
    ))
}

fn validate_transition_event_names<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    for event_name in &transition.events {
        if event_name.is_empty() {
            return Err(HsmError::Validation(format!(
                "Transition '{}' event name cannot be empty",
                name
            )));
        }

        if event_name == ANY_EVENT_NAME
            || event_name.starts_with("hsm_call:")
            || event_name.starts_with("hsm_timer_")
            || event_name.starts_with(model.qualified_name())
        {
            continue;
        }

        if event_name.contains('*') || event_name.contains('?') {
            return Err(HsmError::Validation(format!(
                "Transition '{}' wildcard event patterns are not supported; use AnyEvent",
                name
            )));
        }
    }

    Ok(())
}

fn validate_transition_trigger<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    let source_is_pseudostate = model
        .get_vertex(&transition.source)
        .is_some_and(|source| kind::is_kind(source.kind(), kind::PSEUDOSTATE));

    if !transition.events.is_empty() || source_is_pseudostate {
        return Ok(());
    }

    Err(HsmError::Validation(format!(
        "Transition '{}' has no trigger",
        name
    )))
}

fn validate_timer_transition_source<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    let has_timer_event = ["after", "at", "every"].iter().any(|name| {
        model
            .get_constraint(&path::join(transition.qualified_name(), name))
            .is_some_and(|constraint| {
                constraint.duration.is_some() || constraint.timepoint.is_some()
            })
    });
    if !has_timer_event || model.get_state(&transition.source).is_some() {
        return Ok(());
    }

    Err(HsmError::Validation(format!(
        "Timer transition '{}' can only be used where the source is a state",
        name
    )))
}

fn validate_initial_transition<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    let Some(source) = model.get_vertex(&transition.source) else {
        return Ok(());
    };

    if !kind::is_kind(source.kind(), kind::INITIAL) {
        return Ok(());
    }

    let owner = path::dirname(&transition.source);
    if transition.target != owner && !path::is_ancestor_or_equal(owner, &transition.target) {
        return Err(HsmError::Validation(format!(
            "Initial transition '{}' must target inside '{}'",
            name, owner
        )));
    }

    if !transition.guard.is_empty() {
        return Err(HsmError::Validation(format!(
            "Initial transition '{}' cannot have guard",
            name
        )));
    }

    if transition
        .events
        .iter()
        .any(|event_name| event_name != "hsm/initial")
    {
        return Err(HsmError::Validation(format!(
            "Initial transition '{}' cannot have triggers",
            name
        )));
    }

    Ok(())
}

fn validate_transition_target_or_effect<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    if !transition.target.is_empty() {
        return Ok(());
    }

    if let Some(source) = model.get_vertex(&transition.source) {
        if kind::is_kind(source.kind(), kind::INITIAL) {
            return Err(HsmError::Validation(format!(
                "Initial transition '{}' requires target",
                name
            )));
        }

        if kind::is_kind(source.kind(), kind::CHOICE)
            || kind::is_kind(source.kind(), kind::SHALLOW_HISTORY)
            || kind::is_kind(source.kind(), kind::DEEP_HISTORY)
        {
            return Err(HsmError::Validation(format!(
                "Pseudostate transition '{}' requires target",
                name
            )));
        }
    }

    if transition.effect.is_empty() {
        return Err(HsmError::Validation(format!(
            "Transition '{}' target or effect is required; internal transitions require an effect",
            name
        )));
    }

    Ok(())
}

fn validate_pseudostate_transition_triggers<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    if transition.events.is_empty() {
        return Ok(());
    }

    let Some(source) = model.get_vertex(&transition.source) else {
        return Ok(());
    };

    if kind::is_kind(source.kind(), kind::CHOICE)
        || kind::is_kind(source.kind(), kind::SHALLOW_HISTORY)
        || kind::is_kind(source.kind(), kind::DEEP_HISTORY)
        || kind::is_kind(source.kind(), kind::ENTRY_POINT)
        || kind::is_kind(source.kind(), kind::EXIT_POINT)
    {
        return Err(HsmError::Validation(format!(
            "Transition '{}' from pseudostate '{}' cannot have triggers",
            name, transition.source
        )));
    }

    Ok(())
}

fn is_model_endpoint<T: Instance + 'static>(model: &Model<T>, qualified_name: &str) -> bool {
    qualified_name == model.qualified_name() || model.get_vertex(qualified_name).is_some()
}

fn validate_initial_vertex_declaration(name: &str, vertex: &Vertex) -> Result<()> {
    if vertex.transitions.len() > 1 {
        return Err(HsmError::Validation(format!(
            "Initial '{}' cannot have multiple transitions",
            name
        )));
    }

    Ok(())
}

fn validate_entry_point_declaration<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    vertex: &Vertex,
) -> Result<()> {
    if vertex.transitions.len() > 1 {
        return Err(HsmError::Validation(format!(
            "Entry point '{}' cannot have multiple transitions",
            name
        )));
    }

    let Some(transition_name) = vertex.transitions.first() else {
        return Err(HsmError::Validation(format!(
            "Entry point '{}' requires target",
            name
        )));
    };
    let Some(transition) = model.get_transition(transition_name) else {
        return Ok(());
    };

    if !transition.guard.is_empty() {
        return Err(HsmError::Validation(format!(
            "Entry point '{}' cannot have a guard",
            name
        )));
    }

    if transition.target.is_empty() {
        return Err(HsmError::Validation(format!(
            "Entry point '{}' requires target",
            name
        )));
    }

    if let Some(target) = model.get_vertex(&transition.target) {
        if kind::is_kind(target.kind(), kind::ENTRY_POINT) {
            return Err(HsmError::Validation(format!(
                "Entry point '{}' cannot target entry point",
                name
            )));
        }

        if kind::is_kind(target.kind(), kind::EXIT_POINT) {
            return Err(HsmError::Validation(format!(
                "Entry point '{}' cannot target exit point",
                name
            )));
        }
    }

    let owner = path::dirname(name);
    if transition.target != owner && !path::is_ancestor_or_equal(owner, &transition.target) {
        return Err(HsmError::Validation(format!(
            "Entry point '{}' must target inside '{}'",
            name, owner
        )));
    }

    Ok(())
}

fn validate_history_declaration<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    vertex: &Vertex,
) -> Result<()> {
    let owner = path::dirname(name);
    if owner == model.qualified_name() {
        return Err(HsmError::Validation(format!(
            "History '{}' must be nested inside a state",
            name
        )));
    }

    if vertex.transitions.is_empty() {
        return Err(HsmError::Validation(format!(
            "History '{}' requires a default transition",
            name
        )));
    }

    for transition_name in &vertex.transitions {
        let Some(transition) = model.get_transition(transition_name) else {
            continue;
        };
        if transition.target.is_empty() {
            continue;
        }
        if transition.target != owner && !path::is_ancestor_or_equal(owner, &transition.target) {
            return Err(HsmError::Validation(format!(
                "History '{}' must target inside '{}'",
                name, owner
            )));
        }
    }

    Ok(())
}

fn validate_entry_point_target<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    let Some(target) = model.get_vertex(&transition.target) else {
        return Ok(());
    };
    if !kind::is_kind(target.kind(), kind::ENTRY_POINT) {
        return Ok(());
    }

    let Some(source) = model.get_vertex(&transition.source) else {
        return Ok(());
    };
    if kind::is_kind(source.kind(), kind::ENTRY_POINT) {
        return Ok(());
    }

    let boundary = path::dirname(&transition.target);
    let Some(boundary_state) = model.get_state(boundary) else {
        return Err(HsmError::Validation(format!(
            "Entry point target '{}' requires a submachine transition target",
            transition.target
        )));
    };

    if !kind::is_kind(boundary_state.kind(), kind::SUBMACHINE_STATE) {
        return Err(HsmError::Validation(format!(
            "Entry point target '{}' requires a submachine transition target",
            transition.target
        )));
    }

    if !kind::is_kind(source.kind(), kind::EXIT_POINT)
        && transition.source != boundary
        && path::is_ancestor_or_equal(boundary, &transition.source)
    {
        return Err(HsmError::Validation(format!(
            "Transition '{}' entry point target cannot be internal",
            name
        )));
    }

    Ok(())
}

fn validate_exit_point_handler<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    let Some(source) = model.get_vertex(&transition.source) else {
        return Ok(());
    };
    if !kind::is_kind(source.kind(), kind::EXIT_POINT) {
        return Ok(());
    }

    let boundary = enclosing_submachine_boundary(model, &transition.source);
    if boundary.is_empty() {
        return Err(HsmError::Validation(format!(
            "Transition '{}' exit point handler requires a submachine owner",
            name
        )));
    }

    if !path::is_ancestor_or_equal(&boundary, &transition.source) {
        return Err(HsmError::Validation(format!(
            "Transition '{}' missing exit point '{}'",
            name, transition.source
        )));
    }

    Ok(())
}

fn validate_submachine_boundary<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    transition: &Transition,
) -> Result<()> {
    if transition.target.is_empty() {
        return Ok(());
    }

    let source_boundary = enclosing_submachine_boundary(model, &transition.source);
    let target_boundary = enclosing_submachine_boundary(model, &transition.target);
    let source = model.get_vertex(&transition.source);
    let target = model.get_vertex(&transition.target);
    let owner = path::dirname(name);

    if !source_boundary.is_empty()
        && !path::is_ancestor_or_equal(&source_boundary, owner)
        && !source.is_some_and(|source| kind::is_kind(source.kind(), kind::EXIT_POINT))
    {
        return Err(HsmError::Validation(format!(
            "Transition '{}' has submachine internal source '{}'",
            name, transition.source
        )));
    }

    if !source_boundary.is_empty()
        && target_boundary != source_boundary
        && (target_boundary.is_empty()
            || !path::is_ancestor_or_equal(&source_boundary, &target_boundary))
        && !source.is_some_and(|source| kind::is_kind(source.kind(), kind::EXIT_POINT))
    {
        return Err(HsmError::Validation(format!(
            "Transition '{}' cannot target outside submachine boundary '{}'",
            name, source_boundary
        )));
    }

    if !target_boundary.is_empty()
        && source_boundary != target_boundary
        && !path::is_ancestor_or_equal(&target_boundary, owner)
        && !target.is_some_and(|target| kind::is_kind(target.kind(), kind::ENTRY_POINT))
        && !source.is_some_and(|source| kind::is_kind(source.kind(), kind::EXIT_POINT))
    {
        return Err(HsmError::Validation(format!(
            "Transition '{}' cannot target submachine internal state '{}'",
            name, transition.target
        )));
    }

    Ok(())
}

fn enclosing_submachine_boundary<T: Instance + 'static>(
    model: &Model<T>,
    qualified_name: &str,
) -> String {
    let mut current = path::dirname(qualified_name).to_string();
    while !current.is_empty() && current != "/" {
        if model
            .get_state(&current)
            .is_some_and(|state| kind::is_kind(state.kind(), kind::SUBMACHINE_STATE))
        {
            return current;
        }

        let parent = path::dirname(&current);
        if parent == current {
            break;
        }
        current = parent.to_string();
    }

    String::new()
}
