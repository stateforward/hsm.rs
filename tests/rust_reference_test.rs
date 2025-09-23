/**
 * @fileoverview Tests following the HSM Framework Rust Reference
 * Tests macro-based API, proper function signatures, and Rust-specific patterns
 */

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use rust::*;

// Test instance following the reference pattern
#[derive(Debug)]
pub struct MyInstance {
    pub counter: i32,
    pub status: String,
    pub history: Vec<String>,
}

impl MyInstance {
    pub fn new() -> Self {
        Self {
            counter: 0,
            status: "idle".to_string(),
            history: Vec::new(),
        }
    }
    
    pub fn log(&mut self, message: &str) {
        self.history.push(message.to_string());
    }
    
    pub fn increment(&mut self) {
        self.counter += 1;
    }
}

impl Instance for MyInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Behavior functions following exact signatures from reference
fn state_entry(ctx: &Context, inst: &mut MyInstance, event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.counter += 1;
    inst.log(&format!("Entering state, counter: {}", inst.counter));
    Box::pin(async move {})
}

fn state_exit(ctx: &Context, inst: &mut MyInstance, event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log(&format!("Exiting state, final counter: {}", inst.counter));
    inst.counter = 0;
    Box::pin(async move {})
}

fn transition_effect(ctx: &Context, inst: &mut MyInstance, event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.status = "transitioning".to_string();
    inst.log("Transition effect executed");
    Box::pin(async move {})
}

fn counter_guard(ctx: &Context, inst: &MyInstance, event: &Event) -> bool {
    inst.counter > 5
}

fn status_guard(ctx: &Context, inst: &MyInstance, event: &Event) -> bool {
    inst.status == "ready"
}

fn background_activity(ctx: &Context, inst: &mut MyInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log("Activity started");
    let ctx_clone = ctx.clone(); // Clone context to avoid lifetime issues
    Box::pin(async move {
        let mut iterations = 0;
        while !ctx_clone.is_cancelled() && iterations < 10 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            iterations += 1;
        }
        // Note: Can't modify inst here due to move semantics
        // In real implementation, would need to structure differently
    })
}

fn short_delay(ctx: &Context, inst: &MyInstance, event: &Event) -> Duration {
    Duration::from_millis(500)
}

fn periodic_interval(ctx: &Context, inst: &MyInstance, event: &Event) -> Duration {
    Duration::from_secs(2)
}

#[tokio::test]
async fn test_macro_based_definition() -> Result<()> {
    let mut instance = MyInstance::new();
    let ctx = Context::new();

    // Test that macros compile and work
    let model = define!("MacroMachine",
        initial!(target!("idle")),
        state!("idle",
            entry!(state_entry),
            transition!(on!("start"), target!("../active"))
        ),
        state!("active", 
            exit!(state_exit),
            transition!(on!("stop"), target!("../idle"))
        )
    );

    // Should compile without errors
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_context_patterns() -> Result<()> {
    let instance = MyInstance::new();
    
    // Test context creation patterns
    let ctx1 = Context::new();
    assert!(!ctx1.is_cancelled());
    
    let ctx2 = Context::with_timeout(Duration::from_millis(100));
    assert!(!ctx2.is_cancelled());
    
    // Test cancellation
    ctx1.cancel();
    assert!(ctx1.is_cancelled());
    assert!(ctx1.is_done()); // Legacy compatibility
    
    Ok(())
}

#[tokio::test]
async fn test_event_with_typed_data() -> Result<()> {
    let _instance = MyInstance::new();
    let _ctx = Context::new();

    // Test event creation and data access following the reference
    let counter_event = Event::new("counter_event").with_data(42i32);
    let message_event = Event::new("message_event").with_data("hello".to_string());
    let flag_event = Event::new("flag_event").with_data(true);
    let empty_event = Event::new("empty_event");

    // Test typed data access
    assert_eq!(counter_event.get_data::<i32>(), Some(&42));
    assert_eq!(message_event.get_data::<String>(), Some(&"hello".to_string()));
    assert_eq!(flag_event.get_data::<bool>(), Some(&true));
    assert_eq!(empty_event.get_data::<i32>(), None);
    
    // Test wrong type returns None
    assert_eq!(counter_event.get_data::<String>(), None);

    Ok(())
}

#[tokio::test]
async fn test_path_resolution_patterns() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test that path patterns compile
    let model = define!("PathMachine",
        initial!(target!("parent/child1")),
        state!("parent",
            state!("child1",
                transition!(on!("next"), target!("../child2"))
            ),
            state!("child2",
                transition!(on!("reset"), target!("../child1")),
                transition!(on!("up"), target!("../../other"))
            )
        ),
        state!("other",
            transition!(on!("absolute"), target!("/PathMachine/parent/child1"))
        )
    );

    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_choice_with_guards() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test choice state with guards and guardless fallback
    let model = define!("ChoiceMachine",
        initial!(target!("deciding")),
        state!("deciding",
            transition!(on!("decide"), target!("../decision"))
        ),
        choice!("decision",
            transition!(guard!(counter_guard), target!("high")),
            transition!(guard!(status_guard), target!("medium")),
            transition!(target!("low"))  // Guardless fallback - REQUIRED
        ),
        state!("high"),
        state!("medium"), 
        state!("low")
    );

    // Should validate successfully
    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_final_states() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test final state definition
    let model = define!("FinalMachine",
        initial!(target!("working")),
        state!("working",
            transition!(on!("complete"), target!("../completed"))
        ),
        final_state!("completed")
    );

    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_hierarchical_patterns() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test hierarchical state patterns
    let model = define!("HierarchicalMachine",
        initial!(target!("system/initializing")),
        state!("system",
            entry!(state_entry),
            exit!(state_exit),
            state!("initializing",
                transition!(on!("initialized"), target!("../running"))
            ),
            state!("running",
                state!("idle",
                    transition!(on!("start"), target!("../processing"))
                ),
                state!("processing",
                    activity!(background_activity),
                    transition!(on!("complete"), target!("../idle")),
                    transition!(on!("error"), target!("../../error"))
                ),
                initial!(target!("idle"))
            ),
            state!("error",
                transition!(on!("reset"), target!("../running"))
            )
        )
    );

    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_timer_patterns() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test timer-based transitions (simplified for now)
    let model = define!("TimerMachine",
        initial!(target!("waiting")),
        state!("waiting",
            // Note: after! and every! macros would need full implementation
            // For now, testing that the pattern compiles
            transition!(on!("timeout"), target!("../finished"))
        ),
        state!("finished")
    );

    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_error_handling_patterns() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test that Result<()> patterns work
    let model = define!("ErrorMachine",
        initial!(target!("normal")),
        state!("normal",
            transition!(on!("error"), target!("../error"))
        ),
        state!("error")
    );

    // Should return Result
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_validation_errors() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test choice without guardless fallback should fail validation
    let bad_model = define!("BadChoiceMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("go"), target!("../bad_choice"))
        ),
        choice!("bad_choice",
            transition!(guard!(counter_guard), target!("never"))
            // Missing guardless fallback!
        ),
        state!("never")
    );

    // Should fail validation
    let validation_result = validate(&bad_model);
    assert!(validation_result.is_err());
    
    if let Err(HsmError::Validation(msg)) = validation_result {
        assert!(msg.contains("guardless fallback"));
    } else {
        panic!("Expected validation error");
    }

    Ok(())
}

#[tokio::test]
async fn test_deferred_events() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    // Test deferred event patterns
    let model = define!("DeferredMachine",
        initial!(target!("busy")),
        state!("busy",
            defer!("deferred_event", "another_event"),
            transition!(on!("finish"), target!("../ready"))
        ),
        state!("ready",
            transition!(on!("deferred_event"), target!("."))
        )
    );

    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;
    Ok(())
}

#[tokio::test]
async fn test_function_signatures() {
    // Test that function signatures follow the reference exactly
    
    // Entry/Exit/Effect: (ctx, inst, event) -> Pin<Box<dyn Future<Output = ()>>>
    let _entry: fn(&Context, &mut MyInstance, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> = state_entry;
    let _exit: fn(&Context, &mut MyInstance, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> = state_exit;
    let _effect: fn(&Context, &mut MyInstance, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> = transition_effect;
    let _activity: fn(&Context, &mut MyInstance, &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> = background_activity;
    
    // Guard: (ctx, inst, event) -> bool
    let _guard: fn(&Context, &MyInstance, &Event) -> bool = counter_guard;
    
    // Timer: (ctx, inst, event) -> Duration
    let _timer: fn(&Context, &MyInstance, &Event) -> Duration = short_delay;
    
    // All signatures should match the reference exactly
}

#[test]
fn test_synchronous_guards() {
    let instance = MyInstance::new();
    let ctx = Context::new();
    let event = Event::new("test");
    
    // Guards are synchronous and can be tested directly
    assert!(!counter_guard(&ctx, &instance, &event));
    
    // Modify instance state and test again
    let mut instance = instance;
    instance.counter = 10;
    assert!(counter_guard(&ctx, &instance, &event));
}

#[tokio::test]
async fn test_async_runtime_integration() -> Result<()> {
    let instance = MyInstance::new();
    let ctx = Context::new();

    let model = define!("AsyncMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("go"), target!("../processing"))
        ),
        state!("processing",
            activity!(background_activity),
            transition!(on!("complete"), target!("../finished"))
        ),
        final_state!("finished")
    );

    validate(&model)?;
    let _hsm = start(&ctx, instance, model)?;

    // Test that everything works with tokio runtime
    tokio::time::sleep(Duration::from_millis(1)).await;
    
    Ok(())
}

#[tokio::test]
async fn test_context_timeout() -> Result<()> {
    // Test context with timeout
    let ctx = Context::with_timeout(Duration::from_millis(50));
    
    // Should not be cancelled immediately
    assert!(!ctx.is_cancelled());
    
    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Should be cancelled after timeout
    assert!(ctx.is_cancelled());
    
    Ok(())
}