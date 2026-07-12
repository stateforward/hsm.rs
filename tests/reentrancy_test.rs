use stateforward_hsm::*;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Default)]
struct ReentrantInstance;

impl Instance for ReentrantInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn dispatch_audit(
    ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        DispatchFromContext(&ctx, Event::new("audit"))
            .await
            .expect("behavior dispatch should enqueue");
    })
}

fn dispatch_all_audit(
    ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        DispatchAll(&ctx, Event::new("audit"))
            .await
            .expect("behavior broadcast should enqueue");
    })
}

fn dispatch_to_current_audit(
    ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        DispatchTo(&ctx, Event::new("audit"), vec!["current"])
            .await
            .expect("behavior targeted dispatch should enqueue");
    })
}

fn set_count(
    ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        SetFromContext(&ctx, "count", 1).expect("behavior set should enqueue change event");
    })
}

fn call_record(
    ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        CallFromContext(&ctx, "record")
            .await
            .expect("behavior call should enqueue call event");
    })
}

fn record_operation(
    _ctx: &Context,
    _instance: &mut ReentrantInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async {})
}

#[tokio::test]
async fn behavior_dispatch_from_entry_replays_after_entry_state_is_active() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ReentrantInstance> = Define(
        "EntryDispatchMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(dispatch_audit),
                    Transition(vec![On("audit"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ReentrantInstance, model)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/EntryDispatchMachine/done");

    Ok(())
}

#[tokio::test]
async fn behavior_dispatch_all_from_entry_selects_current_active_machine() -> Result<()> {
    let ctx = Context::new();
    let mut config = Config();
    config.ID = Some("current".to_string());
    let model: Model<ReentrantInstance> = Define(
        "EntryDispatchAllMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(dispatch_all_audit),
                    Transition(vec![On("audit"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = StartWithConfig(&ctx, ReentrantInstance, model, config)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/EntryDispatchAllMachine/done");

    Ok(())
}

#[tokio::test]
async fn behavior_dispatch_to_from_entry_selects_current_active_machine() -> Result<()> {
    let ctx = Context::new();
    let mut config = Config();
    config.ID = Some("current".to_string());
    let model: Model<ReentrantInstance> = Define(
        "EntryDispatchToMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(dispatch_to_current_audit),
                    Transition(vec![On("audit"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = StartWithConfig(&ctx, ReentrantInstance, model, config)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/EntryDispatchToMachine/done");

    Ok(())
}

#[tokio::test]
async fn behavior_set_from_entry_replays_generated_change_after_entry_state_is_active() -> Result<()>
{
    let ctx = Context::new();
    let model: Model<ReentrantInstance> = Define(
        "EntrySetMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(set_count),
                    Transition(vec![OnSet("count"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ReentrantInstance, model)?;
    hsm.start().await?;

    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(hsm.state(), "/EntrySetMachine/done");

    Ok(())
}

#[tokio::test]
async fn behavior_call_from_entry_replays_generated_call_after_entry_state_is_active() -> Result<()>
{
    let ctx = Context::new();
    let model: Model<ReentrantInstance> = Define(
        "EntryCallMachine",
        vec![
            Operation("record", record_operation),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(call_record),
                    Transition(vec![OnCall("record"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ReentrantInstance, model)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/EntryCallMachine/done");

    Ok(())
}

#[tokio::test]
async fn transition_effect_dispatch_replays_after_compound_target_resolution() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ReentrantInstance> = Define(
        "EffectDispatchMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![
                    On("go"),
                    Target("../parent"),
                    Effect(dispatch_audit),
                ])],
            ),
            State(
                "parent",
                vec![
                    Initial(vec![Target("child")]),
                    State(
                        "child",
                        vec![Transition(vec![On("audit"), Target("../done")])],
                    ),
                    State("done", vec![]),
                ],
            ),
        ],
    );

    let hsm = start(&ctx, ReentrantInstance, model)?;
    hsm.start().await?;
    Dispatch(&ctx, &hsm, Event::new("go")).await?;

    assert_eq!(hsm.state(), "/EffectDispatchMachine/parent/done");

    Ok(())
}
