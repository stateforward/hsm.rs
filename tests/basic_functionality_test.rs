use rust::*;
/**
 * @fileoverview Test basic state transitions and state machine lifecycle
 * Tests fundamental HSM features including state transitions, lifecycle methods, and basic event handling
 */
use std::future::Future;
use std::pin::Pin;

// Test instance implementation following the reference pattern
#[derive(Debug)]
pub struct BasicTestInstance {
    pub log: Vec<String>,
    pub data: std::collections::HashMap<String, String>,
    pub counter: i32,
}

impl BasicTestInstance {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            data: std::collections::HashMap::new(),
            counter: 0,
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.log.push(action.to_string());
    }

    pub fn increment(&mut self) {
        self.counter += 1;
    }
}

impl Instance for BasicTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Basic behavior functions following exact signatures from reference
fn basic_entry(
    _ctx: &Context,
    inst: &mut BasicTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("basic-entry");
    inst.increment();
    Box::pin(async move {})
}

fn basic_exit(
    _ctx: &Context,
    inst: &mut BasicTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("basic-exit");
    Box::pin(async move {})
}

fn transition_effect(
    _ctx: &Context,
    inst: &mut BasicTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("transition-effect");
    Box::pin(async move {})
}

fn counter_effect(
    _ctx: &Context,
    inst: &mut BasicTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.increment();
    inst.log_action(&format!("counter-effect-{}", inst.counter));
    Box::pin(async move {})
}

#[tokio::test]
async fn test_basic_state_machine_with_simple_transitions() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "BasicMachine",
        initial!(target!("idle")),
        state!("idle", transition!(on!("start"), target!("../running"))),
        state!("running", transition!(on!("stop"), target!("../idle")))
    );

    // Start the state machine
    let hsm = start(&ctx, instance, model)?;

    hsm.start().await;

    // Should start in idle state
    assert_eq!(hsm.state(), "/BasicMachine/idle");

    // Dispatch start event
    let start_event = Event::new("start");
    hsm.dispatch(&ctx, start_event).await;
    assert_eq!(hsm.state(), "/BasicMachine/running");

    // Dispatch stop event
    let stop_event = Event::new("stop");
    hsm.dispatch(&ctx, stop_event).await;
    assert_eq!(hsm.state(), "/BasicMachine/idle");

    Ok(())
}

#[tokio::test]
async fn test_state_machine_lifecycle_with_entry_exit() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "LifecycleMachine",
        initial!(target!("active")),
        state!("active", entry!(basic_entry), exit!(basic_exit))
    );

    // Start the state machine
    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should have executed entry action
    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["basic-entry"]);
    assert_eq!(inst.counter, 1);
    assert_eq!(hsm.state(), "/LifecycleMachine/active");
    drop(inst_ref);

    // Stop the state machine would trigger exit (not directly testable in current API)
    Ok(())
}

#[tokio::test]
async fn test_multiple_transitions_from_same_state() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "MultiTransitionMachine",
        initial!(target!("idle")),
        state!(
            "idle",
            transition!(on!("event1"), target!("../state1")),
            transition!(on!("event2"), target!("../state2"))
        ),
        state!("state1", transition!(on!("back"), target!("../idle"))),
        state!("state2", transition!(on!("back"), target!("../idle")))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Test first transition
    let event1 = Event::new("event1");
    hsm.dispatch(&ctx, event1).await;
    assert_eq!(hsm.state(), "/MultiTransitionMachine/state1");

    // Go back to idle
    let back_event = Event::new("back");
    hsm.dispatch(&ctx, back_event).await;
    assert_eq!(hsm.state(), "/MultiTransitionMachine/idle");

    // Test second transition
    let event2 = Event::new("event2");
    hsm.dispatch(&ctx, event2).await;
    assert_eq!(hsm.state(), "/MultiTransitionMachine/state2");

    Ok(())
}

#[tokio::test]
async fn test_self_transitions() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "SelfTransitionMachine",
        initial!(target!("counter")),
        state!(
            "counter",
            entry!(basic_entry),
            exit!(basic_exit),
            transition!(on!("increment"), target!("."), effect!(counter_effect))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.counter, 1);
    assert_eq!(inst.log, vec!["basic-entry"]);
    drop(inst_ref);

    // Self transition should exit and re-enter the state
    let increment_event = Event::new("increment");
    hsm.dispatch(&ctx, increment_event).await;

    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.counter, 3); // entry(1) + effect(1) + re-entry(1)
    assert_eq!(
        inst.log,
        vec![
            "basic-entry",
            "basic-exit",
            "counter-effect-2",
            "basic-entry"
        ]
    );

    Ok(())
}

#[tokio::test]
async fn test_internal_transitions() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "InternalTransitionMachine",
        initial!(target!("active")),
        state!(
            "active",
            entry!(basic_entry),
            exit!(basic_exit),
            transition!(on!("internal"), effect!(transition_effect))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.counter, 1);
    assert_eq!(inst.log, vec!["basic-entry"]);
    drop(inst_ref);

    // Internal transition should NOT exit/enter the state
    let internal_event = Event::new("internal");
    hsm.dispatch(&ctx, internal_event).await;

    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.counter, 1); // Should not re-enter
    assert_eq!(inst.log, vec!["basic-entry", "transition-effect"]);

    Ok(())
}

#[tokio::test]
async fn test_unknown_events_ignored() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "UnknownEventMachine",
        initial!(target!("stable")),
        state!("stable", entry!(basic_entry))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;
    assert_eq!(hsm.state(), "/UnknownEventMachine/stable");

    // Dispatch unknown events
    let unknown1 = Event::new("unknown1");
    hsm.dispatch(&ctx, unknown1).await;
    let unknown2 = Event::new("unknown2");
    hsm.dispatch(&ctx, unknown2).await;
    let unknown3 = Event::new("unknown3");
    hsm.dispatch(&ctx, unknown3).await;

    // State should remain unchanged
    assert_eq!(hsm.state(), "/UnknownEventMachine/stable");

    // No additional actions should have been triggered
    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["basic-entry"]);

    Ok(())
}

#[tokio::test]
async fn test_event_with_data() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    fn data_effect(
        _ctx: &Context,
        inst: &mut BasicTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action(&format!("effect-{}", &event.name));
        if let Some(data) = event.get_data::<i32>() {
            inst.log_action(&format!("data-{}", data));
        }
        Box::pin(async move {})
    }

    let model = define!(
        "EventTypeMachine",
        initial!(target!("waiting")),
        state!(
            "waiting",
            transition!(on!("proceed"), target!("../done"), effect!(data_effect))
        ),
        state!("done")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Test event with data
    let event_with_data = Event::new("proceed").with_data(42i32);
    hsm.dispatch(&ctx, event_with_data).await;

    assert_eq!(hsm.state(), "/EventTypeMachine/done");

    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["effect-proceed", "data-42"]);

    Ok(())
}

#[tokio::test]
async fn test_final_states() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "FinalMachine",
        initial!(target!("working")),
        state!(
            "working",
            transition!(on!("complete"), target!("../completed"))
        ),
        final_state!("completed")
    );

    validate(&model)?;
    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    let complete_event = Event::new("complete");
    hsm.dispatch(&ctx, complete_event).await;
    assert_eq!(hsm.state(), "/FinalMachine/completed");

    Ok(())
}

#[tokio::test]
async fn test_context_cancellation() -> Result<()> {
    let instance = BasicTestInstance::new();
    let ctx = Context::new();

    fn cancellation_activity(
        _ctx: &Context,
        inst: &mut BasicTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("activity-started");
        Box::pin(async move {
            // Simulate some async work
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        })
    }

    let model = define!(
        "CancellationMachine",
        initial!(target!("active")),
        state!(
            "active",
            activity!(cancellation_activity),
            transition!(on!("stop"), target!("../stopped"))
        ),
        state!("stopped")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should start activity
    let inst_ref = hsm.instance().read().unwrap();
    let inst = inst_ref
        .as_any()
        .downcast_ref::<BasicTestInstance>()
        .unwrap();
    assert!(inst.log.contains(&"activity-started".to_string()));
    drop(inst_ref);

    // Test cancellation
    assert!(!ctx.is_cancelled());
    ctx.cancel();
    assert!(ctx.is_cancelled());

    Ok(())
}

#[tokio::test]
async fn test_synchronous_guards() -> Result<()> {
    let mut instance = BasicTestInstance::new();
    instance.counter = 5;
    let ctx = Context::new();

    fn counter_guard(_ctx: &Context, inst: &BasicTestInstance, _event: &Event) -> bool {
        inst.counter > 3
    }

    let model = define!(
        "GuardMachine",
        initial!(target!("checking")),
        state!(
            "checking",
            transition!(on!("test"), guard!(counter_guard), target!("../passed")),
            transition!(on!("test"), target!("../failed"))
        ),
        state!("passed"),
        state!("failed")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    let test_event = Event::new("test");
    hsm.dispatch(&ctx, test_event).await;
    assert_eq!(hsm.state(), "/GuardMachine/passed");

    Ok(())
}
