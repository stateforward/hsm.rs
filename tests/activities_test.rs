use stateforward_hsm::*;
/**
 * @fileoverview Test activity execution and cancellation
 * Tests long-running activities, concurrent execution, and proper cancellation on state transitions
 */
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

#[derive(Debug)]
pub struct ActivityTestInstance {
    pub activity_starts: Vec<String>,
    pub activity_completions: Vec<String>,
    pub activity_counter: Arc<AtomicI32>,
    pub work_data: i32,
}

impl ActivityTestInstance {
    pub fn new() -> Self {
        Self {
            activity_starts: Vec::new(),
            activity_completions: Vec::new(),
            activity_counter: Arc::new(AtomicI32::new(0)),
            work_data: 0,
        }
    }

    pub fn record_activity_start(&mut self, name: &str) {
        self.activity_starts.push(name.to_string());
        self.activity_counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_activity_completion(&mut self, name: &str) {
        self.activity_completions.push(name.to_string());
        self.activity_counter.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_active_count(&self) -> i32 {
        self.activity_counter.load(Ordering::Relaxed)
    }
}

impl Instance for ActivityTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Activity functions
fn quick_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("quick");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    Box::pin(async move {
        // Simulate very short work
        tokio::time::sleep(Duration::from_millis(10)).await;

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

fn medium_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("medium");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    Box::pin(async move {
        // Simulate medium work with cancellation checks
        for i in 0..10 {
            if ctx_clone.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

fn long_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("long");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    Box::pin(async move {
        // Simulate long work with frequent cancellation checks
        for _i in 0..50 {
            if ctx_clone.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

fn network_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("network");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    Box::pin(async move {
        // Simulate network activity with retries
        for retry in 0..3 {
            if ctx_clone.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Simulate successful connection on last retry
            if retry == 2 && !ctx_clone.is_cancelled() {
                break;
            }
        }

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

fn monitoring_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("monitoring");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    Box::pin(async move {
        // Continuous monitoring loop
        while !ctx_clone.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

fn data_activity(
    ctx: &Context,
    inst: &mut ActivityTestInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.record_activity_start("data_activity");
    let counter = inst.activity_counter.clone();
    let ctx_clone = ctx.clone();

    // Access event data
    let work_amount = if let Some(amount) = event.get_data::<i32>() {
        *amount
    } else {
        10 // default
    };

    Box::pin(async move {
        // Simulate work proportional to data
        for _i in 0..work_amount {
            if ctx_clone.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        counter.fetch_sub(1, Ordering::Relaxed);
    })
}

#[tokio::test]
async fn test_single_activity_execution() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "SingleActivityTest",
        initial!(target!("working")),
        state!(
            "working",
            activity!(quick_activity),
            transition!(on!("done"), target!("finished"))
        ),
        final_state!("finished")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should start in working state and begin activity
    assert_eq!(hsm.state(), "/SingleActivityTest/working");

    // Wait for activity to complete
    tokio::time::sleep(Duration::from_millis(50)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 1);
    assert_eq!(inst.activity_starts[0], "quick");
    assert_eq!(inst.get_active_count(), 0);

    // Transition to finished
    let done_event = Event::new("done");
    hsm.dispatch(&ctx, done_event).await;
    assert_eq!(hsm.state(), "/SingleActivityTest/finished");

    Ok(())
}

#[tokio::test]
async fn test_multiple_concurrent_activities() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    fn multi_activity1(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        quick_activity(ctx, inst, event)
    }

    fn multi_activity2(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        medium_activity(ctx, inst, event)
    }

    fn multi_activity3(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        network_activity(ctx, inst, event)
    }

    let model = define!(
        "MultipleActivityTest",
        initial!(target!("busy")),
        state!(
            "busy",
            activity!(multi_activity1),
            activity!(multi_activity2),
            activity!(multi_activity3),
            transition!(on!("stop"), target!("idle"))
        ),
        state!("idle")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should start in busy state and begin all activities concurrently
    assert_eq!(hsm.state(), "/MultipleActivityTest/busy");

    // Give activities time to start
    tokio::time::sleep(Duration::from_millis(30)).await;

    // All three activities should have started
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 3);
    assert_eq!(inst.get_active_count(), 3);
    drop(instance);

    // Wait longer for activities to complete
    tokio::time::sleep(Duration::from_millis(300)).await;

    // All activities should eventually complete
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.get_active_count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_activity_cancellation_on_state_exit() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    fn long_activity1(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        long_activity(ctx, inst, event)
    }

    fn monitoring_activity1(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        monitoring_activity(ctx, inst, event)
    }

    let model = define!(
        "ActivityCancellationTest",
        initial!(target!("running")),
        state!(
            "running",
            activity!(long_activity1),
            activity!(monitoring_activity1),
            transition!(on!("interrupt"), target!("stopped"))
        ),
        state!("stopped")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should start in running state and begin activities
    assert_eq!(hsm.state(), "/ActivityCancellationTest/running");

    // Give activities time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 2);
    assert_eq!(inst.get_active_count(), 2);
    drop(instance);

    // Interrupt before activities complete
    let interrupt_event = Event::new("interrupt");
    hsm.dispatch(&ctx, interrupt_event).await;
    assert_eq!(hsm.state(), "/ActivityCancellationTest/stopped");

    // Give time for activities to detect cancellation and complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Activities should have been cancelled
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.get_active_count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_activity_with_event_data_access() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "ActivityDataTest",
        initial!(target!("start")),
        state!("start", transition!(on!("work"), target!("processing"))),
        state!(
            "processing",
            activity!(data_activity),
            transition!(on!("done"), target!("finished"))
        ),
        final_state!("finished")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Create event with work data
    let work_event = Event::new("work").with_data(5i32);
    hsm.dispatch(&ctx, work_event).await;
    assert_eq!(hsm.state(), "/ActivityDataTest/processing");

    // Give activity time to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 1);
    assert_eq!(inst.activity_starts[0], "data_activity");
    assert_eq!(inst.get_active_count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_activities_in_hierarchical_states() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    fn parent_monitoring(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.record_activity_start("parent-monitoring");
        let counter = inst.activity_counter.clone();
        let ctx_clone = ctx.clone();

        Box::pin(async move {
            while !ctx_clone.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            counter.fetch_sub(1, Ordering::Relaxed);
        })
    }

    fn child_quick(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        quick_activity(ctx, inst, event)
    }

    fn sibling_medium(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        medium_activity(ctx, inst, event)
    }

    let model = define!(
        "HierarchicalActivityTest",
        initial!(target!("parent")),
        state!(
            "parent",
            activity!(parent_monitoring),
            initial!(target!("child")),
            state!(
                "child",
                activity!(child_quick),
                transition!(on!("switch"), target!("../sibling"))
            ),
            state!("sibling", activity!(sibling_medium)),
            transition!(on!("exit"), target!("../other"))
        ),
        state!("other")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should start in parent/child with both activities running
    assert_eq!(hsm.state(), "/HierarchicalActivityTest/parent/child");

    // Give activities time to start
    tokio::time::sleep(Duration::from_millis(5)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 2);
    // Quick activity might already be done, so check >= 1
    assert!(inst.get_active_count() >= 1);
    drop(instance);

    // Switch to sibling - parent activity continues, child activity stops, sibling starts
    let switch_event = Event::new("switch");
    hsm.dispatch(&ctx, switch_event).await;
    assert_eq!(hsm.state(), "/HierarchicalActivityTest/parent/sibling");

    // Give time for state change activities
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should have 3 activities started (monitoring, quick, medium)
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 3);
    drop(instance);

    // Exit parent completely - all activities should be cancelled
    let exit_event = Event::new("exit");
    hsm.dispatch(&ctx, exit_event).await;
    assert_eq!(hsm.state(), "/HierarchicalActivityTest/other");

    // Give time for all activities to be cancelled
    tokio::time::sleep(Duration::from_millis(100)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.get_active_count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_activity_error_handling_and_cleanup() -> Result<()> {
    let instance = ActivityTestInstance::new();
    let ctx = Context::new();

    // Global flag to control error behavior
    static SHOULD_ERROR: AtomicBool = AtomicBool::new(false);

    fn error_prone_activity(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.record_activity_start("error_prone");
        let counter = inst.activity_counter.clone();
        let ctx_clone = ctx.clone();

        Box::pin(async move {
            // Simulate some work
            tokio::time::sleep(Duration::from_millis(30)).await;

            if SHOULD_ERROR.load(Ordering::Relaxed) && !ctx_clone.is_cancelled() {
                // In real implementation, this might dispatch an error event
                counter.fetch_sub(1, Ordering::Relaxed);
                return;
            }

            if !ctx_clone.is_cancelled() {
                counter.fetch_sub(1, Ordering::Relaxed);
            }
        })
    }

    fn quick_activity2(
        ctx: &Context,
        inst: &mut ActivityTestInstance,
        event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        quick_activity(ctx, inst, event)
    }

    let model = define!(
        "ActivityErrorTest",
        initial!(target!("working")),
        state!(
            "working",
            activity!(error_prone_activity),
            activity!(quick_activity2),
            transition!(on!("retry"), target!(".")),
            transition!(on!("stop"), target!("stopped"))
        ),
        state!("stopped")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // First run - should succeed
    tokio::time::sleep(Duration::from_millis(100)).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 2);
    assert_eq!(inst.get_active_count(), 0);
    drop(instance);

    // Set error flag and retry
    SHOULD_ERROR.store(true, Ordering::Relaxed);
    let retry_event = Event::new("retry");
    hsm.dispatch(&ctx, retry_event).await;

    // Give time for activities to run and potentially fail
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should have more activity starts
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ActivityTestInstance>()
        .unwrap();
    assert_eq!(inst.activity_starts.len(), 4); // 2 more starts

    Ok(())
}
