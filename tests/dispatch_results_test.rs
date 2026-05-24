use rust::*;

#[derive(Debug)]
struct ResultInstance;

impl Instance for ResultInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[tokio::test]
async fn dispatch_completes_when_submitted_event_is_deferred() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ResultInstance> = Define(
        "DeferredDispatchCompletionMachine",
        vec![
            Initial(vec![Target("busy")]),
            State(
                "busy",
                vec![
                    Defer(vec!["work"]),
                    Transition(vec![On("ready"), Target("../ready")]),
                ],
            ),
            State(
                "ready",
                vec![Transition(vec![On("work"), Target("../done")])],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ResultInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::new("work")).await?;
    assert_eq!(hsm.state(), "/DeferredDispatchCompletionMachine/busy");
    hsm.dispatch(&ctx, Event::new("ready")).await?;
    assert_eq!(hsm.state(), "/DeferredDispatchCompletionMachine/done");

    Ok(())
}
