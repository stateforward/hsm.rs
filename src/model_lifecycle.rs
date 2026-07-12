use crate::builder::PartialElement;
use crate::element::*;
use crate::event::final_event;
use crate::kind::{self, KindValue};
use crate::model::Model;
use crate::path;

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

    finalize_model(&mut model);

    model
}

#[allow(non_snake_case)]
pub fn Define<T: Instance + 'static>(
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    define(name, elements)
}

pub fn redefine<T: Instance + 'static>(
    model: &Model<T>,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    let name = model.name();
    redefine_as(model, &name, elements)
}

pub fn redefine_as<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    let qualified_name = path::join("/", name);
    let mut redefined = model.rebase(qualified_name.clone());
    let mut stack = vec![qualified_name];

    for element in elements {
        element.apply(&mut redefined, &mut stack);
    }

    finalize_model(&mut redefined);
    redefined
}

#[allow(non_snake_case)]
pub fn Redefine<T: Instance + 'static>(
    model: &Model<T>,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    redefine(model, elements)
}

#[allow(non_snake_case)]
pub fn RedefineAs<T: Instance + 'static>(
    model: &Model<T>,
    name: &str,
    elements: Vec<Box<dyn PartialElement<T>>>,
) -> Model<T> {
    redefine_as(model, name, elements)
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultModelFinalizer;

impl DefaultModelFinalizer {
    pub fn finalize<T: Instance + 'static>(&self, model: &mut Model<T>) {
        default_finalize_model(model)
    }
}

impl<T: Instance + 'static> ModelFinalizer<T> for DefaultModelFinalizer {
    fn finalize(&self, model: &mut Model<T>) {
        default_finalize_model(model)
    }
}

fn finalize_model<T: Instance + 'static>(model: &mut Model<T>) {
    // Finalize model-derived runtime metadata after all forward references exist.
    model.clear_runtime_metadata();
    remove_generated_observation_behaviors(model);
    model.resolve_transition_targets();
    register_completion_transitions(model);
    register_transition_sources(model);
    model.finalize_transition_kinds();
    run_custom_model_validator(model);
    apply_observations(model);
    let finalizer = selected_model_finalizer(model);
    finalizer.finalize(model);
}

fn default_finalize_model<T: Instance + 'static>(model: &mut Model<T>) {
    model.calculate_transition_paths();
    model.build_transition_table();
    model.build_deferred_table();
    model.build_history_table();
}

fn run_custom_model_validator<T: Instance + 'static>(model: &Model<T>) {
    if let Some(validator) = selected_model_validator(model) {
        if let Err(error) = validator.validate(model) {
            panic!("model validator failed: {}", error);
        }
    }
}

pub(crate) fn selected_model_validator<T: Instance + 'static>(
    model: &Model<T>,
) -> Option<std::sync::Arc<dyn ModelValidator<T>>> {
    model
        .members
        .iter()
        .filter_map(|(name, element)| {
            let ElementVariant::Validator(validator) = element else {
                return None;
            };
            Some((
                model_hook_index(name, "validator"),
                validator.validator.clone(),
            ))
        })
        .max_by_key(|(index, _)| *index)
        .map(|(_, validator)| validator)
}

fn selected_model_finalizer<T: Instance + 'static>(
    model: &Model<T>,
) -> std::sync::Arc<dyn ModelFinalizer<T>> {
    model
        .members
        .iter()
        .filter_map(|(name, element)| {
            let ElementVariant::Finalizer(finalizer) = element else {
                return None;
            };
            Some((
                model_hook_index(name, "finalizer"),
                finalizer.finalizer.clone(),
            ))
        })
        .max_by_key(|(index, _)| *index)
        .map(|(_, finalizer)| finalizer)
        .unwrap_or_else(|| std::sync::Arc::new(DefaultModelFinalizer))
}

fn model_hook_index(qualified_name: &str, prefix: &str) -> usize {
    let hook_name = path::basename(qualified_name);
    let prefix = format!("{}_", prefix);
    hook_name
        .strip_prefix(&prefix)
        .and_then(|index| index.parse::<usize>().ok())
        .unwrap_or(0)
}

fn register_completion_transitions<T: Instance + 'static>(model: &mut Model<T>) {
    let completion_transitions: Vec<String> = model
        .members
        .iter()
        .filter_map(|(name, element)| {
            let ElementVariant::Transition(transition) = element else {
                return None;
            };
            if !transition.events.is_empty() {
                return None;
            }
            let source = model.get_vertex(&transition.source)?;
            if kind::is_kind(source.kind(), kind::PSEUDOSTATE) {
                return None;
            }
            Some(name.clone())
        })
        .collect();

    let final_event_name = final_event().name;
    for transition_name in completion_transitions {
        if let Some(ElementVariant::Transition(transition)) =
            model.members.get_mut(&transition_name)
        {
            transition.events.push(final_event_name.clone());
        }
    }
}

fn register_transition_sources<T: Instance + 'static>(model: &mut Model<T>) {
    let mut transitions: Vec<(String, String)> = model
        .members
        .iter()
        .filter_map(|(name, element)| {
            let ElementVariant::Transition(transition) = element else {
                return None;
            };
            Some((name.clone(), transition.source.clone()))
        })
        .collect();
    transitions.sort_by(|left, right| left.0.cmp(&right.0));

    for (transition_name, source_name) in transitions {
        if let Some(ElementVariant::State(state)) = model.members.get_mut(&source_name) {
            if !state
                .vertex
                .transitions
                .iter()
                .any(|name| name == &transition_name)
            {
                state.vertex.transitions.push(transition_name);
            }
            continue;
        }

        if let Some(ElementVariant::Vertex(vertex)) = model.members.get_mut(&source_name) {
            if !vertex
                .transitions
                .iter()
                .any(|name| name == &transition_name)
            {
                vertex.transitions.push(transition_name);
            }
        }
    }
}

#[derive(Clone)]
enum BehaviorAttachment {
    StateEntry(String),
    StateExit(String),
    StateActivity(String),
    TransitionEffect(String),
}

fn remove_generated_observation_behaviors<T: Instance + 'static>(model: &mut Model<T>) {
    let generated: Vec<String> = model
        .members
        .iter()
        .filter_map(|(name, element)| {
            let ElementVariant::Behavior(behavior) = element else {
                return None;
            };
            if is_observation_behavior(behavior) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    if generated.is_empty() {
        return;
    }

    for element in model.members.values_mut() {
        match element {
            ElementVariant::State(state) => {
                state.entry.retain(|name| !generated.contains(name));
                state.exit.retain(|name| !generated.contains(name));
                state.activities.retain(|name| !generated.contains(name));
            }
            ElementVariant::Transition(transition) => {
                transition.effect.retain(|name| !generated.contains(name));
            }
            _ => {}
        }
    }

    for name in generated {
        model.members.remove(&name);
    }
}

fn apply_observations<T: Instance + 'static>(model: &mut Model<T>) {
    let mut observations: Vec<Observation<T>> = model
        .members
        .values()
        .filter_map(|element| match element {
            ElementVariant::Observation(observation) => Some(observation.clone()),
            _ => None,
        })
        .collect();
    observations.sort_by(|left, right| left.qualified_name().cmp(right.qualified_name()));

    for observation in observations {
        let mut member_names: Vec<String> = model.members.keys().cloned().collect();
        member_names.sort();

        for member_name in member_names {
            let Some(element) = model.members.get(&member_name).cloned() else {
                continue;
            };

            match element {
                ElementVariant::Behavior(behavior) => {
                    if is_observation_behavior(&behavior)
                        || !observation_matches_behavior(&observation, &member_name)
                    {
                        continue;
                    }
                    insert_behavior_observation(model, &observation, &member_name);
                }
                ElementVariant::Transition(transition) => {
                    if observation_matches_transition(&observation, &member_name, &transition) {
                        insert_transition_observation(model, &observation, &member_name);
                    }
                }
                _ => {}
            }
        }
    }
}

fn observation_matches_behavior<T: Instance + 'static>(
    observation: &Observation<T>,
    behavior_name: &str,
) -> bool {
    observation.targets.is_empty()
        || observation
            .targets
            .iter()
            .any(|target| target == behavior_name)
}

fn observation_matches_transition<T: Instance + 'static>(
    observation: &Observation<T>,
    transition_name: &str,
    transition: &Transition,
) -> bool {
    observation.targets.is_empty()
        || observation
            .targets
            .iter()
            .any(|target| target == transition_name || transition.events.contains(target))
}

fn insert_behavior_observation<T: Instance + 'static>(
    model: &mut Model<T>,
    observation: &Observation<T>,
    behavior_name: &str,
) {
    let Some(attachment) = behavior_attachment(model, behavior_name) else {
        return;
    };
    let owner = attachment.owner();
    let kind = if matches!(attachment, BehaviorAttachment::StateActivity(_)) {
        kind::CONCURRENT
    } else {
        kind::BEHAVIOR
    };
    let generated_name =
        unique_observation_behavior_name(model, &owner, observation, "behavior", behavior_name);
    let behavior = observation_behavior(
        observation,
        &generated_name,
        kind,
        behavior_name,
        "behavior",
    );

    model.set_member(generated_name.clone(), ElementVariant::Behavior(behavior));
    attach_observation_behavior(model, attachment, behavior_name, generated_name);
}

fn insert_transition_observation<T: Instance + 'static>(
    model: &mut Model<T>,
    observation: &Observation<T>,
    transition_name: &str,
) {
    let Some(transition) = model.get_transition(transition_name) else {
        return;
    };
    let insert_at = transition
        .effect
        .iter()
        .take_while(|behavior_name| is_generated_observation_behavior(model, behavior_name))
        .count();
    let generated_name = unique_observation_behavior_name(
        model,
        transition_name,
        observation,
        "event",
        transition_name,
    );
    let behavior = observation_behavior(
        observation,
        &generated_name,
        kind::BEHAVIOR,
        transition_name,
        "event",
    );

    model.set_member(generated_name.clone(), ElementVariant::Behavior(behavior));
    if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(transition_name) {
        transition.effect.insert(insert_at, generated_name);
    }
}

fn behavior_attachment<T: Instance + 'static>(
    model: &Model<T>,
    behavior_name: &str,
) -> Option<BehaviorAttachment> {
    let owner = path::dirname(behavior_name).to_string();

    if let Some(state) = model.get_state(&owner) {
        if state.entry.iter().any(|name| name == behavior_name) {
            return Some(BehaviorAttachment::StateEntry(owner));
        }
        if state.exit.iter().any(|name| name == behavior_name) {
            return Some(BehaviorAttachment::StateExit(owner));
        }
        if state.activities.iter().any(|name| name == behavior_name) {
            return Some(BehaviorAttachment::StateActivity(owner));
        }
    }

    if let Some(transition) = model.get_transition(&owner) {
        if transition.effect.iter().any(|name| name == behavior_name) {
            return Some(BehaviorAttachment::TransitionEffect(owner));
        }
    }

    None
}

impl BehaviorAttachment {
    fn owner(&self) -> String {
        match self {
            BehaviorAttachment::StateEntry(owner)
            | BehaviorAttachment::StateExit(owner)
            | BehaviorAttachment::StateActivity(owner)
            | BehaviorAttachment::TransitionEffect(owner) => owner.clone(),
        }
    }
}

fn attach_observation_behavior<T: Instance + 'static>(
    model: &mut Model<T>,
    attachment: BehaviorAttachment,
    before_behavior: &str,
    generated_name: String,
) {
    match attachment {
        BehaviorAttachment::StateEntry(owner) => {
            if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner) {
                insert_before(&mut state.entry, before_behavior, generated_name);
            }
        }
        BehaviorAttachment::StateExit(owner) => {
            if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner) {
                insert_before(&mut state.exit, before_behavior, generated_name);
            }
        }
        BehaviorAttachment::StateActivity(owner) => {
            if let Some(ElementVariant::State(state)) = model.members.get_mut(&owner) {
                insert_before(&mut state.activities, before_behavior, generated_name);
            }
        }
        BehaviorAttachment::TransitionEffect(owner) => {
            if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&owner) {
                insert_before(&mut transition.effect, before_behavior, generated_name);
            }
        }
    }
}

fn insert_before(list: &mut Vec<String>, before: &str, value: String) {
    if let Some(index) = list.iter().position(|name| name == before) {
        list.insert(index, value);
    }
}

fn unique_observation_behavior_name<T: Instance + 'static>(
    model: &Model<T>,
    owner: &str,
    observation: &Observation<T>,
    occurrence: &str,
    source: &str,
) -> String {
    let observation_name = path::basename(observation.qualified_name());
    let source_name = path::basename(source);
    let base = format!(
        "observe_{}_{}_{}",
        observation_name, occurrence, source_name
    );
    let mut index = 0;

    loop {
        let local_name = if index == 0 {
            base.clone()
        } else {
            format!("{}_{}", base, index)
        };
        let qualified_name = path::join(owner, &local_name);
        if !model.members.contains_key(&qualified_name) {
            return qualified_name;
        }
        index += 1;
    }
}

fn observation_behavior<T: Instance + 'static>(
    observation: &Observation<T>,
    qualified_name: &str,
    behavior_kind: KindValue,
    source: &str,
    occurrence: &str,
) -> Behavior<T> {
    Behavior {
        element: NamedElement {
            kind: behavior_kind,
            qualified_name: qualified_name.to_string(),
        },
        entry: None,
        effect: None,
        exit: None,
        activity: None,
        operation: Some(BehaviorOperation::Observation {
            observer: observation.observer,
            source: source.to_string(),
            occurrence: occurrence.to_string(),
        }),
    }
}

fn is_generated_observation_behavior<T: Instance + 'static>(
    model: &Model<T>,
    behavior_name: &str,
) -> bool {
    model
        .get_behavior(behavior_name)
        .map(is_observation_behavior)
        .unwrap_or(false)
}

fn is_observation_behavior<T: Instance + 'static>(behavior: &Behavior<T>) -> bool {
    matches!(
        behavior.operation,
        Some(BehaviorOperation::Observation { .. })
    )
}
