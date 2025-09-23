// Model definition

use std::collections::HashMap;

use crate::element::{
    Element, ElementVariant, Instance, State, Vertex, Transition, Behavior, 
    Constraint, NamedElement
};
use crate::kind;
use crate::path::{dirname, join};

#[derive(Debug)]
pub struct Model<T: Instance> {
    pub state: State,
    pub members: HashMap<String, ElementVariant<T>>,
    pub transition_map: HashMap<String, HashMap<String, Vec<String>>>,
    pub deferred_map: HashMap<String, HashMap<String, bool>>,
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

    pub fn get_constraint(&self, qualified_name: &str) -> Option<&Constraint<T>> {
        match self.members.get(qualified_name) {
            Some(ElementVariant::Constraint(c)) => Some(c),
            _ => None,
        }
    }

    pub fn set_member(&mut self, qualified_name: String, element: ElementVariant<T>) {
        self.members.insert(qualified_name, element);
    }
    
    // Calculate transition paths for all transitions
    pub fn calculate_transition_paths(&mut self) {
        let transitions_to_update: Vec<(String, String, String, bool, bool)> = self.members.iter()
            .filter_map(|(name, element)| {
                if let ElementVariant::Transition(transition) = element {
                    // Check if this is an initial transition
                    let is_initial = transition.events.contains(&"hsm_initial".to_string());
                    // Check if this is an internal transition (no target)
                    let is_internal = transition.target.is_empty();
                    Some((name.clone(), transition.source.clone(), transition.target.clone(), is_initial, is_internal))
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
                if let Some(ElementVariant::Transition(transition)) = self.members.get_mut(&transition_name) {
                    transition.paths.insert(source, empty_path);
                }
            } else if is_initial {
                // For initial transitions, calculate paths from the parent state machine
                let parent_source = dirname(&source).to_string();
                let path = Self::calculate_path_static(&parent_source, &target);
                
                if let Some(ElementVariant::Transition(transition)) = self.members.get_mut(&transition_name) {
                    transition.paths.insert(parent_source, path);
                }
            } else {
                // Calculate the normal transition path
                let path = Self::calculate_path_static(&source, &target);
                
                if let Some(ElementVariant::Transition(transition)) = self.members.get_mut(&transition_name) {
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
        self.transition_map.insert(root_state_name.clone(), HashMap::new());
        
        // Add transitions from the root state's initial vertex
        if !self.state.initial.is_empty() {
            if let Some(ElementVariant::Vertex(initial_vertex)) = self.members.get(&self.state.initial) {
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
            if let Some(state) = self.get_state(&state_name).cloned() {
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
                            if let Some(ElementVariant::Vertex(initial_vertex)) = self.members.get(&current_state.initial) {
                                for transition_name in &initial_vertex.transitions {
                                    if let Some(transition) = self.get_transition(transition_name).cloned() {
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
            if let Some(state) = self.get_state(&state_name).cloned() {
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
}

impl<T: Instance> Element for Model<T> {
    fn kind(&self) -> crate::kind::KindValue {
        self.state.kind()
    }
    fn qualified_name(&self) -> &str {
        self.state.qualified_name()
    }
}
