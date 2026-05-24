use rust::*;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;

#[derive(Default)]
struct OperationInstance {
    log: Vec<String>,
    allow: bool,
}

impl Instance for OperationInstance {
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
    machine.call(&ctx, "record").await.unwrap();
    machine.dispatch(&ctx, Event::new("go")).await.unwrap();

    let instance = machine.instance().read().unwrap();
    assert_eq!(
        instance.log,
        vec![
            "record:hsm_initial",
            "record:hsm_call:/machine/record",
            "record:go",
        ]
    );
    assert_eq!(machine.state(), "/machine/done");
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
            "record:hsm_call:/machine/record",
            "record:hsm_call:/machine/record",
            "record:hsm_call:/machine/record",
        ]
    );
    assert_eq!(machine.state(), "/machine/done");
}
