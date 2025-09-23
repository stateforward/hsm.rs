/**
 * @fileoverview Tests for initial transitions in HSM
 * Tests initial state behavior, automatic transitions, and initial target resolution
 */

use std::time::Duration;
use rust::*;

// Test instance for initial transition tests
#[derive(Debug)]
pub struct InitialTestInstance {
    pub current_state: String,
    pub entry_calls: Vec<String>,
    pub transition_count: i32,
    pub initialized: bool,
}

impl InitialTestInstance {
    pub fn new() -> Self {
        Self {
            current_state: "none".to_string(),
            entry_calls: Vec::new(),
            transition_count: 0,
            initialized: false,
        }
    }
    
    pub fn log_entry(&mut self, state: &str) {
        self.entry_calls.push(format!("enter_{}", state));
    }
    
    pub fn increment_transitions(&mut self) {
        self.transition_count += 1;
    }
    
    pub fn mark_initialized(&mut self) {
        self.initialized = true;
    }
}

impl Instance for InitialTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Entry action functions
fn init_entry(_ctx: &Context, inst: &mut InitialTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("init");
    inst.mark_initialized();
    Box::pin(async move {})
}

fn ready_entry(_ctx: &Context, inst: &mut InitialTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("ready");
    inst.current_state = "ready".to_string();
    Box::pin(async move {})
}

fn running_entry(_ctx: &Context, inst: &mut InitialTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("running");
    inst.current_state = "running".to_string();
    Box::pin(async move {})
}

fn nested_entry(_ctx: &Context, inst: &mut InitialTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_entry("nested");
    inst.current_state = "nested".to_string();
    Box::pin(async move {})
}

#[test]
fn test_simple_initial_transition() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test basic initial transition to a target state
    let model: Model<InitialTestInstance> = define!("SimpleInitialMachine",
        initial!(target!("ready")),
        state!("ready", 
            entry!(ready_entry)
        )
    );

    // Model should validate successfully
    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Simple initial machine should validate");

    // HSM should start successfully
    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Simple initial machine should start");
}

#[test]
fn test_nested_initial_transition() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test initial transition to nested state
    let model: Model<InitialTestInstance> = define!("NestedInitialMachine",
        initial!(target!("parent/child")),
        state!("parent",
            state!("child",
                entry!(nested_entry)
            )
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Nested initial machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Nested initial machine should start");
}

#[test]
fn test_absolute_path_initial() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test initial transition using absolute path
    let model: Model<InitialTestInstance> = define!("AbsoluteInitialMachine",
        initial!(target!("/AbsoluteInitialMachine/target_state")),
        state!("target_state",
            entry!(ready_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Absolute path initial should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Absolute path initial should start");
}

#[test]
fn test_hierarchical_initial_chain() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test hierarchical state with nested initial transitions
    let model: Model<InitialTestInstance> = define!("HierarchicalInitialMachine",
        initial!(target!("system")),
        state!("system",
            entry!(init_entry),
            initial!(target!("subsystem")),
            state!("subsystem",
                initial!(target!("component")),
                state!("component",
                    entry!(nested_entry)
                )
            )
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Hierarchical initial should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Hierarchical initial should start");
}

#[test]
fn test_multiple_initial_states() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test machine with separate initial states in different regions
    let model: Model<InitialTestInstance> = define!("MultiInitialMachine",
        initial!(target!("region1")),
        state!("region1",
            entry!(init_entry),
            initial!(target!("state1")),
            state!("state1",
                entry!(ready_entry)
            )
        ),
        state!("region2", 
            initial!(target!("state2")),
            state!("state2",
                entry!(running_entry)
            )
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Multi-initial machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-initial machine should start");
}

#[tokio::test]
async fn test_initial_with_async_entry() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Entry action that performs async work
    fn async_init_entry(_ctx: &Context, inst: &mut InitialTestInstance, _event: &Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        inst.log_entry("async_init");
        inst.mark_initialized();
        Box::pin(async move {
            // Simulate async initialization
            tokio::time::sleep(Duration::from_millis(1)).await;
        })
    }

    let model: Model<InitialTestInstance> = define!("AsyncInitialMachine",
        initial!(target!("initializing")),
        state!("initializing",
            entry!(async_init_entry),
            transition!(on!("complete"), target!("../ready"))
        ),
        state!("ready",
            entry!(ready_entry)
        )
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Async initial machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Async initial machine should start");
}

#[test]
fn test_initial_state_macro_variations() {
    let instance = InitialTestInstance::new();
    
    // Test different ways to specify initial targets
    
    // Direct state name
    let _target1: Box<dyn PartialElement<InitialTestInstance>> = target!("state1");
    
    // Relative path
    let _target2: Box<dyn PartialElement<InitialTestInstance>> = target!("../sibling");
    
    // Nested path
    let _target3: Box<dyn PartialElement<InitialTestInstance>> = target!("parent/child");
    
    // Absolute path
    let _target4: Box<dyn PartialElement<InitialTestInstance>> = target!("/Machine/state");
    
    // Initial with target
    let _initial1: Box<dyn PartialElement<InitialTestInstance>> = initial!(target!("ready"));
    
    // All should compile without errors
    assert!(true, "All initial macro variations should compile");
}

#[test] 
fn test_initial_target_validation() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Test that initial targets are validated
    let model: Model<InitialTestInstance> = define!("ValidInitialMachine",
        initial!(target!("existing_state")),
        state!("existing_state",
            entry!(ready_entry)
        )
    );

    // Should validate successfully when target exists
    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Valid initial target should pass validation");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Valid initial machine should start");
}

#[test]
fn test_initial_state_patterns() {
    // Test that initial states follow expected patterns
    let instance = InitialTestInstance::new();
    
    // Pattern 1: Machine starts with initial transition
    let model1: Model<InitialTestInstance> = define!("Pattern1Machine",
        initial!(target!("start")),
        state!("start")
    );
    
    // Pattern 2: Nested states have their own initials  
    let model2: Model<InitialTestInstance> = define!("Pattern2Machine",
        initial!(target!("parent")),
        state!("parent",
            initial!(target!("child")),
            state!("child")
        )
    );
    
    // Pattern 3: Multiple levels of nesting
    let model3: Model<InitialTestInstance> = define!("Pattern3Machine",
        initial!(target!("level1")),
        state!("level1",
            initial!(target!("level2")),
            state!("level2",
                initial!(target!("level3")),
                state!("level3")
            )
        )
    );
    
    // All patterns should validate
    assert!(validate(&model1).is_ok(), "Pattern 1 should validate");
    assert!(validate(&model2).is_ok(), "Pattern 2 should validate");
    assert!(validate(&model3).is_ok(), "Pattern 3 should validate");
}

#[test]
fn test_initial_pseudostate_behavior() {
    let instance = InitialTestInstance::new();
    let ctx = Context::new();

    // Initial pseudostates should not have entry/exit actions or transitions
    // This is enforced by the macro system - initial only takes target
    let model: Model<InitialTestInstance> = define!("PseudostateMachine",
        initial!(target!("real_state")),
        state!("real_state",
            entry!(ready_entry),
            transition!(on!("event"), target!("other_state"))
        ),
        state!("other_state")
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "Pseudostate machine should validate");

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Pseudostate machine should start");
}