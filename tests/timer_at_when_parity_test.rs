use std::time::{Duration, SystemTime};

use rust::*;

#[derive(Debug)]
struct ParityInstance;

impl Instance for ParityInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn soon(_ctx: &Context, _inst: &ParityInstance, _event: &Event) -> SystemTime {
    SystemTime::now() + Duration::from_millis(1)
}

#[tokio::test]
async fn when_is_canonical_alias_for_on_set() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ParityInstance> = Define(
        "WhenAliasMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![When("count"), Target("../changed")])],
            ),
            State("changed", vec![]),
        ],
    );

    let hsm = start(&ctx, ParityInstance, model)?;
    hsm.start().await?;

    hsm.Set("count", 1);
    assert_eq!(hsm.state(), "/WhenAliasMachine/changed");

    Ok(())
}

#[tokio::test]
async fn at_declares_absolute_timer_transition() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ParityInstance> = Define(
        "AtTimerMachine",
        vec![
            Initial(vec![Target("waiting")]),
            State(
                "waiting",
                vec![Transition(vec![At(soon), Target("../done")])],
            ),
            State("done", vec![]),
        ],
    );

    validate(&model)?;
    let hsm = start(&ctx, ParityInstance, model)?;
    hsm.start().await?;

    hsm.dispatch(&ctx, Event::time_event("hsm_timer_at"))
        .await?;
    assert_eq!(hsm.state(), "/AtTimerMachine/done");

    Ok(())
}
