use stateforward_hsm::*;

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

#[tokio::test]
async fn direct_dispatch_requires_started_machine() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ResultInstance> = Define(
        "InactiveDispatchMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ResultInstance, model)?;

    let error = Dispatch(&ctx, &hsm, Event::new("go")).await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "dispatch requires a started HSM"
    ));
    assert_eq!(hsm.state(), "");

    hsm.start().await?;
    Stop(&ctx, &hsm).await?;

    let error = hsm.dispatch(&ctx, Event::new("go")).await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "dispatch requires a started HSM"
    ));
    assert_eq!(hsm.state(), "");

    Ok(())
}

#[tokio::test]
async fn package_dispatch_forwards_to_started_machine() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ResultInstance> = Define(
        "PackageDispatchMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    let hsm = Started(&ctx, ResultInstance, model).await?;

    Dispatch(&ctx, &hsm, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/PackageDispatchMachine/done");

    Ok(())
}

#[tokio::test]
async fn borrowed_dispatch_matches_owned_dispatch_lifecycle() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ResultInstance> = Define(
        "BorrowedDispatchMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, ResultInstance, model)?;

    let error = hsm
        .dispatch_borrowed(&ctx, Event::new("go"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "dispatch requires a started HSM"
    ));

    hsm.start().await?;
    hsm.dispatch_borrowed(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/BorrowedDispatchMachine/done");

    Ok(())
}
