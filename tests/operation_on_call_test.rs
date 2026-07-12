use stateforward_hsm::*;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Default)]
struct OperationInstance {
    log: Vec<String>,
    allow: bool,
}

#[derive(Default)]
struct CallPayloadInstance {
    payloads: Vec<(String, usize)>,
}

impl Instance for OperationInstance {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Instance for CallPayloadInstance {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn record_operation(
    _ctx: &Context,
    inst: &mut OperationInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push(format!("record:{}", event.name));
    Box::pin(async {})
}

fn capture_call_payload(
    _ctx: &Context,
    inst: &mut CallPayloadInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let data = event
        .get_data::<CallData>()
        .expect("call event should carry CallData payload");
    inst.payloads.push((data.Name.clone(), data.Args.len()));
    Box::pin(async {})
}

fn capture_args_operation(
    _ctx: &Context,
    inst: &mut OperationInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let data = event
        .get_data::<CallData>()
        .expect("call event should carry CallData payload");
    let count = data
        .Args
        .first()
        .and_then(|value| value.downcast_ref::<i32>())
        .copied()
        .expect("first arg should be i32");
    let label = data
        .Args
        .get(1)
        .and_then(|value| value.downcast_ref::<String>())
        .expect("second arg should be String");
    inst.log
        .push(format!("args:{}:{}:{}", data.Name, count, label));
    Box::pin(async {})
}

fn allow_guard(_ctx: &Context, inst: &OperationInstance, _event: &Event) -> bool {
    inst.allow
}

#[tokio::test]
async fn named_operation_can_be_called_and_referenced_by_behaviors() {
    let ctx = Context::new();
    let model = define(
        "machine",
        vec![
            operation("record", record_operation),
            state_with_behaviors(
                "idle",
                vec![
                    entry_operation("record"),
                    transition(vec![on("go"), target("done"), effect_operation("record")]),
                ],
            ),
            state("done"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(&ctx, OperationInstance::default(), model).unwrap();
    machine.start().await.unwrap();
    call(&ctx, &machine, "record").await.unwrap();
    machine.dispatch(&ctx, Event::new("go")).await.unwrap();

    let instance = machine.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec!["record:hsm/initial", "record:/machine/record", "record:go",]
    );
    assert_eq!(machine.state(), "/machine/done");
}

#[tokio::test]
async fn call_event_carries_call_data_payload() -> Result<()> {
    let ctx = Context::new();
    let model = define(
        "CallPayloadMachine",
        vec![
            operation("record", capture_call_payload),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call("record"), target("done")])],
            ),
            state("done"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(&ctx, CallPayloadInstance::default(), model)?;
    machine.start().await?;
    Call(&ctx, &machine, "record").await?;

    let instance = machine.instance().read().unwrap();
    assert_eq!(
        instance.payloads,
        vec![("/CallPayloadMachine/record".to_string(), 0)]
    );
    assert_eq!(machine.state(), "/CallPayloadMachine/done");

    Ok(())
}

#[tokio::test]
async fn call_with_args_populates_operation_and_transition_payloads() -> Result<()> {
    let ctx = Context::new();
    let model = define(
        "ArgsMachine",
        vec![
            operation("capture", capture_args_operation),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on_call("capture"),
                    target("done"),
                    effect_operation("capture"),
                ])],
            ),
            state("done"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(&ctx, OperationInstance::default(), model)?;
    machine.start().await?;
    let args: Vec<Arc<dyn Any + Send + Sync>> = vec![Arc::new(7_i32), Arc::new(String::from("ok"))];
    CallWithArgs(&ctx, &machine, "capture", args).await?;

    let instance = machine.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "args:/ArgsMachine/capture:7:ok",
            "args:/ArgsMachine/capture:7:ok",
        ]
    );
    assert_eq!(machine.state(), "/ArgsMachine/done");

    Ok(())
}

#[tokio::test]
async fn call_requires_started_machine_before_executing_operation() -> Result<()> {
    let ctx = Context::new();
    let model = define(
        "machine",
        vec![
            operation("record", record_operation),
            state("idle"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(&ctx, OperationInstance::default(), model)?;

    let error = machine.call(&ctx, "record").await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "operation requires a started HSM"
    ));
    assert!(machine.instance().read().unwrap().log.is_empty());

    machine.start().await?;
    Stop(&ctx, &machine).await?;

    let error = Call(&ctx, &machine, "record").await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "operation requires a started HSM"
    ));
    assert!(machine.instance().read().unwrap().log.is_empty());

    Ok(())
}

#[tokio::test]
async fn call_missing_operation_fails_before_dispatching_on_call() -> Result<()> {
    let ctx = Context::new();
    let model = define(
        "machine",
        vec![
            state_with_behaviors("idle", vec![transition(vec![on("go"), target("done")])]),
            state("done"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(&ctx, OperationInstance::default(), model)?;
    machine.start().await?;

    let error = Call(&ctx, &machine, "missing").await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "missing operation \"missing\""
    ));
    assert_eq!(machine.state(), "/machine/idle");

    Ok(())
}

#[tokio::test]
async fn on_call_transition_uses_named_guard_operation() {
    let ctx = Context::new();
    let model = define(
        "machine",
        vec![
            operation("record", record_operation),
            guard_operation("allow", allow_guard),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on_call("record"),
                    guard_operation_ref("allow"),
                    target("done"),
                    effect_operation("record"),
                ])],
            ),
            state("done"),
            initial_with_target(target("idle")),
        ],
    );

    let machine = start(
        &ctx,
        OperationInstance {
            allow: false,
            ..Default::default()
        },
        model,
    )
    .unwrap();
    machine.start().await.unwrap();
    machine.call(&ctx, "record").await.unwrap();
    assert_eq!(machine.state(), "/machine/idle");

    machine.instance_mut().allow = true;
    machine.call(&ctx, "record").await.unwrap();

    let instance = machine.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "record:/machine/record",
            "record:/machine/record",
            "record:/machine/record",
        ]
    );
    assert_eq!(machine.state(), "/machine/done");
}
