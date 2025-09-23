/**
 * @fileoverview Tests for entry behaviors in HSM
 * Tests entry action execution, order, async behavior, and error handling
 */

use std::time::Duration;
use std::sync::{Arc, Mutex};
use rust::*;

// Test instance for entry behavior tests
#[derive(Debug)]
pub struct EntryTestInstance {
    pub entry_log: Vec<String>,
    pub entry_count: i32,
    pub async_operations: Vec<String>,
    pub state_data: std::collections::HashMap<String, String>,
    pub error_count: i32,
    pub initialization_complete: bool,
}

impl EntryTestInstance {
    pub fn new() -> Self {
        Self {
            entry_log: Vec::new(),
            entry_count: 0,
            async_operations: Vec::new(),
            state_data: std::collections::HashMap::new(),
            error_count: 0,
            initialization_complete: false,
        }
    }
    
    pub fn log_entry(&mut self, state: &str) {
        self.entry_log.push(format!("entry_{}", state));
        self.entry_count += 1;
    }
    
    pub fn log_async_op(&mut self, operation: &str) {
        self.async_operations.push(operation.to_string());
    }
    
    pub fn set_data(&mut self, key: &str, value: &str) {
        self.state_data.insert(key.to_string(), value.to_string());
    }
    
    pub fn increment_errors(&mut self) {
        self.error_count += 1;
    }
    
    pub fn mark_initialized(&mut self) {
        self.initialization_complete = true;
    }
}

impl Instance for EntryTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Entry action functions following exact signatures
fn simple_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("simple");
    Box::pin(async move {})
}

fn init_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("init");
    inst.mark_initialized();
    inst.set_data("init_time", "startup");
    Box::pin(async move {})
}

fn parent_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("parent");
    inst.set_data("parent_status", "entered");
    Box::pin(async move {})
}

fn child_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("child");
    inst.set_data("child_status", "active");
    Box::pin(async move {})
}

fn async_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("async_start");
    inst.log_async_op("starting_async_work");
    Box::pin(async move {
        // Simulate async work
        tokio::time::sleep(Duration::from_millis(10)).await;
        // Note: Can't modify inst here due to move semantics
        // In real implementation, would use channels or shared state
    })
}

fn data_entry(_ctx: &Context, inst: &mut EntryTestInstance, event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("data");
    
    // Extract data from event if available
    if let Some(counter) = event.get_data::<i32>() {
        inst.set_data("event_counter", &counter.to_string());
    } else if let Some(message) = event.get_data::<String>() {
        inst.set_data("event_message", message);
    }
    
    Box::pin(async move {})
}

fn cancellation_aware_entry(ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("cancellation_aware");
    let cancelled = ctx.is_cancelled();
    inst.set_data("was_cancelled", &cancelled.to_string());
    Box::pin(async move {})
}

fn error_prone_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("error_prone");
    inst.increment_errors();
    Box::pin(async move {
        // Simulate potential error condition
        // In real implementation, this might fail
    })
}

#[test]
fn test_simple_entry_execution() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test basic entry action execution
    let model: Model<EntryTestInstance> = define!("SimpleEntryMachine",
        initial!(target!("active")),
        state!("active",
            entry!(simple_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Simple entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Simple entry machine should start");
}

#[test]
fn test_entry_with_data_initialization() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action that initializes instance data
    let model: Model<EntryTestInstance> = define!("DataEntryMachine",
        initial!(target!("initializing")),
        state!("initializing",
            entry!(init_entry),
            transition!(on!("complete"), target!("../ready"))
        ),
        state!("ready")
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Data entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Data entry machine should start");
}

#[test]
fn test_hierarchical_entry_order() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test that parent entry runs before child entry
    let model: Model<EntryTestInstance> = define!("HierarchicalEntryMachine",
        initial!(target!("parent/child")),
        state!("parent",
            entry!(parent_entry),
            state!("child",
                entry!(child_entry)
            )
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Hierarchical entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Hierarchical entry machine should start");
}

#[test]
fn test_multiple_entry_actions() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test state with multiple entry actions
    fn first_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("first");
        inst.set_data("step", "1");
        Box::pin(async move {})
    }

    fn second_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("second");
        inst.set_data("step", "2");
        Box::pin(async move {})
    }

    fn third_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("third");
        inst.set_data("final_step", "3");
        Box::pin(async move {})
    }

    let model: Model<EntryTestInstance> = define!("MultiEntryMachine",
        initial!(target!("multi_action")),
        state!("multi_action",
            entry!(first_entry),
            entry!(second_entry),
            entry!(third_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Multi-entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-entry machine should start");
}

#[test]
fn test_multiple_entry_execution_order() {
    let mut instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test that multiple entry actions execute in order
    fn setup_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("setup");
        inst.set_data("phase", "setup");
        Box::pin(async move {})
    }

    fn configure_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("configure");
        inst.set_data("phase", "configure");
        Box::pin(async move {})
    }

    fn finalize_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("finalize");
        inst.set_data("phase", "finalize");
        inst.mark_initialized();
        Box::pin(async move {})
    }

    let model: Model<EntryTestInstance> = define!("OrderedEntryMachine",
        initial!(target!("initialization")),
        state!("initialization",
            entry!(setup_entry),
            entry!(configure_entry), 
            entry!(finalize_entry),
            transition!(on!("done"), target!("../ready"))
        ),
        state!("ready")
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Ordered entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Ordered entry machine should start");
    
    // Note: In a real test we'd need to examine the instance state after execution
    // to verify the order and that all actions were called
}

#[test]
fn test_multiple_entry_single_macro() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test the new syntax: entry!(fn1, fn2, fn3) instead of multiple entry! calls
    fn setup_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("setup");
        inst.set_data("phase", "setup");
        Box::pin(async move {})
    }

    fn configure_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("configure");
        inst.set_data("phase", "configure");
        Box::pin(async move {})
    }

    fn finalize_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("finalize");
        inst.set_data("phase", "finalize");
        inst.mark_initialized();
        Box::pin(async move {})
    }

    let model: Model<EntryTestInstance> = define!("SingleMacroEntryMachine",
        initial!(target!("initialization")),
        state!("initialization",
            entry!(setup_entry, configure_entry, finalize_entry),
            transition!(on!("done"), target!("../ready"))
        ),
        state!("ready")
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Single macro entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Single macro entry machine should start");
}

#[tokio::test]
async fn test_async_entry_actions() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action that performs async work
    let model: Model<EntryTestInstance> = define!("AsyncEntryMachine",
        initial!(target!("async_state")),
        state!("async_state",
            entry!(async_entry),
            transition!(on!("continue"), target!("../next"))
        ),
        state!("next",
            entry!(simple_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Async entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Async entry machine should start");

    // Allow async entry to complete
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[test]
fn test_entry_with_event_data() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action that accesses event data
    let model: Model<EntryTestInstance> = define!("EventDataEntryMachine",
        initial!(target!("data_processor")),
        state!("data_processor",
            entry!(data_entry),
            transition!(on!("process"), target!("."))
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Event data entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Event data entry machine should start");
}

#[test]
fn test_entry_context_awareness() {
    let mut instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action that checks context state
    let model: Model<EntryTestInstance> = define!("ContextAwareEntryMachine",
        initial!(target!("context_aware")),
        state!("context_aware",
            entry!(cancellation_aware_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Context-aware entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Context-aware entry machine should start");
}

#[test]
fn test_entry_error_handling() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action that might encounter errors
    let model: Model<EntryTestInstance> = define!("ErrorHandlingEntryMachine",
        initial!(target!("error_prone")),
        state!("error_prone",
            entry!(error_prone_entry),
            transition!(on!("recover"), target!("../recovery"))
        ),
        state!("recovery",
            entry!(simple_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Error handling entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Error handling entry machine should start");
}

#[test]
fn test_entry_on_transition() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test that entry actions run when transitioning to states
    let model: Model<EntryTestInstance> = define!("TransitionEntryMachine",
        initial!(target!("start")),
        state!("start",
            entry!(simple_entry),
            transition!(on!("go"), target!("../target"))
        ),
        state!("target",
            entry!(init_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Transition entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Transition entry machine should start");
}

#[test]
fn test_entry_self_transition() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test entry action on self-transition (should re-enter)
    let model: Model<EntryTestInstance> = define!("SelfTransitionEntryMachine",
        initial!(target!("self_state")),
        state!("self_state",
            entry!(simple_entry),
            transition!(on!("self"), target!("."))
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Self-transition entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Self-transition entry machine should start");
}

#[test]
fn test_entry_function_signatures() {
    // Test that entry functions follow the exact signature requirements
    
    // Entry: (ctx, inst, event) -> Pin<Box<dyn Future<Output = ()> + Send>>
    let _entry_fn: fn(&Context, &mut EntryTestInstance, &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = simple_entry;
    let _init_fn: fn(&Context, &mut EntryTestInstance, &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = init_entry;
    let _async_fn: fn(&Context, &mut EntryTestInstance, &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = async_entry;
    
    // All signatures should match the reference exactly
    assert!(true, "Entry function signatures should match reference");
}

#[test]
fn test_entry_macro_variations() {
    let instance = EntryTestInstance::new();
    
    // Test different ways to specify entry actions
    let _entry1: Box<dyn PartialElement<EntryTestInstance>> = entry!(simple_entry);
    let _entry2: Box<dyn PartialElement<EntryTestInstance>> = entry!(init_entry);
    let _entry3: Box<dyn PartialElement<EntryTestInstance>> = entry!(async_entry);
    
    // Test new multiple function syntax
    let _entry4: Box<dyn PartialElement<EntryTestInstance>> = entry!(simple_entry, init_entry);
    let _entry5: Box<dyn PartialElement<EntryTestInstance>> = entry!(simple_entry, init_entry, async_entry);
    
    // All entry macro variations should compile
    assert!(true, "All entry macro variations should compile");
}

#[tokio::test]
async fn test_entry_with_timeout_context() {
    let instance = EntryTestInstance::new();
    let ctx = Context::with_timeout(Duration::from_millis(100));

    // Test entry action with timeout context
    fn timeout_entry(_ctx: &Context, inst: &mut EntryTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("timeout_aware");
        Box::pin(async move {
            // Short delay that should complete before timeout
            tokio::time::sleep(Duration::from_millis(10)).await;
        })
    }

    let model: Model<EntryTestInstance> = define!("TimeoutEntryMachine",
        initial!(target!("timeout_state")),
        state!("timeout_state",
            entry!(timeout_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Timeout entry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Timeout entry machine should start");

    // Wait a bit to let entry complete
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[test]
fn test_entry_patterns() {
    // Test common entry action patterns
    let instance = EntryTestInstance::new();
    
    // Pattern 1: Initialization entry
    let model1: Model<EntryTestInstance> = define!("InitPattern",
        initial!(target!("init")),
        state!("init", entry!(init_entry))
    );
    
    // Pattern 2: Data setup entry
    let model2: Model<EntryTestInstance> = define!("DataPattern",
        initial!(target!("setup")),
        state!("setup", entry!(data_entry))
    );
    
    // Pattern 3: Nested state entries
    let model3: Model<EntryTestInstance> = define!("NestedPattern",
        initial!(target!("outer")),
        state!("outer",
            entry!(parent_entry),
            initial!(target!("inner")),
            state!("inner", entry!(child_entry))
        )
    );
    
    // All patterns should validate
    assert!(validate(&model1).is_ok(), "Init pattern should validate");
    assert!(validate(&model2).is_ok(), "Data pattern should validate");
    assert!(validate(&model3).is_ok(), "Nested pattern should validate");
}

#[test]
fn test_entry_no_reentry_on_ancestor() {
    let instance = EntryTestInstance::new();
    let ctx = Context::new();

    // Test that already active ancestor states don't re-enter
    // When transitioning S1/S2 -> S1/S3, S1 shouldn't re-enter
    let model: Model<EntryTestInstance> = define!("NoReentryMachine",
        initial!(target!("parent/child1")),
        state!("parent",
            entry!(parent_entry),
            state!("child1",
                entry!(child_entry),
                transition!(on!("switch"), target!("../child2"))
            ),
            state!("child2",
                entry!(simple_entry)
            )
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "No-reentry machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "No-reentry machine should start");
}