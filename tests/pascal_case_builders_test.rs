use std::time::Duration;

use rust::*;

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
                    Transition(vec![Every(short_duration)]),
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
