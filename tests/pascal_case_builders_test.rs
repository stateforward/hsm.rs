use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use stateforward_hsm::*;

#[derive(Debug)]
struct PascalCaseInstance;

impl Instance for PascalCaseInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn short_duration(_ctx: &Context, _inst: &PascalCaseInstance, _event: &Event) -> Duration {
    Duration::from_millis(1)
}

fn noop_effect(
    _ctx: &Context,
    _inst: &mut PascalCaseInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async {})
}

#[tokio::test]
async fn pascal_case_builders_match_lowercase_dsl_semantics() -> Result<()> {
    let ctx = Context::new();
    let model: Model<PascalCaseInstance> = Define(
        "PascalCaseMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Transition(vec![On("go"), Target("../done")]),
                    Transition(vec![After(short_duration), Target("../done")]),
                    Transition(vec![Every(short_duration), Effect(noop_effect)]),
                ],
            ),
            Choice("branch", vec![Transition(vec![Target("done")])]),
            Final("done"),
        ],
    );

    validate(&model)?;

    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    assert_eq!(hsm.state(), "/PascalCaseMachine/idle");

    hsm.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/PascalCaseMachine/done");

    Ok(())
}

#[tokio::test]
async fn pascal_on_and_defer_accept_event_values() -> Result<()> {
    let ctx = Context::new();
    let trigger = Event::new("go");
    let model: Model<PascalCaseInstance> = Define(
        "PascalEventValueMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Defer(vec![Event::new("resume")]),
                    Transition(vec![On(&trigger), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    );

    validate(&model)?;
    let idle = model.get_state("/PascalEventValueMachine/idle").unwrap();
    assert_eq!(idle.deferred, vec!["resume"]);
    let transition = model.get_transition(&idle.vertex.transitions[0]).unwrap();
    assert_eq!(transition.events, vec!["go"]);

    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, trigger).await?;
    assert_eq!(hsm.state(), "/PascalEventValueMachine/done");

    Ok(())
}

#[tokio::test]
async fn macro_on_and_defer_accept_event_values() -> Result<()> {
    let ctx = Context::new();
    let trigger = Event::new("go");
    let model: Model<PascalCaseInstance> = define!(
        "MacroEventValueMachine",
        initial!(target!("idle")),
        state!(
            "idle",
            defer!(Event::new("resume")),
            transition!(on!(&trigger), target!("../done"))
        ),
        state!("done")
    );

    validate(&model)?;
    let idle = model.get_state("/MacroEventValueMachine/idle").unwrap();
    assert_eq!(idle.deferred, vec!["resume"]);
    let transition = model.get_transition(&idle.vertex.transitions[0]).unwrap();
    assert_eq!(transition.events, vec!["go"]);

    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, trigger).await?;
    assert_eq!(hsm.state(), "/MacroEventValueMachine/done");

    Ok(())
}

#[tokio::test]
async fn pascal_source_routes_top_level_transition() -> Result<()> {
    let ctx = Context::new();
    let model: Model<PascalCaseInstance> = Define(
        "PascalSourceMachine",
        vec![
            Initial(vec![Target("idle")]),
            Transition(vec![On("go"), Source("idle"), Target("done")]),
            State("idle", vec![]),
            State("done", vec![]),
        ],
    );

    validate(&model)?;
    let transition = model
        .members
        .values()
        .find_map(|member| match member {
            ElementVariant::Transition(transition)
                if transition.events.iter().any(|event| event == "go") =>
            {
                Some(transition)
            }
            _ => None,
        })
        .expect("top-level source transition should exist");
    assert_eq!(transition.source, "/PascalSourceMachine/idle");
    assert_eq!(transition.target, "/PascalSourceMachine/done");

    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/PascalSourceMachine/done");

    Ok(())
}

#[tokio::test]
async fn macro_pascal_source_routes_top_level_transition() -> Result<()> {
    let ctx = Context::new();
    let model: Model<PascalCaseInstance> = define!(
        "MacroPascalSourceMachine",
        initial!(target!("idle")),
        transition!(on!("go"), Source!("idle"), target!("done")),
        state!("idle"),
        state!("done")
    );

    validate(&model)?;
    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/MacroPascalSourceMachine/done");

    Ok(())
}

#[tokio::test]
async fn source_relative_parent_segments_are_normalized() -> Result<()> {
    let ctx = Context::new();
    let model: Model<PascalCaseInstance> = Define(
        "RelativeSourceMachine",
        vec![
            Initial(vec![Target("other")]),
            State(
                "outer",
                vec![Transition(vec![
                    On("go"),
                    Source("../other"),
                    Target("../done"),
                ])],
            ),
            State("other", vec![]),
            State("done", vec![]),
        ],
    );

    validate(&model)?;
    let transition = model
        .members
        .values()
        .find_map(|member| match member {
            ElementVariant::Transition(transition)
                if transition.events.iter().any(|event| event == "go") =>
            {
                Some(transition)
            }
            _ => None,
        })
        .expect("relative source transition should exist");
    assert_eq!(transition.source, "/RelativeSourceMachine/other");
    assert_eq!(transition.target, "/RelativeSourceMachine/done");

    let hsm = start(&ctx, PascalCaseInstance, model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/RelativeSourceMachine/done");

    Ok(())
}
