use std::any::Any;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use stateforward_hsm::*;

#[derive(Debug, Default)]
struct ConfigInstance {
    started_data: Option<String>,
}

impl Instance for ConfigInstance {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn capture_start_data(
    _ctx: &Context,
    instance: &mut ConfigInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    if let Some(value) = event.get_data::<String>() {
        instance.started_data = Some(value.clone());
    }
    Box::pin(async {})
}

fn mark_go_handled(
    _ctx: &Context,
    instance: &mut ConfigInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    instance.started_data = Some("handled".to_string());
    Box::pin(async {})
}

fn capture_entry_snapshot_state(
    ctx: &Context,
    instance: &mut ConfigInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let (machine, ok) = FromContext::<ConfigInstance>(ctx);
    if ok {
        if let Some(machine) = machine {
            if let Ok(snapshot) = machine.TakeSnapshot() {
                instance.started_data = Some(snapshot.State);
            }
        }
    }
    Box::pin(async {})
}

#[tokio::test]
async fn config_applies_runtime_identity_and_initial_data_without_mutating_model() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustConfigMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(capture_start_data)]),
        ],
    );

    let mut config = Config();
    config.ID = Some("rust-alpha".to_string());
    config.Name = Some("/RuntimeRustAlpha".to_string());
    config.Data = Some(Arc::new("boot-data".to_string()));

    let hsm = StartWithConfig(&ctx, ConfigInstance::default(), model, config)?;
    hsm.start().await?;

    assert_eq!(hsm.ID(), "rust-alpha");
    assert_eq!(ID(&hsm), "rust-alpha");
    assert_eq!(hsm.Name(), "/RuntimeRustAlpha");
    assert_eq!(QualifiedName(&hsm), "/RuntimeRustAlpha");
    assert_eq!(hsm.state(), "/RustConfigMachine/idle");

    let instance = hsm.instance().read().unwrap();
    assert_eq!(instance.started_data.as_deref(), Some("boot-data"));

    Ok(())
}

#[tokio::test]
async fn start_and_restart_accept_explicit_runtime_data() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustRuntimeDataMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(capture_start_data)]),
        ],
    );

    let hsm = start(&ctx, ConfigInstance::default(), model)?;
    StartWithData(&hsm, "boot".to_string()).await?;
    assert_eq!(hsm.state(), "/RustRuntimeDataMachine/idle");
    assert_eq!(
        hsm.instance().read().unwrap().started_data.as_deref(),
        Some("boot"),
    );

    RestartWithData(&ctx, &hsm, "again".to_string()).await?;
    assert_eq!(hsm.state(), "/RustRuntimeDataMachine/idle");
    assert_eq!(
        hsm.instance().read().unwrap().started_data.as_deref(),
        Some("again"),
    );

    Stop(&ctx, &hsm).await?;
    hsm.StartWithData("manual".to_string()).await?;
    assert_eq!(hsm.state(), "/RustRuntimeDataMachine/idle");
    assert_eq!(
        hsm.instance().read().unwrap().started_data.as_deref(),
        Some("manual"),
    );

    let model: Model<ConfigInstance> = Define(
        "RustStartedRuntimeDataMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(capture_start_data)]),
        ],
    );
    let started = StartedWithData(
        &ctx,
        ConfigInstance::default(),
        model,
        "created".to_string(),
    )
    .await?;
    assert_eq!(started.state(), "/RustStartedRuntimeDataMachine/idle");
    assert_eq!(
        started.instance().read().unwrap().started_data.as_deref(),
        Some("created"),
    );

    Ok(())
}

#[tokio::test]
async fn package_constructors_bind_models_without_starting() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustNewMachine",
        vec![Initial(vec![Target("idle")]), State("idle", vec![])],
    );

    let hsm = New(ConfigInstance::default(), model);
    assert_eq!(hsm.state(), "");

    let error = take_snapshot(&ctx, &hsm).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "take snapshot requires a started HSM"
    ));

    hsm.start().await?;
    assert_eq!(hsm.state(), "/RustNewMachine/idle");

    let model: Model<ConfigInstance> = Define(
        "RustNewConfiguredMachine",
        vec![Initial(vec![Target("idle")]), State("idle", vec![])],
    );
    let mut config = Config();
    config.ID = Some("new-with-config-id".to_string());
    config.Name = Some("/RuntimeNewWithConfig".to_string());

    let hsm = new_with_config(ConfigInstance::default(), model, config);
    assert_eq!(hsm.ID(), "new-with-config-id");
    assert_eq!(hsm.Name(), "/RuntimeNewWithConfig");
    assert_eq!(hsm.state(), "");

    Ok(())
}

#[tokio::test]
async fn take_snapshot_uses_runtime_identity_and_copies_attributes() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustSnapshotMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    let mut config = Config();
    config.ID = Some("snapshot-id".to_string());
    config.Name = Some("/RuntimeSnapshot".to_string());

    let hsm = start_with_config(&ctx, ConfigInstance::default(), model, config)?;
    hsm.start().await?;
    hsm.Set("count", 7)?;

    let snapshot = take_snapshot(&ctx, &hsm)?;
    assert_eq!(snapshot.ID, "snapshot-id");
    assert_eq!(snapshot.QualifiedName, "/RuntimeSnapshot");
    assert_eq!(snapshot.State, "/RustSnapshotMachine/idle");
    assert_eq!(
        snapshot.Attributes.get("/RustSnapshotMachine/count"),
        Some(&AttributeValue::Int(7)),
    );
    assert_eq!(snapshot.QueueLen, 0);
    assert_eq!(snapshot.Events.len(), 1);
    assert_eq!(snapshot.Events[0].Name, "go");
    assert_eq!(
        snapshot.Events[0].Target.as_deref(),
        Some("/RustSnapshotMachine/done"),
    );

    hsm.Set("count", 8)?;
    assert_eq!(
        snapshot.Attributes.get("/RustSnapshotMachine/count"),
        Some(&AttributeValue::Int(7)),
    );
    assert_eq!(
        hsm.TakeSnapshot()?
            .Attributes
            .get("/RustSnapshotMachine/count"),
        Some(&AttributeValue::Int(8)),
    );

    Ok(())
}

#[tokio::test]
async fn take_snapshot_requires_started_machine() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "InactiveSnapshotMachine",
        vec![
            Attribute("count", 0),
            Initial(vec![Target("idle")]),
            State("idle", vec![]),
        ],
    );

    let hsm = start(&ctx, ConfigInstance::default(), model)?;

    let error = TakeSnapshot(&ctx, &hsm).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "take snapshot requires a started HSM"
    ));

    hsm.start().await?;
    Stop(&ctx, &hsm).await?;

    let error = hsm.TakeSnapshot().unwrap_err();
    assert!(matches!(
        error,
        HsmError::Runtime(message) if message == "take snapshot requires a started HSM"
    ));

    Ok(())
}

#[tokio::test]
async fn behavior_snapshot_during_initial_entry_observes_root_state() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "EntrySnapshotMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Entry(capture_entry_snapshot_state)]),
        ],
    );

    let hsm = start(&ctx, ConfigInstance::default(), model)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/EntrySnapshotMachine/idle");
    assert_eq!(
        hsm.instance().read().unwrap().started_data.as_deref(),
        Some("/EntrySnapshotMachine"),
    );

    Ok(())
}

#[tokio::test]
async fn config_accepts_clock_and_custom_regular_queue() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustRuntimeHooksMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    let slept = Arc::new(Mutex::new(Vec::new()));
    let sleep_log = slept.clone();
    let clock = Clock(Some(Arc::new(move |duration| {
        let sleep_log = sleep_log.clone();
        Box::pin(async move {
            sleep_log.lock().unwrap().push(duration);
        })
    })));

    let events = Arc::new(Mutex::new(VecDeque::new()));
    let pushed = Arc::new(Mutex::new(Vec::new()));
    let push_events = events.clone();
    let push_log = pushed.clone();
    let pop_events = events.clone();
    let len_events = events.clone();
    let queue = Queue(
        Arc::new(move |_ctx, event| {
            push_log.lock().unwrap().push(event.name.clone());
            push_events.lock().unwrap().push_back(event);
            Ok(())
        }),
        Arc::new(move |_ctx| Ok(pop_events.lock().unwrap().pop_front())),
        Arc::new(move |_ctx| Ok(len_events.lock().unwrap().len())),
    );

    let mut config = Config();
    config.Clock = Some(clock);
    config.Queue = Some(queue);

    let hsm = StartWithConfig(&ctx, ConfigInstance::default(), model, config)?;
    hsm.Clock().Sleep(Duration::from_millis(3)).await;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("go")).await?;

    assert_eq!(hsm.state(), "/RustRuntimeHooksMachine/done");
    assert_eq!(*slept.lock().unwrap(), vec![Duration::from_millis(3)]);
    assert_eq!(*pushed.lock().unwrap(), vec!["go"]);
    assert_eq!(hsm.TakeSnapshot()?.QueueLen, 0);

    Ok(())
}

#[tokio::test]
async fn stop_clears_queued_custom_regular_events_before_restart() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ConfigInstance> = Define(
        "RustStopClearsQueuedEventMachine",
        vec![
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![Transition(vec![On("go"), Effect(mark_go_handled)])],
            ),
        ],
    );

    let events = Arc::new(Mutex::new(VecDeque::new()));
    let allow_pop = Arc::new(Mutex::new(false));
    let push_events = events.clone();
    let pop_events = events.clone();
    let pop_allowed = allow_pop.clone();
    let len_events = events.clone();
    let queue = Queue(
        Arc::new(move |_ctx, event| {
            push_events.lock().unwrap().push_back(event);
            Ok(())
        }),
        Arc::new(move |_ctx| {
            if !*pop_allowed.lock().unwrap() {
                return Ok(None);
            }
            Ok(pop_events.lock().unwrap().pop_front())
        }),
        Arc::new(move |_ctx| Ok(len_events.lock().unwrap().len())),
    );

    let mut config = Config();
    config.Queue = Some(queue);

    let hsm = StartWithConfig(&ctx, ConfigInstance::default(), model, config)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("go")).await?;

    assert_eq!(hsm.state(), "/RustStopClearsQueuedEventMachine/idle");
    assert_eq!(hsm.TakeSnapshot()?.QueueLen, 1);

    *allow_pop.lock().unwrap() = true;
    Stop(&ctx, &hsm).await?;
    assert_eq!(events.lock().unwrap().len(), 0);

    hsm.start().await?;
    assert_eq!(hsm.state(), "/RustStopClearsQueuedEventMachine/idle");
    assert!(hsm.instance().read().unwrap().started_data.is_none());

    Ok(())
}
