// Builder System

use std::collections::HashMap;
use std::time::Duration;

use crate::context::Context;
use crate::element::{
    ActivityFn, Attribute, AttributeValue, Behavior, Constraint, DurationFn, EffectFn, Element,
    ElementVariant, EntryFn, ExitFn, GuardFn, Instance, NamedElement, Operation, OperationFn,
    State, TimepointFn, Transition, Vertex,
};
use crate::event::call_event_name;
use crate::kind::{self, is_kind};
use crate::model::Model;
use crate::path::{basename, dirname, is_ancestor_or_equal, join};

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

pub trait PartialElement<T: Instance>: Send + Sync {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>);
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
            format!("transition_{}", model.members.len())
        } else {
            self.name
        };
        let qualified_name = join(&owner_qn, &transition_name);

        let mut transition = Transition {
            element: NamedElement {
                kind: kind::TRANSITION,
                qualified_name: qualified_name.clone(),
            },
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

        // Add transition to source vertex
        if let Some(ElementVariant::State(state)) = model.members.get_mut(&transition.source) {
            state.vertex.transitions.push(qualified_name.clone());
        } else if let Some(ElementVariant::Vertex(vertex)) =
            model.members.get_mut(&transition.source)
        {
            vertex.transitions.push(qualified_name.clone());
        }

        // Determine transition kind and compute paths
        if transition.target == transition.source {
            transition.element.kind = kind::SELF;
        } else if transition.target.is_empty() {
            transition.element.kind = kind::INTERNAL;
        } else if is_ancestor_or_equal(&transition.source, &transition.target) {
            transition.element.kind = kind::LOCAL;
        } else {
            transition.element.kind = kind::EXTERNAL;
        }

        // Don't compute paths here - let the model's calculate_transition_paths handle it
        // This ensures consistent path calculation, especially for self-transitions

        model.set_member(qualified_name, ElementVariant::Transition(transition));
    }
}

// Additional partial element types
pub struct PartialSource {
    pub source: String,
}

impl<T: Instance> PartialElement<T> for PartialSource {
    fn apply(self: Box<Self>, model: &mut Model<T>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.source = if self.source.starts_with('/') {
                self.source
            } else {
                let context = stack
                    .get(stack.len() - 2)
                    .unwrap_or(&"/".to_string())
                    .clone();
                join(&context, &self.source)
            };
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
            // Determine the correct context based on the transition type.
            let context = if stack.len() >= 3 && stack[stack.len() - 2].contains(".initial") {
                // For initial transitions, resolve relative to the parent of the initial vertex.
                stack
                    .get(stack.len() - 3)
                    .unwrap_or(&"/".to_string())
                    .clone()
            } else if stack.len() >= 3
                && model
                    .get_vertex(&stack[stack.len() - 2])
                    .map(|vertex| {
                        is_kind(vertex.kind(), kind::SHALLOW_HISTORY)
                            || is_kind(vertex.kind(), kind::DEEP_HISTORY)
                    })
                    .unwrap_or(false)
            {
                // History default transitions are also relative to the containing state.
                stack
                    .get(stack.len() - 3)
                    .unwrap_or(&"/".to_string())
                    .clone()
            } else {
                // For regular transitions, handle different path types.
                let state_path = stack
                    .get(stack.len() - 2)
                    .unwrap_or(&"/".to_string())
                    .clone();
                if self.target == "."
                    || self.target == ".."
                    || self.target.contains("../")
                    || self.target.contains("./")
                {
                    state_path
                } else {
                    let parent_path = dirname(&state_path);

                    if stack.len() >= 2 {
                        let transition_owner = stack
                            .get(stack.len() - 2)
                            .unwrap_or(&"/".to_string())
                            .clone();

                        if transition_owner == parent_path {
                            transition_owner
                        } else {
                            parent_path.to_string()
                        }
                    } else {
                        parent_path.to_string()
                    }
                }
            };
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
            transition.events.push(call_event_name(&operation_name));
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
            operation: Some(operation_name),
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
            source: initial_name.clone(),
            target: String::new(),
            guard: String::new(),
            effect: Vec::new(),
            events: vec!["hsm_initial".to_string()],
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
        }
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
            transition.events.push("hsm_timer_after".to_string());
            transition.guard = constraint_name;
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
            transition.events.push("hsm_timer_at".to_string());
            transition.guard = constraint_name;
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
            transition.events.push("hsm_timer_every".to_string());
            transition.guard = constraint_name;
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

pub fn on<T: Instance + 'static>(event: &str) -> Box<dyn PartialElement<T>> {
    Box::new(PartialTrigger {
        events: vec![event.to_string()],
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

pub fn defer<T: Instance + 'static>(events: Vec<&str>) -> Box<dyn PartialElement<T>> {
    Box::new(PartialDefer {
        events: events.into_iter().map(|s| s.to_string()).collect(),
    })
}
