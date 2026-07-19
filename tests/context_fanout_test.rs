use stateforward_hsm::{
    Attribute, AttributeValue, CallFromContext, Config, Context, DispatchAll, DispatchFromContext,
    DispatchTo, Event, FromContext, GetFromContext, HsmError, Initial, Instance,
    InstancesFromContext, Model, On, Operation, Restart, Result, SetFromContext, StartWithConfig,
    Started, StartedWithConfig, State, Stop, Target, Transition, define,
};
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Default)]
struct FanoutInstance {
    name: String,
}

impl Instance for FanoutInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn record_context_call(
    _ctx: &Context,
    instance: &mut FanoutInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    instance.name = event.name.clone();
    Box::pin(async {})
}

fn fanout_model() -> Model<FanoutInstance> {
    define(
        "FanoutMachine",
        vec![
            Attribute("count", 0),
            Operation("record", record_context_call),
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    )
}

fn configured(id: &str) -> stateforward_hsm::RuntimeConfig {
    let mut config = Config();
    config.ID = Some(id.to_string());
    config
}

#[tokio::test]
async fn start_registers_instances_in_shared_context() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartWithConfig(
        &ctx,
        FanoutInstance {
            name: "alpha".to_string(),
        },
        fanout_model(),
        configured("alpha"),
    )?;
    let bravo = StartWithConfig(
        alpha.context(),
        FanoutInstance {
            name: "bravo".to_string(),
        },
        fanout_model(),
        configured("bravo"),
    )?;

    let (_, ok) = FromContext::<FanoutInstance>(bravo.context());
    assert!(!ok);
    let (instances, ok) = InstancesFromContext(bravo.context());
    assert!(!ok);
    assert!(instances.is_empty());

    alpha.start().await?;
    bravo.start().await?;

    let (current, ok) = FromContext::<FanoutInstance>(bravo.context());
    assert!(ok);
    assert_eq!(current.unwrap().ID(), "bravo");

    let (instances, ok) = InstancesFromContext(bravo.context());
    assert!(ok);
    let ids: Vec<String> = instances.iter().map(|machine| machine.id()).collect();
    assert_eq!(ids, vec!["alpha".to_string(), "bravo".to_string()]);

    let alpha_instance = alpha.instance().read().unwrap();
    assert_eq!(alpha_instance.name, "alpha");
    drop(alpha_instance);

    let bravo_instance = bravo.instance().read().unwrap();
    assert_eq!(bravo_instance.name, "bravo");

    Ok(())
}

#[tokio::test]
async fn context_runtime_helpers_use_current_started_machine() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartedWithConfig(
        &ctx,
        FanoutInstance {
            name: "alpha".to_string(),
        },
        fanout_model(),
        configured("alpha"),
    )
    .await?;
    let bravo = StartedWithConfig(
        alpha.context(),
        FanoutInstance {
            name: "bravo".to_string(),
        },
        fanout_model(),
        configured("bravo"),
    )
    .await?;

    assert_eq!(
        GetFromContext(bravo.context(), "count"),
        Some(AttributeValue::Int(0))
    );

    SetFromContext(bravo.context(), "count", 2)?;
    assert_eq!(alpha.Get("count"), Some(AttributeValue::Int(0)));
    assert_eq!(bravo.Get("count"), Some(AttributeValue::Int(2)));

    CallFromContext(bravo.context(), "record").await?;
    assert_eq!(alpha.instance().read().unwrap().name, "alpha");
    assert_eq!(
        bravo.instance().read().unwrap().name,
        "/FanoutMachine/record"
    );

    DispatchFromContext(bravo.context(), Event::new("go")).await?;
    assert_eq!(alpha.state(), "/FanoutMachine/idle");
    assert_eq!(bravo.state(), "/FanoutMachine/done");

    Stop(bravo.context(), &bravo).await?;
    assert_eq!(GetFromContext(bravo.context(), "count"), None);

    let error = SetFromContext(bravo.context(), "count", 3).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "set requires a started HSM"
    ));

    let error = CallFromContext(bravo.context(), "record")
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "operation requires a started HSM"
    ));

    let error = DispatchFromContext(bravo.context(), Event::new("go"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "dispatch requires a started HSM"
    ));

    Ok(())
}

#[tokio::test]
async fn dispatch_to_targets_selected_started_instances() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("alpha"),
    )?;
    let bravo = StartWithConfig(
        alpha.context(),
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )?;

    alpha.start().await?;
    bravo.start().await?;

    DispatchTo(bravo.context(), Event::new("go"), vec!["alpha"]).await?;
    assert_eq!(alpha.state(), "/FanoutMachine/done");
    assert_eq!(bravo.state(), "/FanoutMachine/idle");

    DispatchTo(bravo.context(), Event::new("go"), vec!["br*"]).await?;
    assert_eq!(bravo.state(), "/FanoutMachine/done");

    Ok(())
}

#[tokio::test]
async fn dispatch_all_reaches_all_started_instances() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("alpha"),
    )?;
    let bravo = StartWithConfig(
        alpha.context(),
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )?;
    alpha.start().await?;
    bravo.start().await?;

    let unstarted = StartWithConfig(
        bravo.context(),
        FanoutInstance::default(),
        fanout_model(),
        configured("unstarted"),
    )?;
    let (instances, ok) = InstancesFromContext(bravo.context());
    assert!(ok);
    let ids: Vec<String> = instances.iter().map(|machine| machine.id()).collect();
    assert_eq!(ids, vec!["alpha".to_string(), "bravo".to_string()]);

    DispatchAll(bravo.context(), Event::new("go")).await?;
    assert_eq!(alpha.state(), "/FanoutMachine/done");
    assert_eq!(bravo.state(), "/FanoutMachine/done");
    assert_eq!(unstarted.state(), "");

    Ok(())
}

#[tokio::test]
async fn started_starts_and_registers_machine() -> Result<()> {
    let ctx = Context::new();
    let machine = Started(&ctx, FanoutInstance::default(), fanout_model()).await?;

    assert_eq!(machine.state(), "/FanoutMachine/idle");

    let (current, ok) = FromContext::<FanoutInstance>(machine.context());
    assert!(ok);
    assert_eq!(current.unwrap().state(), "/FanoutMachine/idle");

    Ok(())
}

#[tokio::test]
async fn stop_exits_machine_and_removes_it_from_fanout() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartedWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("alpha"),
    )
    .await?;
    let bravo = StartedWithConfig(
        alpha.context(),
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )
    .await?;

    Stop(bravo.context(), &bravo).await?;
    assert_eq!(bravo.state(), "");
    let (instances, ok) = InstancesFromContext(alpha.context());
    assert!(ok);
    let ids: Vec<String> = instances.iter().map(|machine| machine.id()).collect();
    assert_eq!(ids, vec!["alpha".to_string()]);

    DispatchAll(alpha.context(), Event::new("go")).await?;
    assert_eq!(alpha.state(), "/FanoutMachine/done");
    assert_eq!(bravo.state(), "");

    Ok(())
}

#[tokio::test]
async fn stopping_unstarted_machine_does_not_register_it() -> Result<()> {
    let ctx = Context::new();
    let machine = StartWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )?;

    Stop(machine.context(), &machine).await?;

    let (_, ok) = FromContext::<FanoutInstance>(machine.context());
    assert!(!ok);
    let (instances, ok) = InstancesFromContext(machine.context());
    assert!(!ok);
    assert!(instances.is_empty());

    Ok(())
}

#[tokio::test]
async fn start_rejects_already_started_machine() -> Result<()> {
    let ctx = Context::new();
    let machine = StartedWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )
    .await?;

    let error = machine.start().await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "already started HSM"
    ));

    Ok(())
}

#[tokio::test]
async fn canceled_stop_leaves_machine_started() -> Result<()> {
    let ctx = Context::new();
    let machine = StartedWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )
    .await?;
    let canceled = Context::new();
    canceled.cancel();

    Stop(&canceled, &machine).await?;
    assert_eq!(machine.state(), "/FanoutMachine/idle");

    DispatchTo(machine.context(), Event::new("go"), vec!["bravo"]).await?;
    assert_eq!(machine.state(), "/FanoutMachine/done");

    Ok(())
}

#[tokio::test]
async fn restart_enters_initial_state_and_fanout_can_select_machine() -> Result<()> {
    let ctx = Context::new();
    let machine = StartedWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )
    .await?;

    DispatchTo(machine.context(), Event::new("go"), vec!["bravo"]).await?;
    assert_eq!(machine.state(), "/FanoutMachine/done");

    Restart(machine.context(), &machine).await?;
    assert_eq!(machine.state(), "/FanoutMachine/idle");

    DispatchTo(machine.context(), Event::new("go"), vec!["bravo"]).await?;
    assert_eq!(machine.state(), "/FanoutMachine/done");

    Ok(())
}

#[tokio::test]
async fn restart_requires_started_machine() -> Result<()> {
    let ctx = Context::new();
    let machine = StartedWithConfig(
        &ctx,
        FanoutInstance::default(),
        fanout_model(),
        configured("bravo"),
    )
    .await?;

    Stop(machine.context(), &machine).await?;
    assert_eq!(machine.state(), "");

    let error = Restart(machine.context(), &machine).await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "restart requires a started HSM"
    ));

    Ok(())
}
