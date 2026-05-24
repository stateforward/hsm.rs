use rust::*;
/**
 * @fileoverview Test entry and exit function execution
 * Tests single and multiple entry/exit actions, execution order, and behavior during transitions
 */
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
pub struct EntryExitTestInstance {
    pub log: Vec<String>,
    pub counter: i32,
    pub data: std::collections::HashMap<String, String>,
}

impl EntryExitTestInstance {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            counter: 0,
            data: std::collections::HashMap::new(),
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.log.push(action.to_string());
    }

    pub fn increment(&mut self) {
        self.counter += 1;
    }

    pub fn set_data(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
}

impl Instance for EntryExitTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Single entry/exit functions
fn single_entry(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.increment();
    inst.log_action(&format!("single-entry-{}", inst.counter));
    Box::pin(async move {})
}

fn single_exit(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action(&format!("single-exit-{}", inst.counter));
    inst.counter = 0;
    Box::pin(async move {})
}

// Multiple entry functions
fn setup_state(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.set_data("status", "initializing");
    inst.log_action("setup-state");
    Box::pin(async move {})
}

fn log_entry(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.increment();
    inst.log_action("log-entry");
    Box::pin(async move {})
}

fn initialize_counters(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.increment();
    inst.log_action("initialize-counters");
    Box::pin(async move {})
}

fn finalize_setup(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.set_data("status", "ready");
    inst.log_action("finalize-setup");
    Box::pin(async move {})
}

// Multiple exit functions
fn save_data(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.set_data("saved", "true");
    inst.log_action("save-data");
    Box::pin(async move {})
}

fn cleanup_resources(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.set_data("cleaned", "true");
    inst.log_action("cleanup-resources");
    Box::pin(async move {})
}

fn log_exit(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action(&format!("log-exit-{}", inst.counter));
    Box::pin(async move {})
}

fn reset_state(
    _ctx: &Context,
    inst: &mut EntryExitTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.counter = 0;
    inst.set_data("status", "idle");
    inst.log_action("reset-state");
    Box::pin(async move {})
}

#[tokio::test]
async fn test_single_entry_exit_functions() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "SingleEntryExitMachine",
        initial!(target!("active")),
        state!(
            "active",
            entry!(single_entry),
            exit!(single_exit),
            transition!(on!("next"), target!("../finished"))
        ),
        state!("finished")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should execute entry action
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["single-entry-1"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    // Transition should execute exit action
    let next_event = Event::new("next");
    hsm.dispatch(&ctx, next_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["single-entry-1", "single-exit-1"]);
    assert_eq!(inst.counter, 0); // Reset by exit action
    assert_eq!(hsm.state(), "/SingleEntryExitMachine/finished");

    Ok(())
}

#[tokio::test]
async fn test_multiple_entry_functions() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    // Note: Multiple functions would need to be implemented in the actual API
    // For now, we'll test the concept with sequential calls
    let model = define!(
        "MultipleEntryMachine",
        initial!(target!("configuring")),
        state!(
            "configuring",
            entry!(setup_state),
            transition!(on!("configure"), target!("../ready"))
        ),
        state!(
            "ready",
            entry!(log_entry),
            transition!(on!("next"), target!("../processing"))
        ),
        state!(
            "processing",
            entry!(initialize_counters),
            transition!(on!("done"), target!("../finished"))
        ),
        state!("finished", entry!(finalize_setup))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should execute first entry action
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["setup-state"]);
    assert_eq!(inst.data.get("status"), Some(&"initializing".to_string()));
    drop(instance);

    // Transition through states to see multiple entry actions
    let configure_event = Event::new("configure");
    hsm.dispatch(&ctx, configure_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["setup-state", "log-entry"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    let next_event = Event::new("next");
    hsm.dispatch(&ctx, next_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec!["setup-state", "log-entry", "initialize-counters"]
    );
    assert_eq!(inst.counter, 2);
    drop(instance);

    let done_event = Event::new("done");
    hsm.dispatch(&ctx, done_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "setup-state",
            "log-entry",
            "initialize-counters",
            "finalize-setup"
        ]
    );
    assert_eq!(inst.data.get("status"), Some(&"ready".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_multiple_exit_functions() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    // Test multiple exit actions in sequence
    let model = define!(
        "MultipleExitMachine",
        initial!(target!("working")),
        state!(
            "working",
            entry!(setup_state),
            exit!(save_data),
            transition!(on!("next"), target!("../cleanup"))
        ),
        state!(
            "cleanup",
            entry!(log_entry),
            exit!(cleanup_resources),
            transition!(on!("next"), target!("../logging"))
        ),
        state!(
            "logging",
            entry!(initialize_counters),
            exit!(log_exit),
            transition!(on!("next"), target!("../finished"))
        ),
        state!("finished", entry!(reset_state))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Go through transitions to trigger multiple exit actions
    let next1_event = Event::new("next");
    hsm.dispatch(&ctx, next1_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.data.get("saved"), Some(&"true".to_string()));
    assert!(inst.log.contains(&"save-data".to_string()));
    assert!(inst.log.contains(&"log-entry".to_string()));
    drop(instance);

    let next2_event = Event::new("next");
    hsm.dispatch(&ctx, next2_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.data.get("cleaned"), Some(&"true".to_string()));
    assert!(inst.log.contains(&"cleanup-resources".to_string()));
    assert!(inst.log.contains(&"initialize-counters".to_string()));
    drop(instance);

    let next3_event = Event::new("next");
    hsm.dispatch(&ctx, next3_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert!(inst.log.contains(&"log-exit-2".to_string()));
    assert!(inst.log.contains(&"reset-state".to_string()));
    assert_eq!(inst.counter, 0); // Reset by final entry action
    assert_eq!(inst.data.get("status"), Some(&"idle".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_entry_exit_with_event_data() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    fn data_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        if let Some(message) = event.get_data::<String>() {
            inst.log_action(&format!("entry-{}", message));
            inst.set_data("entry_data", message);
        } else {
            inst.log_action("entry-no-data");
        }
        Box::pin(async move {})
    }

    fn data_exit(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        if let Some(message) = event.get_data::<String>() {
            inst.log_action(&format!("exit-{}", message));
            inst.set_data("exit_data", message);
        } else {
            inst.log_action("exit-no-data");
        }
        Box::pin(async move {})
    }

    let model = define!(
        "EventDataMachine",
        initial!(target!("active")),
        state!(
            "active",
            entry!(data_entry),
            exit!(data_exit),
            transition!(on!("transition"), target!("../finished"))
        ),
        state!("finished", entry!(data_entry))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Check initial entry (with no event data)
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert!(inst.log.contains(&"entry-no-data".to_string()));
    drop(instance);

    // Transition with different event data
    let transition_event = Event::new("transition").with_data("finishing".to_string());
    hsm.dispatch(&ctx, transition_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert!(inst.log.contains(&"exit-finishing".to_string()));
    assert!(inst.log.contains(&"entry-finishing".to_string()));
    assert_eq!(inst.data.get("exit_data"), Some(&"finishing".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_hierarchical_entry_exit_order() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    fn parent_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("parent-entry");
        Box::pin(async move {})
    }

    fn parent_exit(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("parent-exit");
        Box::pin(async move {})
    }

    fn child_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("child-entry");
        Box::pin(async move {})
    }

    fn child_exit(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("child-exit");
        Box::pin(async move {})
    }

    fn grandchild_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("grandchild-entry");
        Box::pin(async move {})
    }

    fn grandchild_exit(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("grandchild-exit");
        Box::pin(async move {})
    }

    fn other_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("other-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "HierarchicalEntryExitMachine",
        initial!(target!("parent")),
        state!(
            "parent",
            initial!(target!("child")),
            entry!(parent_entry),
            exit!(parent_exit),
            state!(
                "child",
                initial!(target!("grandchild")),
                entry!(child_entry),
                exit!(child_exit),
                state!(
                    "grandchild",
                    entry!(grandchild_entry),
                    exit!(grandchild_exit),
                    transition!(on!("exit"), target!("../../../other"))
                )
            )
        ),
        state!("other", entry!(other_entry))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should enter in hierarchical order: parent -> child -> grandchild
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec!["parent-entry", "child-entry", "grandchild-entry"]
    );
    drop(instance);

    // Exit should be in reverse order: grandchild -> child -> parent
    let exit_event = Event::new("exit");
    hsm.dispatch(&ctx, exit_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "parent-entry",
            "child-entry",
            "grandchild-entry",
            "grandchild-exit",
            "child-exit",
            "parent-exit",
            "other-entry"
        ]
    );

    Ok(())
}

#[tokio::test]
async fn test_entry_exit_with_self_transitions() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "SelfTransitionEntryExitMachine",
        initial!(target!("counter")),
        state!(
            "counter",
            entry!(single_entry),
            exit!(single_exit),
            transition!(on!("self"), target!("."))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["single-entry-1"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    // Self transition should exit and re-enter
    let self_event = Event::new("self");
    hsm.dispatch(&ctx, self_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec!["single-entry-1", "single-exit-1", "single-entry-1"]
    );
    assert_eq!(inst.counter, 1); // Reset to 0 by exit, then incremented to 1 by entry

    Ok(())
}

#[tokio::test]
async fn test_entry_exit_error_handling() -> Result<()> {
    let instance = EntryExitTestInstance::new();
    let ctx = Context::new();

    fn fallible_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("fallible-entry");
        Box::pin(async move {
            // In a real implementation, this might handle errors
            // For this test, we'll just log the attempt
        })
    }

    fn cleanup_entry(
        _ctx: &Context,
        inst: &mut EntryExitTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("cleanup-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "ErrorHandlingMachine",
        initial!(target!("working")),
        state!(
            "working",
            entry!(fallible_entry),
            exit!(save_data),
            transition!(on!("error"), target!("../error"))
        ),
        state!(
            "error",
            entry!(cleanup_entry),
            transition!(on!("recover"), target!("../working"))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["fallible-entry"]);
    drop(instance);

    // Simulate error condition
    let error_event = Event::new("error");
    hsm.dispatch(&ctx, error_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<EntryExitTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec!["fallible-entry", "save-data", "cleanup-entry"]
    );
    assert_eq!(inst.data.get("saved"), Some(&"true".to_string()));

    Ok(())
}
