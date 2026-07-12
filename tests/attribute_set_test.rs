use stateforward_hsm::*;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Default)]
struct AttributeInstance {
    changes: Vec<AttributeChange>,
}

impl Instance for AttributeInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn capture_change(
    _ctx: &Context,
    inst: &mut AttributeInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let change = event
        .get_data::<AttributeChange>()
        .expect("set event should carry AttributeChange payload");
    inst.changes.push(change.clone());
    Box::pin(async {})
}

#[tokio::test]
async fn attribute_set_emits_on_set_and_rejects_invalid_updates() -> Result<()> {
    let ctx = Context::new();
    let model: Model<AttributeInstance> = Define(
        "AttributeSetMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![
                    OnSet("count"),
                    Target("../changed"),
                    Effect(capture_change),
                ])],
            ),
            State("changed", vec![]),
        ],
    );

    let hsm = start(&ctx, AttributeInstance::default(), model)?;
    hsm.start().await?;

    assert_eq!(Get(&ctx, &hsm, "count"), Some(AttributeValue::Int(0)));
    let error = Set(&ctx, &hsm, "missing", 1).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message)
            if message == "missing attribute \"/AttributeSetMachine/missing\""
    ));

    let error = hsm.Set("count", true).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message)
            if message == "attribute \"/AttributeSetMachine/count\" rejected value"
    ));
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));

    Set(&ctx, &hsm, "count", 1)?;
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(hsm.state(), "/AttributeSetMachine/changed");
    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.changes,
        vec![AttributeChange {
            Name: "/AttributeSetMachine/count".to_string(),
            Old: Some(AttributeValue::Int(0)),
            Value: AttributeValue::Int(1),
        }]
    );

    Ok(())
}

#[tokio::test]
async fn submachine_attributes_are_lifted_to_parent_namespace() -> Result<()> {
    let ctx = Context::new();
    let child: Model<AttributeInstance> = Define(
        "AttributeChild",
        vec![
            Attribute("flag", false),
            Initial(vec![Target("waiting")]),
            State(
                "waiting",
                vec![Transition(vec![
                    OnSet("flag"),
                    Target("../changed"),
                    Effect(capture_change),
                ])],
            ),
            State("changed", vec![]),
        ],
    );
    let model: Model<AttributeInstance> = Define(
        "AttributeParent",
        vec![
            Initial(vec![Target("drive")]),
            SubmachineState("drive", child, vec![]),
        ],
    );

    assert!(model.get_attribute("/AttributeParent/flag").is_some());
    assert!(model.get_attribute("/AttributeParent/drive/flag").is_none());
    validate(&model)?;

    let hsm = start(&ctx, AttributeInstance::default(), model)?;
    hsm.start().await?;
    assert_eq!(hsm.state(), "/AttributeParent/drive/waiting");
    assert_eq!(hsm.Get("flag"), Some(AttributeValue::Bool(false)));

    hsm.Set("flag", true)?;
    assert_eq!(hsm.Get("flag"), Some(AttributeValue::Bool(true)));
    assert_eq!(hsm.state(), "/AttributeParent/drive/changed");
    let instance = hsm.instance().read().unwrap();
    assert_eq!(
        instance.changes,
        vec![AttributeChange {
            Name: "/AttributeParent/flag".to_string(),
            Old: Some(AttributeValue::Bool(false)),
            Value: AttributeValue::Bool(true),
        }]
    );

    Ok(())
}

#[tokio::test]
async fn attribute_set_ignores_inactive_machine() -> Result<()> {
    let ctx = Context::new();
    let model: Model<AttributeInstance> = Define(
        "InactiveAttributeSetMachine",
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

    let hsm = start(&ctx, AttributeInstance::default(), model)?;

    let error = Set(&ctx, &hsm, "count", 1).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "set requires a started HSM"
    ));
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));
    assert_eq!(hsm.state(), "/InactiveAttributeSetMachine");

    hsm.start().await?;
    Stop(&ctx, &hsm).await?;

    let error = hsm.Set("count", 1).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "set requires a started HSM"
    ));
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));
    assert_eq!(hsm.state(), "/InactiveAttributeSetMachine");

    Ok(())
}

#[tokio::test]
async fn restart_and_start_reset_runtime_attributes_to_defaults() -> Result<()> {
    let ctx = Context::new();
    let model: Model<AttributeInstance> = Define(
        "RestartAttributeResetMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State("idle", vec![]),
        ],
    );

    let hsm = start(&ctx, AttributeInstance::default(), model)?;
    hsm.start().await?;

    hsm.Set("count", 7)?;
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(7)));

    Restart(&ctx, &hsm).await?;
    assert_eq!(hsm.state(), "/RestartAttributeResetMachine/idle");
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));

    hsm.Set("count", 9)?;
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(9)));

    Stop(&ctx, &hsm).await?;
    hsm.start().await?;
    assert_eq!(hsm.state(), "/RestartAttributeResetMachine/idle");
    assert_eq!(hsm.Get("count"), Some(AttributeValue::Int(0)));

    Ok(())
}
