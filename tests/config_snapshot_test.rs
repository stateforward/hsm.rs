use std::any::Any;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust::*;

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
    hsm.Set("count", 7);

    let snapshot = TakeSnapshot(&ctx, &hsm);
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

    hsm.Set("count", 8);
    assert_eq!(
        snapshot.Attributes.get("/RustSnapshotMachine/count"),
        Some(&AttributeValue::Int(7)),
    );
    assert_eq!(
        hsm.TakeSnapshot()
            .Attributes
            .get("/RustSnapshotMachine/count"),
        Some(&AttributeValue::Int(8)),
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
    assert_eq!(hsm.TakeSnapshot().QueueLen, 0);

    Ok(())
}
