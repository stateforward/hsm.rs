use stateforward_hsm::*;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Default)]
struct AnyEventInstance {
    log: Vec<String>,
}

impl Instance for AnyEventInstance {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn record_special(
    _ctx: &Context,
    inst: &mut AnyEventInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push("special".to_string());
    Box::pin(async {})
}

fn record_fallback(
    _ctx: &Context,
    inst: &mut AnyEventInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push(format!("fallback:{}", event.name));
    Box::pin(async {})
}

fn ordinary_event(_ctx: &Context, _inst: &AnyEventInstance, event: &Event) -> bool {
    event.kind == kind::EVENT
}

fn any_event_model(name: &str) -> Model<AnyEventInstance> {
    define(
        name,
        vec![
            initial_with_target(target("ready")),
            state_with_behaviors(
                "ready",
                vec![
                    transition(vec![
                        on("special"),
                        target("../special"),
                        effect(record_special),
                    ]),
                    transition(vec![
                        on(AnyEvent),
                        guard(ordinary_event),
                        target("../fallback"),
                        effect(record_fallback),
                    ]),
                ],
            ),
            state("special"),
            state("fallback"),
        ],
    )
}

#[tokio::test]
async fn any_event_fallback_handles_unmatched_event() -> Result<()> {
    let ctx = Context::new();
    let machine = start(
        &ctx,
        AnyEventInstance::default(),
        any_event_model("AnyEventFallbackMachine"),
    )?;
    machine.start().await?;

    machine.dispatch(&ctx, Event::new("other")).await?;

    assert_eq!(machine.state(), "/AnyEventFallbackMachine/fallback");
    assert_eq!(
        machine.instance().read().unwrap().log,
        vec!["fallback:other".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn specific_event_transition_precedes_any_event() -> Result<()> {
    let ctx = Context::new();
    let machine = start(
        &ctx,
        AnyEventInstance::default(),
        any_event_model("AnyEventPriorityMachine"),
    )?;
    machine.start().await?;

    machine.dispatch(&ctx, Event::new("special")).await?;

    assert_eq!(machine.state(), "/AnyEventPriorityMachine/special");
    assert_eq!(
        machine.instance().read().unwrap().log,
        vec!["special".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn any_event_guard_can_filter_lifecycle_events() -> Result<()> {
    let ctx = Context::new();
    let machine = start(
        &ctx,
        AnyEventInstance::default(),
        any_event_model("AnyEventLifecycleGuardMachine"),
    )?;
    machine.start().await?;

    machine
        .dispatch(&ctx, Event::completion("internal"))
        .await?;

    assert_eq!(machine.state(), "/AnyEventLifecycleGuardMachine/ready");
    assert!(machine.instance().read().unwrap().log.is_empty());

    Ok(())
}
