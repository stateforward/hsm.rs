use std::future::Future;
use std::pin::Pin;

use stateforward_hsm::*;

#[derive(Debug, Default)]
struct ObserveInstance {
    log: Vec<String>,
}

impl Instance for ObserveInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn observe_log(
    _ctx: &Context,
    inst: &mut ObserveInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let data = event
        .get_data::<ObservationData>()
        .expect("observer should receive observation data");
    inst.log
        .push(format!("observe:{}:{}", data.Occurrence, data.Event.name));
    Box::pin(async {})
}

fn effect_log(
    _ctx: &Context,
    inst: &mut ObserveInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push(format!("effect:{}", event.name));
    Box::pin(async {})
}

fn entry_log(
    _ctx: &Context,
    inst: &mut ObserveInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push(format!("entry:{}", event.name));
    Box::pin(async {})
}

#[tokio::test]
async fn observe_event_value_runs_before_matching_transition_effect() -> Result<()> {
    let ctx = Context::new();
    let trigger = Event::new("go");
    let model: Model<ObserveInstance> = Define(
        "ObserveEventMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![
                    On(&trigger),
                    Target("../done"),
                    Effect(effect_log),
                ])],
            ),
            State("done", vec![]),
            Observe(observe_log, vec![trigger.clone()]),
        ],
    );

    validate(&model)?;
    assert!(
        model
            .members
            .values()
            .any(|member| matches!(member, ElementVariant::Observation(_))),
        "observe should be represented as a model member"
    );

    let hsm = start(&ctx, ObserveInstance::default(), model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, trigger).await?;

    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec!["observe:event:go".to_string(), "effect:go".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn observe_behavior_runs_before_target_behavior() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ObserveInstance> = Define(
        "ObserveBehaviorMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(entry_log)]),
            Observe(observe_log, vec!["/ObserveBehaviorMachine/idle/entry"]),
        ],
    );

    validate(&model)?;
    let hsm = start(&ctx, ObserveInstance::default(), model)?;
    hsm.start().await?;

    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "observe:behavior:hsm/initial".to_string(),
            "entry:hsm/initial".to_string()
        ]
    );

    Ok(())
}

#[tokio::test]
async fn observe_macro_targets_behavior_names() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ObserveInstance> = define!(
        "MacroObserveMachine",
        initial!(target!("idle")),
        state!("idle", entry!(entry_log)),
        observe!(observe_log, "/MacroObserveMachine/idle/entry")
    );

    validate(&model)?;
    let hsm = start(&ctx, ObserveInstance::default(), model)?;
    hsm.start().await?;

    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "observe:behavior:hsm/initial".to_string(),
            "entry:hsm/initial".to_string()
        ]
    );

    Ok(())
}

#[tokio::test]
async fn redefine_rebuilds_observation_behaviors_without_duplicates() -> Result<()> {
    let ctx = Context::new();
    let base: Model<ObserveInstance> = Define(
        "ObserveBaseMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(entry_log)]),
            Observe(observe_log, vec!["/ObserveBaseMachine/idle/entry"]),
        ],
    );

    let redefined = RedefineAs(&base, "ObserveRedefinedMachine", vec![]);
    validate(&redefined)?;

    let idle = redefined
        .get_state("/ObserveRedefinedMachine/idle")
        .expect("redefined idle state should exist");
    assert_eq!(idle.entry.len(), 2);

    let hsm = start(&ctx, ObserveInstance::default(), redefined)?;
    hsm.start().await?;

    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "observe:behavior:hsm/initial".to_string(),
            "entry:hsm/initial".to_string()
        ]
    );

    Ok(())
}
