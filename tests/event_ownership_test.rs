use stateforward_hsm::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct OwnershipInstance {
    seen: Vec<(String, String, kind::KindValue)>,
}

impl Instance for OwnershipInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn record_event(
    _ctx: &Context,
    inst: &mut OwnershipInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let payload = event
        .get_data::<Arc<Mutex<Vec<String>>>>()
        .expect("shared payload");
    payload.lock().unwrap().push("effect".to_string());
    inst.seen
        .push((event.name.clone(), event.qualified_name.clone(), event.kind));
    Box::pin(async move {})
}

#[tokio::test]
async fn dispatch_event_metadata_is_owned_and_payload_is_application_shared() -> Result<()> {
    let ctx = Context::new();
    let payload = Arc::new(Mutex::new(Vec::<String>::new()));
    let event = Event::new("go").with_data(payload.clone());
    let caller_event = event.clone();
    let model = define!(
        "RustEventOwnership",
        initial!(target!("idle")),
        state!(
            "idle",
            transition!(on!("go"), target!("../done"), effect!(record_event))
        ),
        state!("done")
    );

    let hsm = start(&ctx, OwnershipInstance::default(), model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, event).await?;

    assert_eq!(caller_event.name, "go");
    assert_eq!(caller_event.qualified_name, "go");
    assert_eq!(caller_event.kind, kind::EVENT);
    assert_eq!(payload.lock().unwrap().as_slice(), ["effect"]);

    let instance = hsm.instance().read().unwrap();
    let instance = instance
        .as_any()
        .downcast_ref::<OwnershipInstance>()
        .unwrap();
    assert_eq!(
        instance.seen,
        vec![("go".to_string(), "go".to_string(), kind::EVENT)]
    );

    Ok(())
}
