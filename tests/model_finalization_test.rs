use stateforward_hsm::{
    Element, ElementVariant, Instance, define, kind, on, state, state_with_behaviors, target,
    transition,
};

#[derive(Debug)]
struct ModelFinalizationInstance;

impl Instance for ModelFinalizationInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn transition_for_event<'a>(
    model: &'a stateforward_hsm::Model<ModelFinalizationInstance>,
    event: &str,
) -> &'a stateforward_hsm::Transition {
    model
        .members
        .values()
        .find_map(|element| match element {
            ElementVariant::Transition(transition)
                if transition.events.iter().any(|e| e == event) =>
            {
                Some(transition)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn finalization_resolves_local_targets_before_root_fallback() {
    let model = define(
        "TargetFinalization",
        vec![
            state_with_behaviors(
                "outer",
                vec![
                    transition(vec![on("local"), target("handled")]),
                    state("handled"),
                ],
            ),
            state_with_behaviors("start", vec![transition(vec![on("root"), target("done")])]),
            state("done"),
            state("handled"),
        ],
    );

    let local = transition_for_event(&model, "local");
    assert_eq!(local.target, "/TargetFinalization/outer/handled");
    assert!(kind::is_kind(local.kind(), kind::LOCAL));

    let root = transition_for_event(&model, "root");
    assert_eq!(root.target, "/TargetFinalization/done");
    assert!(kind::is_kind(root.kind(), kind::EXTERNAL));
}
