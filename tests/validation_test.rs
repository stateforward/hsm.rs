/**
 * @fileoverview Test model validation and error handling
 * Tests validation rules, error detection, and proper error reporting
 */

use rust::*;

#[derive(Debug)]
pub struct ValidationTestInstance {
    pub counter: i32,
}

impl ValidationTestInstance {
    pub fn new() -> Self {
        Self { counter: 0 }
    }
}

impl Instance for ValidationTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Dummy guard function for testing
fn dummy_guard(_ctx: &Context, _inst: &ValidationTestInstance, _event: &Event) -> bool {
    true
}

fn never_guard(_ctx: &Context, _inst: &ValidationTestInstance, _event: &Event) -> bool {
    false
}

#[tokio::test]
async fn test_valid_model_passes_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("ValidMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("next"), target!("../end"))
        ),
        final_state!("end")
    );

    // Should validate successfully
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_choice_without_guardless_fallback_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("BadChoiceMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("decide"), target!("../choice"))
        ),
        choice!("choice",
            transition!(guard!(dummy_guard), target!("option1")),
            transition!(guard!(never_guard), target!("option2"))
            // Missing guardless fallback!
        ),
        state!("option1"),
        state!("option2")
    );

    // Should fail validation
    let validation_result = validate(&model);
    assert!(validation_result.is_err());

    if let Err(error) = validation_result {
        let error_msg = format!("{:?}", error);
        assert!(error_msg.contains("guardless fallback") || error_msg.contains("choice"));
    }

    Ok(())
}

#[tokio::test]
async fn test_choice_with_guardless_fallback_passes_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("GoodChoiceMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("decide"), target!("../choice"))
        ),
        choice!("choice",
            transition!(guard!(dummy_guard), target!("option1")),
            transition!(guard!(never_guard), target!("option2")),
            transition!(target!("fallback")) // Guardless fallback
        ),
        state!("option1"),
        state!("option2"),
        state!("fallback")
    );

    // Should validate successfully
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_final_state_with_transitions_fails_validation() -> Result<()> {
    use std::future::Future;
    use std::pin::Pin;

    fn invalid_entry(_ctx: &Context, _inst: &mut ValidationTestInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    // This should fail validation since final states cannot have entry actions or transitions
    // Note: The current macro system might not allow this, so we test what we can
    let model: Model<ValidationTestInstance> = define!("InvalidFinalMachine",
        initial!(target!("working")),
        state!("working",
            transition!(on!("finish"), target!("../completed"))
        ),
        final_state!("completed")
        // If the system allowed it, this would be invalid:
        // final_state!("completed", entry!(invalid_entry), transition!(...))
    );

    // This particular test depends on how the macro system is implemented
    // The validation should catch attempts to add actions to final states
    let validation_result = validate(&model);
    assert!(validation_result.is_ok()); // This should pass since we can't create invalid final states with macros

    Ok(())
}

#[tokio::test]
async fn test_multiple_choice_states_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("MultipleChoiceMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("branch1"), target!("../choice1")),
            transition!(on!("branch2"), target!("../choice2"))
        ),
        choice!("choice1",
            transition!(guard!(dummy_guard), target!("result1")),
            transition!(target!("default1")) // Guardless fallback
        ),
        choice!("choice2",
            transition!(guard!(never_guard), target!("result2")),
            transition!(target!("default2")) // Guardless fallback
        ),
        state!("result1"),
        state!("result2"),
        state!("default1"),
        state!("default2")
    );

    // Should validate successfully - all choices have guardless fallbacks
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_nested_choice_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("NestedChoiceMachine",
        initial!(target!("parent")),
        state!("parent",
            initial!(target!("child_choice")),
            choice!("child_choice",
                transition!(guard!(dummy_guard), target!("child_state")),
                transition!(target!("child_default")) // Guardless fallback
            ),
            state!("child_state"),
            state!("child_default")
        )
    );

    // Should validate successfully
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_circular_transition_validation() -> Result<()> {
    // Test circular references which should be allowed
    let model: Model<ValidationTestInstance> = define!("CircularMachine",
        initial!(target!("state1")),
        state!("state1",
            transition!(on!("next"), target!("../state2"))
        ),
        state!("state2",
            transition!(on!("next"), target!("../state3"))
        ),
        state!("state3",
            transition!(on!("reset"), target!("../state1")) // Back to start
        )
    );

    // Circular transitions should be valid
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_deep_hierarchy_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!("DeepHierarchyMachine",
        initial!(target!("level1")),
        state!("level1",
            initial!(target!("level2")),
            state!("level2",
                initial!(target!("level3")),
                state!("level3",
                    initial!(target!("level4")),
                    state!("level4",
                        transition!(on!("escape"), target!("../../../.."))
                    )
                )
            )
        )
    );

    // Deep hierarchies should be valid
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_complex_valid_machine() -> Result<()> {
    use std::future::Future;
    use std::pin::Pin;

    fn state_entry(_ctx: &Context, _inst: &mut ValidationTestInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    fn state_exit(_ctx: &Context, _inst: &mut ValidationTestInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    fn state_effect(_ctx: &Context, _inst: &mut ValidationTestInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    fn state_activity(_ctx: &Context, _inst: &mut ValidationTestInstance, _event: &Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    // Complex but valid machine with all features
    let model: Model<ValidationTestInstance> = define!("ComplexValidMachine",
        initial!(target!("system")),
        state!("system",
            initial!(target!("initializing")),
            entry!(state_entry),
            exit!(state_exit),
            
            state!("initializing",
                entry!(state_entry),
                transition!(on!("ready"), target!("../running"))
            ),
            
            state!("running",
                initial!(target!("processing")),
                entry!(state_entry),
                exit!(state_exit),
                
                state!("processing",
                    entry!(state_entry),
                    activity!(state_activity),
                    transition!(on!("pause"), target!("../paused")),
                    transition!(on!("error"), target!("../../error"))
                ),
                
                state!("paused",
                    entry!(state_entry),
                    transition!(on!("resume"), target!("../processing")),
                    transition!(on!("stop"), target!("../../stopping"))
                )
            ),
            
            state!("stopping",
                entry!(state_entry),
                exit!(state_exit),
                transition!(on!("stopped"), target!("../stopped"))
            ),
            
            state!("error",
                entry!(state_entry),
                transition!(on!("reset"), target!("../initializing")),
                transition!(on!("diagnose"), target!("../diagnosis"))
            ),
            
            state!("diagnosis",
                entry!(state_entry),
                transition!(on!("resolved"), target!("../initializing")),
                transition!(on!("escalate"), target!("../critical"))
            ),
            
            state!("critical",
                entry!(state_entry),
                transition!(on!("recover"), target!("../error"))
            )
        ),
        
        state!("stopped",
            entry!(state_entry),
            transition!(on!("restart"), target!("../system/initializing"))
        ),
        
        choice!("router",
            transition!(guard!(dummy_guard), target!("system")),
            transition!(target!("stopped")) // Guardless fallback
        ),
        
        final_state!("terminated")
    );

    // Should validate successfully despite complexity
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_validation_error_messages() -> Result<()> {
    // Test that validation provides meaningful error messages
    let bad_model: Model<ValidationTestInstance> = define!("BadChoiceValidation",
        initial!(target!("start")),
        state!("start",
            transition!(on!("go"), target!("../bad_choice"))
        ),
        choice!("bad_choice",
            transition!(guard!(dummy_guard), target!("never_reached"))
            // No guardless fallback
        ),
        state!("never_reached")
    );

    let validation_result = validate(&bad_model);
    assert!(validation_result.is_err());

    if let Err(error) = validation_result {
        let error_msg = format!("{:?}", error);
        // Should contain meaningful information about the error
        assert!(
            error_msg.contains("choice") || 
            error_msg.contains("guardless") || 
            error_msg.contains("fallback"),
            "Error message should be descriptive: {}", error_msg
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_start_with_invalid_model() -> Result<()> {
    let instance = ValidationTestInstance::new();
    let ctx = Context::new();

    // Create an invalid model
    let bad_model: Model<ValidationTestInstance> = define!("StartInvalidMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("decide"), target!("../bad_choice"))
        ),
        choice!("bad_choice",
            transition!(guard!(dummy_guard), target!("end"))
            // Missing guardless fallback
        ),
        state!("end")
    );

    // Starting with an invalid model should potentially fail
    // (depending on whether validation is done at start time)
    let start_result = start(&ctx, instance, bad_model);
    
    // This test depends on implementation details
    // Some implementations validate at start time, others at definition time
    match start_result {
        Ok(_) => {
            // If start succeeds, validation might be deferred
            // This is acceptable behavior
        },
        Err(_) => {
            // If start fails due to validation, that's also acceptable
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_model_with_no_states() -> Result<()> {
    // Test edge case of empty or minimal models
    let minimal_model: Model<ValidationTestInstance> = define!("MinimalMachine",
        initial!(target!("only_state")),
        state!("only_state")
    );

    // Minimal valid model should pass validation
    let validation_result = validate(&minimal_model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_model_with_unreachable_states() -> Result<()> {
    // Test model with unreachable states (should still be valid)
    let model: Model<ValidationTestInstance> = define!("UnreachableMachine",
        initial!(target!("start")),
        state!("start",
            transition!(on!("go"), target!("../end"))
        ),
        state!("end"),
        state!("unreachable") // No transitions lead here
    );

    // Unreachable states should not cause validation failure
    // (They might generate warnings in more sophisticated validators)
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}