/**
 * Simplified tests for the Rust HSM implementation following the reference patterns
 * Tests basic functionality that currently works
 */

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

#[tokio::test]
async fn test_context_creation() {
    // Test context creation patterns
    let ctx1 = Context::new();
    assert!(!ctx1.is_cancelled());
    
    let ctx2 = Context::with_timeout(Duration::from_millis(100));
    assert!(!ctx2.is_cancelled());
    
    // Test cancellation
    ctx1.cancel();
    assert!(ctx1.is_cancelled());
    assert!(ctx1.is_done()); // Legacy compatibility
}

#[test]
fn test_event_with_typed_data() {
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
}

#[test]
fn test_instance_trait() {
    let instance = MyInstance::new();
    
    // Test that Instance trait methods work
    let _any_ref = instance.as_any();
    
    let mut instance = instance;
    let _any_mut_ref = instance.as_any_mut();
    
    // Test instance functionality
    instance.increment();
    assert_eq!(instance.counter, 1);
    
    instance.log("test message");
    assert_eq!(instance.history.len(), 1);
    assert_eq!(instance.history[0], "test message");
}

#[test]
fn test_basic_macro_usage() {
    let _instance = MyInstance::new();
    let _ctx = Context::new();

    // Test that basic macros compile and create elements
    let _target_element: Box<dyn PartialElement<MyInstance>> = target!("test_target");
    let _trigger_element: Box<dyn PartialElement<MyInstance>> = on!("test_event");
    
    // These should compile without errors with type annotation
    let _model: Model<MyInstance> = define!("TestMachine",
        state!("idle"),
        state!("active")
    );
}

#[test]
fn test_error_types() {
    // Test that error types work correctly
    let validation_error = HsmError::Validation("test error".to_string());
    
    match validation_error {
        HsmError::Validation(msg) => {
            assert_eq!(msg, "test error");
        }
        _ => panic!("Expected validation error"),
    }
}

#[test]
fn test_start_function() {
    let instance = MyInstance::new();
    let ctx = Context::new();
    
    let model: Model<MyInstance> = define!("StartTestMachine",
        state!("start"),
        state!("end")
    );
    
    // Test that start function returns Result
    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok());
}

#[test] 
fn test_validation_function() {
    let model: Model<MyInstance> = define!("ValidationTestMachine",
        state!("test1"),
        state!("test2")
    );
    
    // Test that validation function works
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());
}

#[tokio::test]
async fn test_context_timeout() {
    // Test context with timeout
    let ctx = Context::with_timeout(Duration::from_millis(50));
    
    // Should not be cancelled immediately
    assert!(!ctx.is_cancelled());
    
    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Should be cancelled after timeout
    assert!(ctx.is_cancelled());
}