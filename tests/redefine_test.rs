use stateforward_hsm::{
    Context, Element, ElementVariant, Event, Instance, Model, Result, call_trigger_name, define,
    initial_with_target, on, on_call, operation, redefine, redefine_as, start, state,
    state_with_behaviors, target, transition,
};

use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
struct RedefineInstance {
    calls: Vec<String>,
}

impl RedefineInstance {
    fn new() -> Self {
        Self { calls: Vec::new() }
    }
}

impl Instance for RedefineInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn record_call(
    _ctx: &Context,
    inst: &mut RedefineInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.calls.push("run".to_string());
    Box::pin(async move {})
}

fn transition_target_for_event(model: &Model<RedefineInstance>, event_name: &str) -> String {
    model
        .members
        .values()
        .find_map(|element| match element {
            ElementVariant::Transition(transition)
                if transition.events.iter().any(|event| event == event_name) =>
            {
                Some(transition.target.clone())
            }
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn redefine_extends_existing_state_without_erasing_model() -> Result<()> {
    let ctx = Context::new();
    let base = define(
        "PackageRedefine",
        vec![
            initial_with_target(target("idle")),
            state("idle"),
            state("ready"),
        ],
    );

    let model = redefine(
        &base,
        vec![
            state("done"),
            state_with_behaviors("idle", vec![transition(vec![on("go"), target("../done")])]),
        ],
    );

    assert!(model.members.contains_key("/PackageRedefine/ready"));
    assert!(model.members.contains_key("/PackageRedefine/done"));
    assert_eq!(
        transition_target_for_event(&model, "go"),
        "/PackageRedefine/done"
    );

    let machine = start(&ctx, RedefineInstance::new(), model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/PackageRedefine/idle");

    machine.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(machine.state(), "/PackageRedefine/done");

    Ok(())
}

#[tokio::test]
async fn redefine_as_rebases_existing_model_paths() -> Result<()> {
    let ctx = Context::new();
    let base = define(
        "BaseModel",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![transition(vec![on("go"), target("../done")])]),
            state("done"),
        ],
    );

    let model = redefine_as(&base, "RenamedModel", vec![state("extra")]);

    assert_eq!(model.qualified_name(), "/RenamedModel");
    assert!(model.members.contains_key("/RenamedModel/idle"));
    assert!(model.members.contains_key("/RenamedModel/done"));
    assert!(model.members.contains_key("/RenamedModel/extra"));
    assert!(!model.members.contains_key("/BaseModel/idle"));
    assert_eq!(model.state.initial, "/RenamedModel/.initial");
    assert_eq!(
        transition_target_for_event(&model, "go"),
        "/RenamedModel/done"
    );

    let machine = start(&ctx, RedefineInstance::new(), model)?;
    machine.start().await?;
    assert_eq!(machine.state(), "/RenamedModel/idle");

    machine.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(machine.state(), "/RenamedModel/done");

    Ok(())
}

#[test]
fn redefine_as_rebases_call_event_names() {
    let base = define(
        "CallBase",
        vec![
            initial_with_target(target("idle")),
            operation("run", record_call),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call("run"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let model = redefine_as(&base, "CallDerived", vec![]);

    assert_eq!(
        transition_target_for_event(&model, &call_trigger_name("/CallDerived/run")),
        "/CallDerived/done"
    );
}
