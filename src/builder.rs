// Builder System

use std::collections::HashMap;
use std::sync::Arc;

use crate::element::{
    ActivityFn, Attribute, AttributeValue, Behavior, BehaviorOperation, Constraint, DurationFn,
    EffectFn, Element, ElementVariant, EntryFn, ExitFn, FinalizerElement, GuardFn, Instance,
    ModelFinalizer, ModelValidator, NamedElement, Observation, Operation, OperationFn, State,
    TimepointFn, Transition, ValidatorElement, Vertex,
};
use crate::event::{Event, IntoEventName, call_trigger_name};
use crate::kind;
use crate::model::Model;
use crate::path::{basename, dirname, join};

// Resolve relative paths with proper ".." handling
fn resolve_relative_path(base: &str, path: &str) -> String {
    if path.starts_with('/') {
        return path.to_string();
    }

    let mut components: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    let path_components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    for component in path_components {
        match component {
            ".." => {
                components.pop();
            }
            "." => {
                // Current directory, do nothing
            }
            _ => {
                components.push(component);
            }
        }
    }

    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn resolve_model_reference<T: Instance>(model: &Model<T>, stack: &[String], name: &str) -> String {
    if name.starts_with('/') {
        return name.to_string();
    }

    let current_qn = stack
        .last()
        .cloned()
        .unwrap_or_else(|| model.state.qualified_name().to_string());
    let owner_qn = match model.members.get(&current_qn) {
        Some(ElementVariant::Transition(_)) => dirname(&current_qn).to_string(),
        _ => current_qn,
    };
    let local_name = join(&owner_qn, name);
    if model.members.contains_key(&local_name) {
        local_name
    } else {
        join(model.state.qualified_name(), name)
    }
}

fn member_name_under(root: &str, qualified_name: &str) -> String {
    let prefix = format!("{}/", root.trim_end_matches('/'));
    if let Some(relative) = qualified_name.strip_prefix(&prefix) {
        relative.to_string()
    } else {
        qualified_name.to_string()
    }
}

fn submachine_operation_rewrites<T: Instance>(
    child: &Model<T>,
    parent_root: &str,
) -> HashMap<String, String> {
    child
        .members
        .iter()
        .filter_map(|(name, element)| {
            if !matches!(element, ElementVariant::Operation(_)) {
                return None;
            }

            let operation_name = member_name_under(child.state.qualified_name(), name);
            Some((name.clone(), join(parent_root, &operation_name)))
        })
        .collect()
}

fn submachine_attribute_rewrites<T: Instance>(
    child: &Model<T>,
    parent_root: &str,
) -> HashMap<String, String> {
    child
        .attributes
        .keys()
        .map(|name| {
            let attribute_name = member_name_under(child.state.qualified_name(), name);
            (name.clone(), join(parent_root, &attribute_name))
        })
        .collect()
}

fn rewrite_submachine_operation_reference(
    reference: &str,
    rewrites: &HashMap<String, String>,
) -> String {
    if let Some(operation) = reference.strip_prefix("hsm_call:") {
        return format!(
            "hsm_call:{}",
            rewrite_submachine_operation_reference(operation, rewrites)
        );
    }

    rewrites
        .get(reference)
        .cloned()
        .unwrap_or_else(|| reference.to_string())
}

fn rewrite_submachine_event_reference(
    reference: &str,
    operation_rewrites: &HashMap<String, String>,
    attribute_rewrites: &HashMap<String, String>,
) -> String {
    if let Some(operation) = reference.strip_prefix("hsm_call:") {
        return format!(
            "hsm_call:{}",
            rewrite_submachine_operation_reference(operation, operation_rewrites)
        );
    }

    operation_rewrites
        .get(reference)
        .or_else(|| attribute_rewrites.get(reference))
        .cloned()
        .unwrap_or_else(|| reference.to_string())
}

fn rewrite_submachine_references<T: Instance>(
    element: ElementVariant<T>,
    operation_rewrites: &HashMap<String, String>,
    attribute_rewrites: &HashMap<String, String>,
) -> ElementVariant<T> {
    if operation_rewrites.is_empty() && attribute_rewrites.is_empty() {
        return element;
    }

    match element {
        ElementVariant::Transition(mut transition) => {
            transition.events = transition
                .events
                .into_iter()
                .map(|event| {
                    rewrite_submachine_event_reference(
                        &event,
                        operation_rewrites,
                        attribute_rewrites,
                    )
                })
                .collect();
            ElementVariant::Transition(transition)
        }
        ElementVariant::Behavior(mut behavior) => {
            behavior.operation = behavior.operation.map(|operation| match operation {
                BehaviorOperation::Operation(name) => BehaviorOperation::Operation(
                    rewrite_submachine_operation_reference(&name, operation_rewrites),
                ),
                BehaviorOperation::Observation {
                    observer,
                    source,
                    occurrence,
                } => BehaviorOperation::Observation {
                    observer,
                    source,
                    occurrence,
                },
            });
            ElementVariant::Behavior(behavior)
        }
        ElementVariant::Observation(mut observation) => {
            observation.targets = observation
                .targets
                .into_iter()
                .map(|target| {
                    rewrite_submachine_event_reference(
                        &target,
                        operation_rewrites,
                        attribute_rewrites,
                    )
                })
                .collect();
            ElementVariant::Observation(observation)
        }
        ElementVariant::Constraint(mut constraint) => {
            constraint.operation = constraint.operation.map(|operation| {
                rewrite_submachine_operation_reference(&operation, operation_rewrites)
            });
            ElementVariant::Constraint(constraint)
        }
        ElementVariant::Operation(mut operation) => {
            if let Some(qualified_name) = operation_rewrites.get(operation.qualified_name()) {
                operation.element.qualified_name = qualified_name.clone();
            }
            ElementVariant::Operation(operation)
        }
        ElementVariant::Attribute(mut attribute) => {
            if let Some(qualified_name) = attribute_rewrites.get(attribute.qualified_name()) {
                attribute.element.qualified_name = qualified_name.clone();
            }
            ElementVariant::Attribute(attribute)
        }
        _ => element,
    }
}

fn transition_endpoint_context<T: Instance>(model: &Model<T>, stack: &[String]) -> String {
    for qualified_name in stack.iter().rev() {
        if qualified_name == model.state.qualified_name()
            || model.get_state(qualified_name).is_some()
        {
            return qualified_name.clone();
        }
    }

    model.state.qualified_name().to_string()
}

fn structural_member_count<T: Instance>(model: &Model<T>) -> usize {
    1 + model
        .members
        .values()
        .filter(|element| {
            matches!(
                element,
                ElementVariant::State(_)
                    | ElementVariant::Vertex(_)
                    | ElementVariant::Transition(_)
            )
        })
        .count()
}

pub trait PartialElement<T: Instance>: Send + Sync {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>);
}

pub trait IntoObservationTarget {
    fn into_observation_target(self) -> String;
}

fn unique_model_hook_name<T: Instance>(model: &Model<T>, prefix: &str) -> String {
    let owner_qn = model.state.qualified_name().to_string();
    let mut index = 0;

    loop {
        let qualified_name = join(&owner_qn, &format!("{}_{}", prefix, index));
        if !model.members.contains_key(&qualified_name) {
            return qualified_name;
        }
        index += 1;
    }
}

impl IntoObservationTarget for &str {
    fn into_observation_target(self) -> String {
        self.to_string()
    }
}

impl IntoObservationTarget for String {
    fn into_observation_target(self) -> String {
        self
    }
}

impl IntoObservationTarget for &String {
    fn into_observation_target(self) -> String {
        self.clone()
    }
}

impl IntoObservationTarget for Event {
    fn into_observation_target(self) -> String {
        self.name
    }
}

impl IntoObservationTarget for &Event {
    fn into_observation_target(self) -> String {
        self.name.clone()
    }
}

// Partial implementations
pub struct PartialState<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialState<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let qualified_name = join(&owner_qn, &self.name);

        if !matches!(
            model.members.get(&qualified_name),
            Some(ElementVariant::State(_))
        ) {
            let state = State {
                vertex: Vertex {
                    element: NamedElement {
                        kind: kind::STATE,
                        qualified_name: qualified_name.clone(),
                    },
                    transitions: Vec::new(),
                },
                initial: String::new(),
                entry: Vec::new(),
                exit: Vec::new(),
                activities: Vec::new(),
                deferred: Vec::new(),
            };

            model.set_member(qualified_name.clone(), ElementVariant::State(state));
        }
        stack.push(qualified_name);

        for element in self.elements {
            element.apply(model, stack);
        }

        stack.pop();
    }
}

pub struct PartialSubmachineState<T: Instance> {
    pub name: String,
    pub machine: Model<T>,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialSubmachineState<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let qualified_name = join(&owner_qn, &self.name);
        let child = self.machine.rebase(qualified_name.clone());

        let mut boundary = child.state.clone();
        boundary.vertex.element.kind = kind::SUBMACHINE_STATE;
        boundary.vertex.element.qualified_name = qualified_name.clone();
        model.set_member(qualified_name.clone(), ElementVariant::State(boundary));

        let operation_rewrites =
            submachine_operation_rewrites(&child, model.state.qualified_name());
        let attribute_rewrites =
            submachine_attribute_rewrites(&child, model.state.qualified_name());

        for (member_name, member) in child.members {
            let rewritten_name = operation_rewrites
                .get(&member_name)
                .or_else(|| attribute_rewrites.get(&member_name))
                .cloned()
                .unwrap_or(member_name);
            let rewritten_member =
                rewrite_submachine_references(member, &operation_rewrites, &attribute_rewrites);
            model.set_member(rewritten_name, rewritten_member);
        }

        for (attribute_name, mut attribute) in child.attributes {
            let rewritten_name = attribute_rewrites
                .get(&attribute_name)
                .cloned()
                .unwrap_or(attribute_name);
            if let Some(qualified_name) = attribute_rewrites.get(attribute.qualified_name()) {
                attribute.element.qualified_name = qualified_name.clone();
            }
            model.attributes.insert(rewritten_name, attribute);
        }

        stack.push(qualified_name);
        for element in self.elements {
            element.apply(model, stack);
        }
        stack.pop();
    }
}

pub struct PartialTransition<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialTransition<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let transition_name = if self.name.is_empty() {
            format!("transition_{}", structural_member_count(model))
        } else {
            self.name
        };
        let qualified_name = join(&owner_qn, &transition_name);

        let mut transition = Transition {
            element: NamedElement {
                kind: kind::TRANSITION,
                qualified_name: qualified_name.clone(),
            },
            kind_override: None,
            source: owner_qn.clone(),
            target: String::new(),
            guard: String::new(),
            effect: Vec::new(),
            events: Vec::new(),
            paths: HashMap::new(),
        };

        // Store the transition in the model so child elements (like guards) can modify it
        model.set_member(
            qualified_name.clone(),
            ElementVariant::Transition(transition.clone()),
        );

        stack.push(qualified_name.clone());

        for element in self.elements {
            element.apply(model, stack);
        }

        stack.pop();

        // Get the updated transition (may have been modified by nested elements like guards)
        if let Some(ElementVariant::Transition(updated_transition)) =
            model.members.get(&qualified_name)
        {
            transition = updated_transition.clone();
        }

        let prepend_source_transition = owner_qn == model.state.qualified_name()
            && transition.source != owner_qn
            && !model.get_vertex(&transition.source).is_some_and(|vertex| {
                kind::is_kind(vertex.kind(), kind::EXIT_POINT) && transition.guard.is_empty()
            });
        let source_insert_index = if prepend_source_transition {
            source_transition_insert_index(model, &transition.source, &owner_qn)
        } else {
            0
        };

        // Add transition to source vertex
        if let Some(ElementVariant::State(state)) = model.members.get_mut(&transition.source) {
            if prepend_source_transition {
                state
                    .vertex
                    .transitions
                    .insert(source_insert_index, qualified_name.clone());
            } else {
                state.vertex.transitions.push(qualified_name.clone());
            }
        } else if let Some(ElementVariant::Vertex(vertex)) =
            model.members.get_mut(&transition.source)
        {
            if prepend_source_transition {
                vertex
                    .transitions
                    .insert(source_insert_index, qualified_name.clone());
            } else {
                vertex.transitions.push(qualified_name.clone());
            }
        }

        model.set_member(qualified_name, ElementVariant::Transition(transition));
    }
}

fn source_transition_insert_index<T: Instance>(
    model: &Model<T>,
    source: &str,
    root_owner: &str,
) -> usize {
    let transitions = match model.members.get(source) {
        Some(ElementVariant::State(state)) => &state.vertex.transitions,
        Some(ElementVariant::Vertex(vertex)) => &vertex.transitions,
        _ => return 0,
    };

    transitions
        .iter()
        .take_while(|transition| dirname(transition) == root_owner)
        .count()
}

// Additional partial element types
pub struct PartialSource {
    pub source: String,
}

impl<T: Instance> PartialElement<T> for PartialSource {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let resolved_source = if self.source.starts_with('/') {
            self.source
        } else {
            let context = transition_endpoint_context(model, stack);
            resolve_relative_path(&context, &self.source)
        };

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.source = resolved_source;
        }
    }
}

pub struct PartialTarget {
    pub target: String,
}

impl<T: Instance> PartialElement<T> for PartialTarget {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let resolved_target = if self.target.starts_with('/') {
            self.target
        } else {
            let context = transition_endpoint_context(model, stack);
            resolve_relative_path(&context, &self.target)
        };

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.target = resolved_target;
        }
    }
}

pub struct PartialTrigger {
    pub events: Vec<String>,
}

impl<T: Instance> PartialElement<T> for PartialTrigger {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.extend(self.events);
        }
    }
}

pub struct PartialOnSet {
    pub name: String,
}

pub struct PartialOnCall {
    pub name: String,
}

impl<T: Instance> PartialElement<T> for PartialOnCall {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let operation_name = resolve_model_reference(model, stack, &self.name);
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(call_trigger_name(&operation_name));
        }
    }
}

impl<T: Instance> PartialElement<T> for PartialOnSet {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let event_name = if self.name.starts_with('/') {
            self.name.clone()
        } else {
            join(model.state.qualified_name(), &self.name)
        };
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(event_name.clone());
        }
        if !model.attributes.contains_key(&event_name) {
            let attribute = Attribute {
                element: NamedElement {
                    kind: kind::ATTRIBUTE,
                    qualified_name: event_name.clone(),
                },
                declared_name: self.name,
                value_type: None,
                default_value: None,
            };
            model
                .attributes
                .insert(event_name.clone(), attribute.clone());
            model.set_member(event_name, ElementVariant::Attribute(attribute));
        }
    }
}

pub struct PartialAttribute {
    pub name: String,
    pub default_value: Option<AttributeValue>,
}

pub struct PartialOperation<T: Instance> {
    pub name: String,
    pub action: OperationFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialOperation<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack
            .last()
            .cloned()
            .unwrap_or_else(|| model.state.qualified_name().to_string());
        let qualified_name = join(&owner_qn, &self.name);
        let operation = Operation {
            element: NamedElement {
                kind: kind::OPERATION,
                qualified_name: qualified_name.clone(),
            },
            action: Some(self.action),
            guard: None,
        };
        model.set_member(qualified_name, ElementVariant::Operation(operation));
    }
}

pub struct PartialGuardOperation<T: Instance> {
    pub name: String,
    pub guard: GuardFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialGuardOperation<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack
            .last()
            .cloned()
            .unwrap_or_else(|| model.state.qualified_name().to_string());
        let qualified_name = join(&owner_qn, &self.name);
        let operation = Operation {
            element: NamedElement {
                kind: kind::OPERATION,
                qualified_name: qualified_name.clone(),
            },
            action: None,
            guard: Some(self.guard),
        };
        model.set_member(qualified_name, ElementVariant::Operation(operation));
    }
}

impl<T: Instance> PartialElement<T> for PartialAttribute {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack
            .last()
            .cloned()
            .unwrap_or_else(|| model.state.qualified_name().to_string());
        let qualified_name = join(&owner_qn, &self.name);
        let value_type = self.default_value.as_ref().map(AttributeValue::value_type);
        let attribute = Attribute {
            element: NamedElement {
                kind: kind::ATTRIBUTE,
                qualified_name: qualified_name.clone(),
            },
            declared_name: self.name,
            value_type,
            default_value: self.default_value,
        };
        model
            .attributes
            .insert(qualified_name.clone(), attribute.clone());
        model.set_member(qualified_name, ElementVariant::Attribute(attribute));
    }
}

pub struct PartialEffect<T: Instance> {
    pub operations: Vec<EffectFn<T>>,
}

impl<T: Instance> PartialElement<T> for PartialEffect<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();

        // Create a behavior for each effect operation
        for (index, operation) in self.operations.into_iter().enumerate() {
            let effect_name = if index == 0 {
                join(&transition_qn, "effect")
            } else {
                join(&transition_qn, &format!("effect_{}", index))
            };

            let behavior = Behavior {
                element: NamedElement {
                    kind: kind::BEHAVIOR,
                    qualified_name: effect_name.clone(),
                },
                entry: None,
                effect: Some(operation),
                exit: None,
                activity: None,
                operation: None,
            };

            model.set_member(effect_name.clone(), ElementVariant::Behavior(behavior));

            if let Some(ElementVariant::Transition(transition)) =
                model.members.get_mut(&transition_qn)
            {
                transition.effect.push(effect_name);
            }
        }
    }
}

pub struct PartialEntry<T: Instance> {
    pub operations: Vec<EntryFn<T>>,
}

impl<T: Instance> PartialElement<T> for PartialEntry<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let state_qn = stack.last().unwrap().clone();

        // Create a behavior for each entry operation
        for (index, operation) in self.operations.into_iter().enumerate() {
            let entry_name = if index == 0 {
                join(&state_qn, "entry")
            } else {
                join(&state_qn, &format!("entry_{}", index))
            };

            let behavior = Behavior {
                element: NamedElement {
                    kind: kind::BEHAVIOR,
                    qualified_name: entry_name.clone(),
                },
                entry: Some(operation),
                effect: None,
                exit: None,
                activity: None,
                operation: None,
            };

            model.set_member(entry_name.clone(), ElementVariant::Behavior(behavior));

            if let Some(ElementVariant::State(state)) = model.members.get_mut(&state_qn) {
                state.entry.push(entry_name);
            }
        }
    }
}

pub struct PartialExit<T: Instance> {
    pub operations: Vec<ExitFn<T>>,
}

impl<T: Instance> PartialElement<T> for PartialExit<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let state_qn = stack.last().unwrap().clone();

        // Create a behavior for each exit operation
        for (index, operation) in self.operations.into_iter().enumerate() {
            let exit_name = if index == 0 {
                join(&state_qn, "exit")
            } else {
                join(&state_qn, &format!("exit_{}", index))
            };

            let behavior = Behavior {
                element: NamedElement {
                    kind: kind::BEHAVIOR,
                    qualified_name: exit_name.clone(),
                },
                entry: None,
                effect: None,
                exit: Some(operation),
                activity: None,
                operation: None,
            };

            model.set_member(exit_name.clone(), ElementVariant::Behavior(behavior));

            if let Some(ElementVariant::State(state)) = model.members.get_mut(&state_qn) {
                state.exit.push(exit_name);
            }
        }
    }
}

pub struct PartialActivity<T: Instance> {
    pub operations: Vec<ActivityFn<T>>,
}

impl<T: Instance> PartialElement<T> for PartialActivity<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let state_qn = stack.last().unwrap().clone();

        // Create a behavior for each activity operation
        for (index, operation) in self.operations.into_iter().enumerate() {
            let activity_name = if index == 0 {
                join(&state_qn, "activity")
            } else {
                join(&state_qn, &format!("activity_{}", index))
            };

            let behavior = Behavior {
                element: NamedElement {
                    kind: kind::CONCURRENT,
                    qualified_name: activity_name.clone(),
                },
                entry: None,
                effect: None,
                exit: None,
                activity: Some(operation),
                operation: None,
            };

            model.set_member(activity_name.clone(), ElementVariant::Behavior(behavior));

            if let Some(ElementVariant::State(state)) = model.members.get_mut(&state_qn) {
                state.activities.push(activity_name);
            }
        }
    }
}

pub struct PartialGuard<T: Instance> {
    pub expression: GuardFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialGuard<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let guard_name = join(&transition_qn, "guard");

        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: guard_name.clone(),
            },
            guard: Some(self.expression),
            operation: None,
            duration: None,
            timepoint: None,
        };

        model.set_member(guard_name.clone(), ElementVariant::Constraint(constraint));

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.guard = guard_name;
        }
    }
}

pub struct PartialBehaviorOperation {
    pub name: String,
    pub role: BehaviorRole,
}

#[derive(Clone, Copy)]
pub enum BehaviorRole {
    Entry,
    Exit,
    Activity,
    Effect,
}

impl<T: Instance> PartialElement<T> for PartialBehaviorOperation {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap().clone();
        let operation_name = resolve_model_reference(model, stack, &self.name);
        let behavior_name = join(
            &owner_qn,
            &format!("{}_{}", self.role.name(), basename(&self.name)),
        );

        let behavior = Behavior {
            element: NamedElement {
                kind: if matches!(self.role, BehaviorRole::Activity) {
                    kind::CONCURRENT
                } else {
                    kind::BEHAVIOR
                },
                qualified_name: behavior_name.clone(),
            },
            entry: None,
            effect: None,
            exit: None,
            activity: None,
            operation: Some(BehaviorOperation::Operation(operation_name)),
        };

        model.set_member(behavior_name.clone(), ElementVariant::Behavior(behavior));

        match self.role {
            BehaviorRole::Entry => {
                if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner_qn) {
                    state.entry.push(behavior_name);
                }
            }
            BehaviorRole::Exit => {
                if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner_qn) {
                    state.exit.push(behavior_name);
                }
            }
            BehaviorRole::Activity => {
                if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner_qn) {
                    state.activities.push(behavior_name);
                }
            }
            BehaviorRole::Effect => {
                if let Some(ElementVariant::Transition(transition)) =
                    model.members.get_mut(&owner_qn)
                {
                    transition.effect.push(behavior_name);
                }
            }
        }
    }
}

impl BehaviorRole {
    fn name(self) -> &'static str {
        match self {
            BehaviorRole::Entry => "entry",
            BehaviorRole::Exit => "exit",
            BehaviorRole::Activity => "activity",
            BehaviorRole::Effect => "effect",
        }
    }
}

pub struct PartialObserve<T: Instance> {
    pub observer: OperationFn<T>,
    pub targets: Vec<String>,
}

impl<T: Instance> PartialElement<T> for PartialObserve<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, _stack: &mut Vec<String>) {
        let qualified_name = unique_model_hook_name(model, "observation");
        let observation = Observation {
            element: NamedElement {
                kind: kind::OBSERVATION,
                qualified_name: qualified_name.clone(),
            },
            observer: self.observer,
            targets: self.targets,
        };

        model.set_member(qualified_name, ElementVariant::Observation(observation));
    }
}

pub struct PartialValidator<T: Instance> {
    pub validator: Arc<dyn ModelValidator<T>>,
}

impl<T: Instance> PartialElement<T> for PartialValidator<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, _stack: &mut Vec<String>) {
        let qualified_name = unique_model_hook_name(model, "validator");
        let validator = ValidatorElement {
            element: NamedElement {
                kind: kind::ELEMENT,
                qualified_name: qualified_name.clone(),
            },
            validator: self.validator,
        };

        model.set_member(qualified_name, ElementVariant::Validator(validator));
    }
}

pub struct PartialFinalizer<T: Instance> {
    pub finalizer: Arc<dyn ModelFinalizer<T>>,
}

impl<T: Instance> PartialElement<T> for PartialFinalizer<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, _stack: &mut Vec<String>) {
        let qualified_name = unique_model_hook_name(model, "finalizer");
        let finalizer = FinalizerElement {
            element: NamedElement {
                kind: kind::ELEMENT,
                qualified_name: qualified_name.clone(),
            },
            finalizer: self.finalizer,
        };

        model.set_member(qualified_name, ElementVariant::Finalizer(finalizer));
    }
}

pub struct PartialGuardOperationRef {
    pub name: String,
}

impl<T: Instance> PartialElement<T> for PartialGuardOperationRef {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let guard_name = join(&transition_qn, "guard");
        let operation_name = resolve_model_reference(model, stack, &self.name);

        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: guard_name.clone(),
            },
            guard: None,
            operation: Some(operation_name),
            duration: None,
            timepoint: None,
        };

        model.set_member(guard_name.clone(), ElementVariant::Constraint(constraint));

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.guard = guard_name;
        }
    }
}

pub struct PartialInitial<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialInitial<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let state_qn = stack.last().unwrap().clone();
        let initial_name = join(&state_qn, &self.name);

        let initial_vertex = Vertex {
            element: NamedElement {
                kind: kind::INITIAL,
                qualified_name: initial_name.clone(),
            },
            transitions: Vec::new(),
        };

        model.set_member(initial_name.clone(), ElementVariant::Vertex(initial_vertex));

        if let Some(ElementVariant::State(state)) = model.members.get_mut(&state_qn) {
            state.initial = initial_name.clone();
        } else if state_qn == model.state.qualified_name() {
            // Handle the root state machine case
            model.state.initial = initial_name.clone();
        }

        // Create initial transition
        stack.push(initial_name.clone());

        let transition_name = join(&initial_name, "transition");
        let transition = Transition {
            element: NamedElement {
                kind: kind::TRANSITION,
                qualified_name: transition_name.clone(),
            },
            kind_override: None,
            source: initial_name.clone(),
            target: String::new(),
            guard: String::new(),
            effect: Vec::new(),
            events: vec!["hsm/initial".to_string()],
            paths: HashMap::new(),
        };

        // Store the transition first so child elements can modify it
        model.set_member(
            transition_name.clone(),
            ElementVariant::Transition(transition),
        );
        stack.push(transition_name.clone());

        for element in self.elements {
            element.apply(model, stack);
        }

        stack.pop(); // transition
        stack.pop(); // initial

        // Add the transition to the initial vertex
        if let Some(ElementVariant::Vertex(vertex)) = model.members.get_mut(&initial_name) {
            vertex.transitions.push(transition_name.clone());
        }
    }
}

pub struct PartialChoice<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialChoice<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let qualified_name = join(&owner_qn, &self.name);

        let choice_vertex = Vertex {
            element: NamedElement {
                kind: kind::CHOICE,
                qualified_name: qualified_name.clone(),
            },
            transitions: Vec::new(),
        };

        model.set_member(
            qualified_name.clone(),
            ElementVariant::Vertex(choice_vertex),
        );
        stack.push(qualified_name);

        for element in self.elements {
            element.apply(model, stack);
        }

        stack.pop();
    }
}

pub struct PartialEntryPoint<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialEntryPoint<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&owner_qn) {
            let target = if transition.target.is_empty() {
                transition.source.clone()
            } else {
                transition.target.clone()
            };
            transition.target = join(&target, &self.name);
            return;
        }

        add_connection_point_vertex(model, stack, self.name, kind::ENTRY_POINT, self.elements);
    }
}

pub struct PartialExitPoint<T: Instance> {
    pub name: String,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialExitPoint<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::Transition(transition)) = model.members.get(&owner_qn) {
            let boundary = transition.source.clone();
            let source = resolve_exit_point_source(model, &boundary, &self.name)
                .unwrap_or_else(|| join(&boundary, &self.name));
            if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&owner_qn) {
                transition.source = source;
            }
            return;
        }

        add_connection_point_vertex(model, stack, self.name, kind::EXIT_POINT, self.elements);
    }
}

fn resolve_exit_point_source<T: Instance>(
    model: &Model<T>,
    boundary: &str,
    exit_point_name: &str,
) -> Option<String> {
    let mut direct = Vec::new();
    let mut nested = Vec::new();

    for member in model.members.values() {
        let ElementVariant::Vertex(vertex) = member else {
            continue;
        };
        if !kind::is_kind(vertex.kind(), kind::EXIT_POINT) {
            continue;
        }
        if basename(vertex.qualified_name()) != exit_point_name {
            continue;
        }
        if !crate::path::is_ancestor_or_equal(boundary, vertex.qualified_name()) {
            continue;
        }

        let owner = dirname(vertex.qualified_name());
        if owner == boundary || dirname(owner) == boundary {
            direct.push(vertex.qualified_name().to_string());
        } else {
            nested.push(vertex.qualified_name().to_string());
        }
    }

    direct.sort();
    nested.sort();
    direct
        .into_iter()
        .next()
        .or_else(|| nested.into_iter().next())
}

fn add_connection_point_vertex<T: Instance>(
    model: &mut Model<T>,
    stack: &mut Vec<String>,
    name: String,
    point_kind: kind::KindValue,
    elements: Vec<Box<dyn PartialElement<T>>>,
) {
    let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
    let qualified_name = join(&owner_qn, &name);

    let vertex = Vertex {
        element: NamedElement {
            kind: point_kind,
            qualified_name: qualified_name.clone(),
        },
        transitions: Vec::new(),
    };

    model.set_member(qualified_name.clone(), ElementVariant::Vertex(vertex));

    if elements.is_empty() {
        return;
    }

    stack.push(qualified_name.clone());

    let transition_name = join(&qualified_name, "transition");
    let transition = Transition {
        element: NamedElement {
            kind: kind::TRANSITION,
            qualified_name: transition_name.clone(),
        },
        kind_override: None,
        source: qualified_name.clone(),
        target: String::new(),
        guard: String::new(),
        effect: Vec::new(),
        events: Vec::new(),
        paths: HashMap::new(),
    };

    model.set_member(
        transition_name.clone(),
        ElementVariant::Transition(transition),
    );
    stack.push(transition_name.clone());

    for element in elements {
        element.apply(model, stack);
    }

    stack.pop();
    stack.pop();

    if let Some(ElementVariant::Vertex(vertex)) = model.members.get_mut(&qualified_name) {
        vertex.transitions.push(transition_name);
    }
}

pub struct PartialHistory<T: Instance> {
    pub name: String,
    pub kind: kind::KindValue,
    pub elements: Vec<Box<dyn PartialElement<T>>>,
}

impl<T: Instance> PartialElement<T> for PartialHistory<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let history_name = join(&owner_qn, &self.name);

        let history_vertex = Vertex {
            element: NamedElement {
                kind: self.kind,
                qualified_name: history_name.clone(),
            },
            transitions: Vec::new(),
        };

        model.set_member(history_name.clone(), ElementVariant::Vertex(history_vertex));

        if !self.elements.is_empty() {
            stack.push(history_name.clone());

            let transition_name = join(&history_name, "transition");
            let transition = Transition {
                element: NamedElement {
                    kind: kind::TRANSITION,
                    qualified_name: transition_name.clone(),
                },
                kind_override: None,
                source: history_name.clone(),
                target: String::new(),
                guard: String::new(),
                effect: Vec::new(),
                events: Vec::new(),
                paths: HashMap::new(),
            };

            model.set_member(
                transition_name.clone(),
                ElementVariant::Transition(transition),
            );
            stack.push(transition_name.clone());

            for element in self.elements {
                element.apply(model, stack);
            }

            stack.pop();
            stack.pop();

            if let Some(ElementVariant::Vertex(vertex)) = model.members.get_mut(&history_name) {
                vertex.transitions.push(transition_name);
            }
        }
    }
}

pub struct PartialFinalState {
    pub name: String,
}

impl<T: Instance> PartialElement<T> for PartialFinalState {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let parent_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let qualified_name = join(&parent_qn, &self.name);

        let final_state = State {
            vertex: Vertex {
                element: NamedElement {
                    kind: kind::FINAL_STATE,
                    qualified_name: qualified_name.clone(),
                },
                transitions: Vec::new(),
            },
            initial: String::new(),
            entry: Vec::new(),
            exit: Vec::new(),
            activities: Vec::new(),
            deferred: Vec::new(),
        };

        model.set_member(qualified_name, ElementVariant::State(final_state));
    }
}

pub struct PartialDefer {
    pub events: Vec<String>,
}

impl<T: Instance> PartialElement<T> for PartialDefer {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let state_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::State(state)) = model.members.get_mut(&state_qn) {
            state.deferred.extend(self.events);
            return;
        }

        panic!("Defer must be declared inside a state");
    }
}

pub struct PartialAfter<T: Instance> {
    pub duration_fn: DurationFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialAfter<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let constraint_name = join(&transition_qn, "after");

        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: constraint_name.clone(),
            },
            guard: None,
            operation: None,
            duration: Some(self.duration_fn),
            timepoint: None,
        };

        model.set_member(
            constraint_name.clone(),
            ElementVariant::Constraint(constraint),
        );

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(join(&transition_qn, "duration"));
            transition.events.push("hsm_timer_after".to_string());
            if transition.guard.is_empty() {
                transition.guard = constraint_name;
            }
        }
    }
}

pub struct PartialAt<T: Instance> {
    pub timepoint_fn: TimepointFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialAt<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let constraint_name = join(&transition_qn, "at");

        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: constraint_name.clone(),
            },
            guard: None,
            operation: None,
            duration: None,
            timepoint: Some(self.timepoint_fn),
        };

        model.set_member(
            constraint_name.clone(),
            ElementVariant::Constraint(constraint),
        );

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(join(&transition_qn, "timepoint"));
            transition.events.push("hsm_timer_at".to_string());
            if transition.guard.is_empty() {
                transition.guard = constraint_name;
            }
        }
    }
}

pub struct PartialEvery<T: Instance> {
    pub duration_fn: DurationFn<T>,
}

impl<T: Instance> PartialElement<T> for PartialEvery<T> {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let constraint_name = join(&transition_qn, "every");

        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: constraint_name.clone(),
            },
            guard: None,
            operation: None,
            duration: Some(self.duration_fn),
            timepoint: None,
        };

        model.set_member(
            constraint_name.clone(),
            ElementVariant::Constraint(constraint),
        );

        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(join(&transition_qn, "duration"));
            transition.events.push("hsm_timer_every".to_string());
            if transition.guard.is_empty() {
                transition.guard = constraint_name;
            }
        }
    }
}

// Builder functions
pub fn state<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialState {
        name: name.to_string(),
        elements: Vec::new(),
    })
}

pub fn transition<T: Instance + 'static>() -> Box<dyn PartialElement<T>> {
    Box::new(PartialTransition {
        name: String::new(),
        elements: Vec::new(),
    })
}

pub fn source<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialSource {
        source: name.to_string(),
    })
}

pub fn target<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTarget {
        target: name.to_string(),
    })
}

pub fn on<T: Instance + 'static, E: IntoEventName>(event: E) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTrigger {
        events: vec![event.into_event_name()],
    })
}

pub fn on_call<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialOnCall {
        name: name.to_string(),
    })
}

pub fn effect<T: Instance + 'static>(operation: EffectFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialEffect {
        operations: vec![operation],
    })
}

pub fn operation<T: Instance + 'static>(
    name: &str,
    action: OperationFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialOperation {
        name: name.to_string(),
        action,
    })
}

pub fn guard_operation<T: Instance + 'static>(
    name: &str,
    guard: GuardFn<T>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialGuardOperation {
        name: name.to_string(),
        guard,
    })
}

pub fn entry_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialBehaviorOperation {
        name: name.to_string(),
        role: BehaviorRole::Entry,
    })
}

pub fn exit_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialBehaviorOperation {
        name: name.to_string(),
        role: BehaviorRole::Exit,
    })
}

pub fn activity_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialBehaviorOperation {
        name: name.to_string(),
        role: BehaviorRole::Activity,
    })
}

pub fn effect_operation<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialBehaviorOperation {
        name: name.to_string(),
        role: BehaviorRole::Effect,
    })
}

pub fn observe<T: Instance + 'static, E: IntoObservationTarget>(
    observer: OperationFn<T>,
    targets: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialObserve {
        observer,
        targets: targets
            .into_iter()
            .map(IntoObservationTarget::into_observation_target)
            .collect(),
    })
}

pub fn validator<T, V>(validator: V) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    V: ModelValidator<T> + 'static,
{
    Box::new(PartialValidator {
        validator: Arc::new(validator),
    })
}

pub fn finalizer<T, F>(finalizer: F) -> Box<dyn PartialElement<T>>
where
    T: Instance + 'static,
    F: ModelFinalizer<T> + 'static,
{
    Box::new(PartialFinalizer {
        finalizer: Arc::new(finalizer),
    })
}

pub fn entry<T: Instance + 'static>(operation: EntryFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialEntry {
        operations: vec![operation],
    })
}

pub fn exit<T: Instance + 'static>(operation: ExitFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialExit {
        operations: vec![operation],
    })
}

pub fn activity<T: Instance + 'static>(operation: ActivityFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialActivity {
        operations: vec![operation],
    })
}

pub fn guard<T: Instance + 'static>(expression: GuardFn<T>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialGuard { expression })
}

pub fn guard_operation_ref<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialGuardOperationRef {
        name: name.to_string(),
    })
}

pub fn initial<T: Instance + 'static>() -> Box<dyn PartialElement<T>> {
    Box::new(PartialInitial {
        name: ".initial".to_string(),
        elements: Vec::new(),
    })
}

pub fn choice<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialChoice {
        name: name.to_string(),
        elements: Vec::new(),
    })
}

pub fn shallow_history<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialHistory {
        name: name.to_string(),
        kind: kind::SHALLOW_HISTORY,
        elements: Vec::new(),
    })
}

pub fn deep_history<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialHistory {
        name: name.to_string(),
        kind: kind::DEEP_HISTORY,
        elements: Vec::new(),
    })
}

pub fn final_state<T: Instance + 'static>(name: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialFinalState {
        name: name.to_string(),
    })
}

pub fn defer<T: Instance + 'static, E: IntoEventName>(
    events: Vec<E>,
) -> Box<dyn PartialElement<T>> {
    Box::new(PartialDefer {
        events: events
            .into_iter()
            .map(IntoEventName::into_event_name)
            .collect(),
    })
}
