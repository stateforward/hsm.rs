use rust::*;

#[derive(Debug)]
struct AttributeInstance;

impl Instance for AttributeInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[tokio::test]
async fn attribute_set_emits_on_set_and_ignores_invalid_updates() -> Result<()> {
    let ctx = Context::new();
    let model: Model<AttributeInstance> = Define(
        "AttributeSetMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![OnSet("count"), Target("../changed")])],
            ),
            State("changed", vec![]),
        ],
    );

    let hsm = start(&ctx, AttributeInstance, model)?;
    hsm.start().await?;

    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));
    hsm.Set("missing", 1);
    hsm.Set("count", true);
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));

    hsm.Set("count", 1);
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(hsm.state(), "/AttributeSetMachine/changed");

    Ok(())
}
