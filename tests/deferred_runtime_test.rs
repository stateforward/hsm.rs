use stateforward_hsm::*;

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

fn false_guard(_ctx: &Context, _inst: &DeferredInstance, _event: &Event) -> bool {
    false
}

#[tokio::test]
async fn deferred_event_is_replayed_after_state_change() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "DeferredRuntimeMachine",
        vec![
            Initial(vec![Target("busy")]),
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

#[tokio::test]
async fn child_transition_precedes_parent_defer() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "ChildTransitionPrecedesParentDeferMachine",
        vec![
            Initial(vec![Target("parent")]),
            State(
                "parent",
                vec![
                    Initial(vec![Target("child")]),
                    Defer(vec!["work"]),
                    State(
                        "child",
                        vec![Transition(vec![On("work"), Target("../handled")])],
                    ),
                    State("handled", vec![]),
                ],
            ),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    assert_eq!(
        hsm.state(),
        "/ChildTransitionPrecedesParentDeferMachine/parent/child"
    );

    hsm.dispatch(&ctx, Event::new("work")).await?;
    assert_eq!(
        hsm.state(),
        "/ChildTransitionPrecedesParentDeferMachine/parent/handled"
    );

    Ok(())
}

#[tokio::test]
async fn false_child_guard_falls_through_to_parent_defer() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "FalseChildGuardFallsThroughToParentDeferMachine",
        vec![
            Initial(vec![Target("parent")]),
            State(
                "parent",
                vec![
                    Initial(vec![Target("child")]),
                    Defer(vec!["maybe"]),
                    Transition(vec![On("release"), Target("../outside")]),
                    State(
                        "child",
                        vec![Transition(vec![
                            On("maybe"),
                            Guard(false_guard),
                            Target("../wrong"),
                        ])],
                    ),
                    State("wrong", vec![]),
                ],
            ),
            State(
                "outside",
                vec![Transition(vec![On("maybe"), Target("../done")])],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::new("maybe")).await?;
    assert_eq!(
        hsm.state(),
        "/FalseChildGuardFallsThroughToParentDeferMachine/parent/child"
    );

    hsm.dispatch(&ctx, Event::new("release")).await?;
    assert_eq!(
        hsm.state(),
        "/FalseChildGuardFallsThroughToParentDeferMachine/done"
    );

    Ok(())
}

#[tokio::test]
async fn source_qualified_transition_precedes_active_defer() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "SourceQualifiedTransitionPrecedesActiveDeferMachine",
        vec![
            Initial(vec![Target("blocked")]),
            State("blocked", vec![Defer(vec!["work"])]),
            State("done", vec![]),
            Transition(vec![source("blocked"), On("work"), Target("done")]),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::new("work")).await?;
    assert_eq!(
        hsm.state(),
        "/SourceQualifiedTransitionPrecedesActiveDeferMachine/done"
    );

    Ok(())
}

#[tokio::test]
async fn parent_declared_source_transition_does_not_precede_child_defer() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "ParentDeclaredSourceTransitionDoesNotPrecedeChildDeferMachine",
        vec![
            Initial(vec![Target("parent")]),
            State(
                "parent",
                vec![
                    Initial(vec![Target("blocked")]),
                    Transition(vec![source("blocked"), On("hold"), Target("../outside")]),
                    State(
                        "blocked",
                        vec![
                            Defer(vec!["hold"]),
                            Transition(vec![On("release"), Target("../ready")]),
                        ],
                    ),
                    State("ready", vec![]),
                ],
            ),
            State("outside", vec![]),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::new("hold")).await?;
    assert_eq!(
        hsm.state(),
        "/ParentDeclaredSourceTransitionDoesNotPrecedeChildDeferMachine/parent/blocked"
    );

    hsm.dispatch(&ctx, Event::new("release")).await?;
    assert_eq!(
        hsm.state(),
        "/ParentDeclaredSourceTransitionDoesNotPrecedeChildDeferMachine/parent/ready"
    );

    Ok(())
}

#[tokio::test]
async fn deferred_replay_can_select_parent_declared_source_transition() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "DeferredReplaySelectsParentDeclaredSourceTransitionMachine",
        vec![
            Initial(vec![Target("parent")]),
            State(
                "parent",
                vec![
                    Initial(vec![Target("blocked")]),
                    Transition(vec![source("ready"), On("work"), Target("../done")]),
                    State(
                        "blocked",
                        vec![
                            Defer(vec!["work"]),
                            Transition(vec![On("release"), Target("../ready")]),
                        ],
                    ),
                    State("ready", vec![]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::new("work")).await?;
    assert_eq!(
        hsm.state(),
        "/DeferredReplaySelectsParentDeclaredSourceTransitionMachine/parent/blocked"
    );

    hsm.dispatch(&ctx, Event::new("release")).await?;
    assert_eq!(
        hsm.state(),
        "/DeferredReplaySelectsParentDeclaredSourceTransitionMachine/done"
    );

    Ok(())
}

#[tokio::test]
async fn stop_clears_deferred_events_before_restart() -> Result<()> {
    let ctx = Context::new();
    let model: Model<DeferredInstance> = Define(
        "StopClearsDeferredMachine",
        vec![
            Initial(vec![Target("busy")]),
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
        ],
    );

    let hsm = start(&ctx, DeferredInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("resume")).await?;

    Stop(&ctx, &hsm).await?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("finish")).await?;

    assert_eq!(hsm.state(), "/StopClearsDeferredMachine/ready");

    Ok(())
}
