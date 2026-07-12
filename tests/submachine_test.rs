use stateforward_hsm::{
    Context, Element, Event, Instance, Result, define, entry_operation, entry_point, exit_point,
    initial_with_target, kind, on, on_call, operation, start, state, state_with_behaviors,
    submachine_state, target, transition,
};
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
struct SubmachineInstance;

impl Instance for SubmachineInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Debug, Default)]
struct OperationSubmachineInstance {
    log: Vec<String>,
}

impl Instance for OperationSubmachineInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn record_operation(
    _ctx: &Context,
    inst: &mut OperationSubmachineInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log.push(event.name.clone());
    Box::pin(async {})
}

#[tokio::test]
async fn submachine_state_enters_child_initial_state() -> Result<()> {
    let ctx = Context::new();
    let child: stateforward_hsm::Model<SubmachineInstance> = define(
        "Motor",
        vec![
            initial_with_target(target("off")),
            state_with_behaviors("off", vec![transition(vec![on("start"), target("../on")])]),
            state("on"),
        ],
    );
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "Drive",
        vec![
            initial_with_target(target("motor")),
            submachine_state("motor", child, vec![]),
        ],
    );

    let boundary = model.get_state("/Drive/motor").unwrap();
    assert!(kind::is_kind(boundary.kind(), kind::SUBMACHINE_STATE));
    assert!(kind::is_kind(boundary.kind(), kind::STATE));

    let machine = start(&ctx, SubmachineInstance, model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/Drive/motor/off");

    machine.dispatch(&ctx, Event::new("start")).await?;
    assert_eq!(machine.state(), "/Drive/motor/on");

    Ok(())
}

#[tokio::test]
async fn submachine_operations_are_lifted_to_parent_namespace() -> Result<()> {
    let ctx = Context::new();
    let child: stateforward_hsm::Model<OperationSubmachineInstance> = define(
        "OperationChild",
        vec![
            operation("approve", record_operation),
            initial_with_target(target("waiting")),
            state_with_behaviors(
                "waiting",
                vec![
                    entry_operation("approve"),
                    transition(vec![on_call("approve"), target("../approved")]),
                ],
            ),
            state("approved"),
        ],
    );
    let model: stateforward_hsm::Model<OperationSubmachineInstance> = define(
        "OperationParent",
        vec![
            initial_with_target(target("drive")),
            submachine_state("drive", child, vec![]),
        ],
    );

    assert!(model.get_operation("/OperationParent/approve").is_some());
    assert!(
        model
            .get_operation("/OperationParent/drive/approve")
            .is_none()
    );
    stateforward_hsm::validate(&model)?;

    let machine = start(&ctx, OperationSubmachineInstance::default(), model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/OperationParent/drive/waiting");

    machine.call(&ctx, "approve").await?;
    assert_eq!(machine.state(), "/OperationParent/drive/approved");
    assert_eq!(
        machine.instance().read().unwrap().log,
        vec![
            "hsm/initial".to_string(),
            "/OperationParent/approve".to_string()
        ]
    );

    Ok(())
}

#[tokio::test]
async fn entry_point_selector_enters_named_child_entry() -> Result<()> {
    let ctx = Context::new();
    let child: stateforward_hsm::Model<SubmachineInstance> = define(
        "EntryChild",
        vec![
            entry_point("warm", vec![target("running")]),
            initial_with_target(target("off")),
            state("off"),
            state("running"),
        ],
    );
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "EntryParent",
        vec![
            initial_with_target(target("outside")),
            state_with_behaviors(
                "outside",
                vec![transition(vec![
                    on("start"),
                    target("../drive"),
                    entry_point("warm", vec![]),
                ])],
            ),
            submachine_state("drive", child, vec![]),
        ],
    );

    let machine = start(&ctx, SubmachineInstance, model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/EntryParent/outside");

    machine.dispatch(&ctx, Event::new("start")).await?;
    assert_eq!(machine.state(), "/EntryParent/drive/running");

    Ok(())
}

#[tokio::test]
async fn exit_point_handler_leaves_submachine_boundary() -> Result<()> {
    let ctx = Context::new();
    let child: stateforward_hsm::Model<SubmachineInstance> = define(
        "ExitChild",
        vec![
            exit_point("done", vec![]),
            initial_with_target(target("active")),
            state_with_behaviors(
                "active",
                vec![transition(vec![on("finish"), target("../done")])],
            ),
        ],
    );
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "ExitParent",
        vec![
            initial_with_target(target("drive")),
            submachine_state(
                "drive",
                child,
                vec![transition(vec![
                    exit_point("done", vec![]),
                    target("../complete"),
                ])],
            ),
            state("complete"),
        ],
    );

    let machine = start(&ctx, SubmachineInstance, model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/ExitParent/drive/active");

    machine.dispatch(&ctx, Event::new("finish")).await?;
    assert_eq!(machine.state(), "/ExitParent/complete");

    Ok(())
}

#[tokio::test]
async fn submachine_connection_point_macros_match_builder_semantics() -> Result<()> {
    let ctx = Context::new();
    let child = stateforward_hsm::define!(
        "MacroChild",
        stateforward_hsm::exit_point!("done"),
        stateforward_hsm::initial!(stateforward_hsm::target!("active")),
        stateforward_hsm::state!(
            "active",
            stateforward_hsm::transition!(
                stateforward_hsm::on!("finish"),
                stateforward_hsm::target!("../done")
            )
        )
    );
    let model = stateforward_hsm::define!(
        "MacroParent",
        stateforward_hsm::initial!(stateforward_hsm::target!("drive")),
        stateforward_hsm::submachine_state!(
            "drive",
            child,
            stateforward_hsm::transition!(
                stateforward_hsm::exit_point!("done"),
                stateforward_hsm::target!("../complete")
            )
        ),
        stateforward_hsm::state!("complete")
    );

    let machine = start(&ctx, SubmachineInstance, model)?;
    machine.start().await?;
    machine.dispatch(&ctx, Event::new("finish")).await?;
    assert_eq!(machine.state(), "/MacroParent/complete");

    Ok(())
}

#[test]
fn parent_transition_cannot_target_submachine_internal_state() {
    let child = define(
        "InternalTargetChild",
        vec![initial_with_target(target("active")), state("active")],
    );
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "InternalTargetParent",
        vec![
            initial_with_target(target("outside")),
            state_with_behaviors(
                "outside",
                vec![transition(vec![on("start"), target("../drive/active")])],
            ),
            submachine_state("drive", child, vec![]),
        ],
    );

    let error = stateforward_hsm::validate(&model).unwrap_err();
    assert!(format!("{:?}", error).contains("internal state"));
}

#[test]
fn entry_point_selector_requires_submachine_boundary() {
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "EntrySelectorRequiresSubmachine",
        vec![
            initial_with_target(target("outside")),
            state_with_behaviors(
                "outside",
                vec![transition(vec![
                    on("start"),
                    target("../plain"),
                    entry_point("warm", vec![]),
                ])],
            ),
            state_with_behaviors(
                "plain",
                vec![entry_point("warm", vec![target("inside")]), state("inside")],
            ),
        ],
    );

    let error = stateforward_hsm::validate(&model).unwrap_err();
    assert!(format!("{:?}", error).contains("requires a submachine"));
}

#[test]
fn entry_point_declaration_cannot_target_outside_boundary() {
    let child: stateforward_hsm::Model<SubmachineInstance> = define(
        "EscapingEntryChild",
        vec![initial_with_target(target("active")), state("active")],
    );
    let model: stateforward_hsm::Model<SubmachineInstance> = define(
        "EscapingEntryParent",
        vec![
            initial_with_target(target("drive")),
            submachine_state(
                "drive",
                child,
                vec![entry_point("bad", vec![target("../outside")])],
            ),
            state("outside"),
        ],
    );

    let error = stateforward_hsm::validate(&model).unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(error_msg.contains("must target inside") || error_msg.contains("outside submachine"));
}
