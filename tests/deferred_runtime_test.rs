use rust::*;

#[derive(Debug)]
struct DeferredInstance;

impl Instance for DeferredInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[tokio::test]
async fn deferred_event_is_replayed_after_state_change() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "DeferredRuntimeMachine",
        vec![
            Initial(vec![Target("busy")]),
            Transition(vec![On("resume"), Target("failed")]),
            State(
                "busy",
                vec![
                    Defer(vec!["resume"]),
                    Transition(vec![On("finish"), Target("../ready")]),
                ],
            ),
            State(
                "ready",
                vec![Transition(vec![On("resume"), Target("../resumed")])],
            ),
            State("resumed", vec![]),
            State("failed", vec![]),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/DeferredRuntimeMachine/busy");

    hsm.dispatch(&ctx, Event::new("resume")).await?;
    assert_eq!(hsm.state(), "/DeferredRuntimeMachine/busy");

    hsm.dispatch(&ctx, Event::new("finish")).await?;
    assert_eq!(hsm.state(), "/DeferredRuntimeMachine/resumed");

    Ok(())
}
