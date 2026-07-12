// Model definition

use std::collections::HashMap;

use crate::element::{
    Attribute, Behavior, BehaviorOperation, Constraint, Element, ElementVariant, FinalizerElement,
    Instance, NamedElement, Observation, Operation, State, Transition, ValidatorElement, Vertex,
};
use crate::kind;
use crate::path::{dirname, is_ancestor_or_equal, join};

#[derive(Debug)]
pub struct Model<T: Instance> {
    pub state: State,
    pub members: HashMap<String, ElementVariant<T>>,
    pub transition_map: HashMap<String, HashMap<String, Vec<String>>>,
    pub deferred_map: HashMap<String, HashMap<String, bool>>,
    pub history_updates: HashMap<String, Vec<HistoryUpdate>>,
    pub attributes: HashMap<String, Attribute>,
}

impl<T: Instance> Clone for Model<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            members: self
                .members
                .iter()
                .map(|(name, element)| (name.clone(), element.clone()))
                .collect(),
            transition_map: self.transition_map.clone(),
            deferred_map: self.deferred_map.clone(),
            history_updates: self.history_updates.clone(),
            attributes: self.attributes.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryUpdate {
    pub parent: String,
    pub shallow_child: String,
    pub deep_leaf: String,
}

impl<T: Instance> Model<T> {
    pub fn new(qualified_name: String) -> Self {
        let state = State {
            vertex: Vertex {
                element: NamedElement {
                    kind: kind::STATE_MACHINE,
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

        Self {
            state,
            members: HashMap::new(),
            transition_map: HashMap::new(),
            deferred_map: HashMap::new(),
            history_updates: HashMap::new(),
            attributes: HashMap::new(),
        }
    }

    pub fn get_state(&self, qualified_name: &str) -> Option<&State> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::State(s)) => Some(s),
            _ => None,
        }
    }

    pub fn get_vertex(&self, qualified_name: &str) -> Option<&Vertex> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Vertex(v)) => Some(v),
            Some(ElementVariant::State(s)) => Some(&s.vertex),
            _ => None,
        }
    }

    pub fn get_transition(&self, qualified_name: &str) -> Option<&Transition> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Transition(t)) => Some(t),
            _ => None,
        }
    }

    pub fn get_behavior(&self, qualified_name: &str) -> Option<&Behavior<T>> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Behavior(b)) => Some(b),
            _ => None,
        }
    }

    pub fn get_observation(&self, qualified_name: &str) -> Option<&Observation<T>> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Observation(o)) => Some(o),
            _ => None,
        }
    }

    pub fn get_constraint(&self, qualified_name: &str) -> Option<&Constraint<T>> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Constraint(c)) => Some(c),
            _ => None,
        }
    }

    pub fn get_operation(&self, qualified_name: &str) -> Option<&Operation<T>> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Operation(o)) => Some(o),
            _ => None,
        }
    }

    pub fn get_attribute(&self, qualified_name: &str) -> Option<&Attribute> {
        self.attributes.get(qualified_name)
    }

    pub fn set_member(&mut self, qualified_name: String, element: ElementVariant<T>) {
        self.members.insert(qualified_name, element);
    }

    pub fn rebase(&self, qualified_name: String) -> Self {
        let old_root = self.qualified_name().to_string();
        let new_root = qualified_name;
        let mut model = self.clone();

        model.state = Self::rebase_state(model.state, &old_root, &new_root);

        let members = std::mem::take(&mut model.members);
        model.members = members
            .into_iter()
            .map(|(name, element)| {
                (
                    Self::rebase_reference(&name, &old_root, &new_root),
                    Self::rebase_element(element, &old_root, &new_root),
                )
            })
            .collect();

        let attributes = std::mem::take(&mut model.attributes);
        model.attributes = attributes
            .into_iter()
            .map(|(name, attribute)| {
                (
                    Self::rebase_reference(&name, &old_root, &new_root),
                    Self::rebase_attribute(attribute, &old_root, &new_root),
                )
            })
            .collect();

        model.clear_runtime_metadata();
        model
    }

    pub fn clear_runtime_metadata(&mut self) {
        self.transition_map.clear();
        self.deferred_map.clear();
        self.history_updates.clear();

        for element in self.members.values_mut() {
            if let ElementVariant::Transition(transition) = element {
                transition.paths.clear();
            }
        }
    }

    fn rebase_element(
        element: ElementVariant<T>,
        old_root: &str,
        new_root: &str,
    ) -> ElementVariant<T> {
        match element {
            ElementVariant::State(state) => {
                ElementVariant::State(Self::rebase_state(state, old_root, new_root))
            }
            ElementVariant::Vertex(vertex) => {
                ElementVariant::Vertex(Self::rebase_vertex(vertex, old_root, new_root))
            }
            ElementVariant::Transition(transition) => {
                ElementVariant::Transition(Self::rebase_transition(transition, old_root, new_root))
            }
            ElementVariant::Behavior(behavior) => {
                ElementVariant::Behavior(Self::rebase_behavior(behavior, old_root, new_root))
            }
            ElementVariant::Observation(observation) => ElementVariant::Observation(
                Self::rebase_observation(observation, old_root, new_root),
            ),
            ElementVariant::Validator(validator) => {
                ElementVariant::Validator(Self::rebase_validator(validator, old_root, new_root))
            }
            ElementVariant::Finalizer(finalizer) => {
                ElementVariant::Finalizer(Self::rebase_finalizer(finalizer, old_root, new_root))
            }
            ElementVariant::Constraint(constraint) => {
                ElementVariant::Constraint(Self::rebase_constraint(constraint, old_root, new_root))
            }
            ElementVariant::Operation(operation) => {
                ElementVariant::Operation(Self::rebase_operation(operation, old_root, new_root))
            }
            ElementVariant::Attribute(attribute) => {
                ElementVariant::Attribute(Self::rebase_attribute(attribute, old_root, new_root))
            }
            ElementVariant::Event(event) => {
                ElementVariant::Event(Self::rebase_event(event, old_root, new_root))
            }
        }
    }

    fn rebase_state(mut state: State, old_root: &str, new_root: &str) -> State {
        state.vertex = Self::rebase_vertex(state.vertex, old_root, new_root);
        state.initial = Self::rebase_reference(&state.initial, old_root, new_root);
        state.entry = Self::rebase_references(state.entry, old_root, new_root);
        state.exit = Self::rebase_references(state.exit, old_root, new_root);
        state.activities = Self::rebase_references(state.activities, old_root, new_root);
        state.deferred = Self::rebase_references(state.deferred, old_root, new_root);
        state
    }

    fn rebase_vertex(mut vertex: Vertex, old_root: &str, new_root: &str) -> Vertex {
        vertex.element = Self::rebase_named_element(vertex.element, old_root, new_root);
        vertex.transitions = Self::rebase_references(vertex.transitions, old_root, new_root);
        vertex
    }

    fn rebase_transition(mut transition: Transition, old_root: &str, new_root: &str) -> Transition {
        transition.element = Self::rebase_named_element(transition.element, old_root, new_root);
        transition.source = Self::rebase_reference(&transition.source, old_root, new_root);
        transition.target = Self::rebase_reference(&transition.target, old_root, new_root);
        transition.guard = Self::rebase_reference(&transition.guard, old_root, new_root);
        transition.effect = Self::rebase_references(transition.effect, old_root, new_root);
        transition.events = Self::rebase_references(transition.events, old_root, new_root);
        transition.paths.clear();
        transition
    }

    fn rebase_behavior(mut behavior: Behavior<T>, old_root: &str, new_root: &str) -> Behavior<T> {
        behavior.element = Self::rebase_named_element(behavior.element, old_root, new_root);
        behavior.operation = behavior.operation.map(|operation| match operation {
            BehaviorOperation::Operation(name) => {
                BehaviorOperation::Operation(Self::rebase_reference(&name, old_root, new_root))
            }
            BehaviorOperation::Observation {
                observer,
                source,
                occurrence,
            } => BehaviorOperation::Observation {
                observer,
                source: Self::rebase_reference(&source, old_root, new_root),
                occurrence,
            },
        });
        behavior
    }

    fn rebase_observation(
        mut observation: Observation<T>,
        old_root: &str,
        new_root: &str,
    ) -> Observation<T> {
        observation.element = Self::rebase_named_element(observation.element, old_root, new_root);
        observation.targets = Self::rebase_references(observation.targets, old_root, new_root);
        observation
    }

    fn rebase_validator(
        mut validator: ValidatorElement<T>,
        old_root: &str,
        new_root: &str,
    ) -> ValidatorElement<T> {
        validator.element = Self::rebase_named_element(validator.element, old_root, new_root);
        validator
    }

    fn rebase_finalizer(
        mut finalizer: FinalizerElement<T>,
        old_root: &str,
        new_root: &str,
    ) -> FinalizerElement<T> {
        finalizer.element = Self::rebase_named_element(finalizer.element, old_root, new_root);
        finalizer
    }

    fn rebase_constraint(
        mut constraint: Constraint<T>,
        old_root: &str,
        new_root: &str,
    ) -> Constraint<T> {
        constraint.element = Self::rebase_named_element(constraint.element, old_root, new_root);
        constraint.operation = constraint
            .operation
            .map(|operation| Self::rebase_reference(&operation, old_root, new_root));
        constraint
    }

    fn rebase_operation(
        mut operation: Operation<T>,
        old_root: &str,
        new_root: &str,
    ) -> Operation<T> {
        operation.element = Self::rebase_named_element(operation.element, old_root, new_root);
        operation
    }

    fn rebase_attribute(mut attribute: Attribute, old_root: &str, new_root: &str) -> Attribute {
        attribute.element = Self::rebase_named_element(attribute.element, old_root, new_root);
        attribute
    }

    fn rebase_event(
        mut event: crate::event::Event,
        old_root: &str,
        new_root: &str,
    ) -> crate::event::Event {
        event.qualified_name = Self::rebase_reference(&event.qualified_name, old_root, new_root);
        event.name = Self::rebase_reference(&event.name, old_root, new_root);
        event
    }

    fn rebase_named_element(
        mut element: NamedElement,
        old_root: &str,
        new_root: &str,
    ) -> NamedElement {
        element.qualified_name =
            Self::rebase_reference(&element.qualified_name, old_root, new_root);
        element
    }

    fn rebase_references(references: Vec<String>, old_root: &str, new_root: &str) -> Vec<String> {
        references
            .into_iter()
            .map(|reference| Self::rebase_reference(&reference, old_root, new_root))
            .collect()
    }

    fn rebase_reference(reference: &str, old_root: &str, new_root: &str) -> String {
        if reference.is_empty() {
            return String::new();
        }

        if let Some(operation) = reference.strip_prefix("hsm_call:") {
            return format!(
                "hsm_call:{}",
                Self::rebase_reference(operation, old_root, new_root)
            );
        }

        if reference == old_root {
            return new_root.to_string();
        }

        let prefix = format!("{}/", old_root.trim_end_matches('/'));
        if let Some(relative) = reference.strip_prefix(&prefix) {
            return join(new_root, relative);
        }

        reference.to_string()
    }

    pub fn resolve_transition_targets(&mut self) {
        let transition_names: Vec<String> = self
            .members
            .iter()
            .filter_map(|(name, element)| {
                if matches!(element, ElementVariant::Transition(_)) {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        for transition_name in transition_names {
            let (source, target) = {
                let Some(ElementVariant::Transition(transition)) =
                    self.members.get(&transition_name)
                else {
                    continue;
                };
                (transition.source.clone(), transition.target.clone())
            };

            if target.is_empty() || self.has_endpoint(&target) {
                continue;
            }

            let context = self.transition_endpoint_context(&source);
            if context.is_empty() || !is_ancestor_or_equal(&context, &target) {
                continue;
            }

            let relative = target[context.len()..].trim_start_matches('/');
            if relative.is_empty() {
                continue;
            }

            let fallback = join(self.qualified_name(), relative);
            if !self.has_endpoint(&fallback) {
                continue;
            }

            if let Some(ElementVariant::Transition(transition)) =
                self.members.get_mut(&transition_name)
            {
                transition.target = fallback;
            }
        }
    }

    pub fn finalize_transition_kinds(&mut self) {
        for element in self.members.values_mut() {
            let ElementVariant::Transition(transition) = element else {
                continue;
            };

            transition.element.kind = if let Some(kind) = transition.kind_override {
                kind
            } else if transition.target == transition.source {
                kind::SELF
            } else if transition.target.is_empty() {
                kind::INTERNAL
            } else if is_ancestor_or_equal(&transition.source, &transition.target) {
                kind::LOCAL
            } else {
                kind::EXTERNAL
            };
        }
    }

    fn has_endpoint(&self, qualified_name: &str) -> bool {
        qualified_name == self.qualified_name() || self.members.contains_key(qualified_name)
    }

    fn transition_endpoint_context(&self, source: &str) -> String {
        if source == self.qualified_name() || self.get_state(source).is_some() {
            source.to_string()
        } else {
            dirname(source).to_string()
        }
    }

    // Calculate transition paths for all transitions
    pub fn calculate_transition_paths(&mut self) {
        let transitions_to_update: Vec<(String, String, String, bool, bool)> = self
            .members
            .iter()
            .filter_map(|(name, element)| {
                if let ElementVariant::Transition(transition) = element {
                    // Check if this is an initial transition
                    let is_initial = transition.events.contains(&"hsm/initial".to_string());
                    // Check if this is an internal transition (no target)
                    let is_internal = transition.target.is_empty();
                    Some((
                        name.clone(),
                        transition.source.clone(),
                        transition.target.clone(),
                        is_initial,
                        is_internal,
                    ))
                } else {
                    None
                }
            })
            .collect();

        for (transition_name, source, target, is_initial, is_internal) in transitions_to_update {
            if is_internal {
                // Internal transitions have empty paths (no exit/enter)
                let empty_path = crate::element::TransitionPath {
                    enter: Vec::new(),
                    exit: Vec::new(),
                };
                if let Some(ElementVariant::Transition(transition)) =
                    self.members.get_mut(&transition_name)
                {
                    transition.paths.insert(source, empty_path);
                }
            } else if is_initial {
                // For initial transitions, calculate paths from the parent state machine
                let parent_source = dirname(&source).to_string();
                let path = Self::calculate_path_static(&parent_source, &target);

                if let Some(ElementVariant::Transition(transition)) =
                    self.members.get_mut(&transition_name)
                {
                    transition.paths.insert(parent_source, path);
                }
            } else {
                // Calculate the normal transition path
                let path = Self::calculate_path_static(&source, &target);

                if let Some(ElementVariant::Transition(transition)) =
                    self.members.get_mut(&transition_name)
                {
                    transition.paths.insert(source, path);
                }
            }
        }
    }

    // Calculate enter/exit path for a transition (static version)
    pub fn calculate_path_static(source: &str, target: &str) -> crate::element::TransitionPath {
        use crate::path::lca;

        let mut enter = Vec::new();
        let mut exit = Vec::new();

        // Special case for self-transitions
        if source == target {
            // For self-transitions, we exit and re-enter the same state
            exit.push(source.to_string());
            enter.push(target.to_string());
            return crate::element::TransitionPath { enter, exit };
        }

        // Find lowest common ancestor
        let common_ancestor = lca(source, target);

        // Calculate exit path (from source up to LCA, not including LCA)
        let mut current = source.to_string();
        while current != common_ancestor && !current.is_empty() {
            exit.push(current.clone());
            current = dirname(&current).to_string();
        }

        // Calculate enter path (from LCA down to target, not including LCA)
        let mut path_to_target = Vec::new();
        let mut current = target.to_string();
        while current != common_ancestor && !current.is_empty() {
            path_to_target.push(current.clone());
            current = dirname(&current).to_string();
        }
        path_to_target.reverse();
        enter = path_to_target;

        crate::element::TransitionPath { enter, exit }
    }

    // Build optimized transition table like JavaScript
    pub fn build_transition_table(&mut self) {
        // First, add the root state machine to the transition map
        let root_state_name = self.state.qualified_name().to_string();
        self.transition_map
            .insert(root_state_name.clone(), HashMap::new());
        let mut root_transitions: Vec<String> = self
            .members
            .iter()
            .filter_map(|(name, element)| {
                let ElementVariant::Transition(transition) = element else {
                    return None;
                };
                if transition.source == root_state_name && !transition.events.is_empty() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        root_transitions.sort();

        // Add transitions from the root state's initial vertex
        if !self.state.initial.is_empty() {
            if let Some(ElementVariant::Vertex(initial_vertex)) =
                self.members.get(&self.state.initial)
            {
                for transition_name in &initial_vertex.transitions {
                    if let Some(transition) = self.get_transition(transition_name).cloned() {
                        for event_name in &transition.events {
                            self.transition_map
                                .get_mut(&root_state_name)
                                .unwrap()
                                .entry(event_name.clone())
                                .or_insert_with(Vec::new)
                                .push(transition_name.clone());
                        }
                    }
                }
            }
        }

        // Then handle all other states
        for state_name in self.members.keys().cloned().collect::<Vec<_>>() {
            if self.get_state(&state_name).is_some() {
                self.transition_map
                    .insert(state_name.clone(), HashMap::new());

                let mut current_path = state_name.clone();
                while !current_path.is_empty() {
                    if let Some(current_state) = self.get_state(&current_path).cloned() {
                        // Add transitions from the state itself
                        for transition_name in &current_state.vertex.transitions {
                            if let Some(transition) = self.get_transition(transition_name).cloned()
                            {
                                for event_name in &transition.events {
                                    self.transition_map
                                        .get_mut(&state_name)
                                        .unwrap()
                                        .entry(event_name.clone())
                                        .or_insert_with(Vec::new)
                                        .push(transition_name.clone());
                                }
                            }
                        }

                        // Add transitions from the state's initial vertex
                        if !current_state.initial.is_empty() {
                            if let Some(ElementVariant::Vertex(initial_vertex)) =
                                self.members.get(&current_state.initial)
                            {
                                for transition_name in &initial_vertex.transitions {
                                    if let Some(transition) =
                                        self.get_transition(transition_name).cloned()
                                    {
                                        for event_name in &transition.events {
                                            self.transition_map
                                                .get_mut(&state_name)
                                                .unwrap()
                                                .entry(event_name.clone())
                                                .or_insert_with(Vec::new)
                                                .push(transition_name.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if current_path == root_state_name {
                        for transition_name in &root_transitions {
                            if let Some(transition) = self.get_transition(transition_name).cloned()
                            {
                                for event_name in &transition.events {
                                    self.transition_map
                                        .get_mut(&state_name)
                                        .unwrap()
                                        .entry(event_name.clone())
                                        .or_insert_with(Vec::new)
                                        .push(transition_name.clone());
                                }
                            }
                        }
                    }

                    if current_path == "/" {
                        break;
                    }
                    let parent = dirname(&current_path);
                    if parent == current_path {
                        break;
                    }
                    current_path = parent.to_string();
                }
            }
        }
    }

    // Build deferred event table
    pub fn build_deferred_table(&mut self) {
        for state_name in self.members.keys().cloned().collect::<Vec<_>>() {
            if self.get_state(&state_name).is_some() {
                self.deferred_map.insert(state_name.clone(), HashMap::new());

                let mut current_path = state_name.clone();
                while !current_path.is_empty() {
                    if let Some(current_state) = self.get_state(&current_path).cloned() {
                        for deferred_event in &current_state.deferred {
                            self.deferred_map
                                .get_mut(&state_name)
                                .unwrap()
                                .insert(deferred_event.clone(), true);
                        }
                    }

                    if current_path == "/" {
                        break;
                    }
                    let parent = dirname(&current_path);
                    if parent == current_path {
                        break;
                    }
                    current_path = parent.to_string();
                }
            }
        }
    }

    pub fn build_history_table(&mut self) {
        self.history_updates.clear();

        for leaf_name in self.members.keys().cloned().collect::<Vec<_>>() {
            if self.get_state(&leaf_name).is_none() {
                continue;
            }

            let mut updates = Vec::new();
            let mut child = leaf_name.clone();
            let mut parent = dirname(&child).to_string();

            while !parent.is_empty() && parent != "/" && parent != child {
                if self.get_state(&parent).is_some() || parent == self.qualified_name() {
                    updates.push(HistoryUpdate {
                        parent: parent.clone(),
                        shallow_child: child.clone(),
                        deep_leaf: leaf_name.clone(),
                    });
                }

                if parent == self.qualified_name() {
                    break;
                }

                child = parent;
                parent = dirname(&child).to_string();
            }

            self.history_updates.insert(leaf_name, updates);
        }
    }
}

impl<T: Instance> Element for Model<T> {
    fn kind(&self) -> crate::kind::KindValue {
        self.state.kind()
    }
    fn qualified_name(&self) -> &str {
        self.state.qualified_name()
    }
}
