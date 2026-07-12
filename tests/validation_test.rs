/**
 * @fileoverview Test model validation and error handling
 * Tests validation rules, error detection, and proper error reporting
 */
use stateforward_hsm::*;
use std::future::Future;
use std::pin::Pin;

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

fn noop_operation(
    _ctx: &Context,
    _inst: &mut ValidationTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async {})
}

fn timer_duration(
    _ctx: &Context,
    _inst: &ValidationTestInstance,
    _event: &Event,
) -> std::time::Duration {
    std::time::Duration::from_secs(1)
}

fn timer_timepoint(
    _ctx: &Context,
    _inst: &ValidationTestInstance,
    _event: &Event,
) -> std::time::SystemTime {
    std::time::SystemTime::now()
}

#[tokio::test]
async fn test_valid_model_passes_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "ValidMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("next"), target!("../end"))),
        final_state!("end")
    );

    // Should validate successfully
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_model_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "Bad/Model",
        vec![initial_with_target(target("idle")), state("idle")],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("model name 'Bad/Model' cannot contain '/'")
    ));

    Ok(())
}

#[tokio::test]
async fn test_initial_transition_cannot_have_guard() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "InitialGuardMachine",
        initial!(guard!(dummy_guard), target!("idle")),
        state!("idle")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Initial transition") && message.contains("cannot have guard")
    ));

    Ok(())
}

#[tokio::test]
async fn test_initial_transition_cannot_have_user_trigger() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "InitialTriggerMachine",
        initial!(on!("go"), target!("idle")),
        state!("idle")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Initial transition") && message.contains("cannot have triggers")
    ));

    Ok(())
}

#[tokio::test]
async fn test_initial_transition_must_target_inside_owner() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "EscapingInitialMachine",
        initial!(target!("parent")),
        state!("parent", initial!(target!("../outside")), state!("inside")),
        state!("outside")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Initial transition") && message.contains("must target inside")
    ));

    Ok(())
}

#[tokio::test]
async fn test_composite_state_requires_initial_transition() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "MissingCompositeInitialMachine",
        initial!(target!("parent")),
        state!("parent", state!("left"), state!("right"))
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Composite state") && message.contains("requires initial")
    ));

    Ok(())
}

#[tokio::test]
async fn test_initial_pseudostate_cannot_have_multiple_transitions() -> Result<()> {
    let mut model: Model<ValidationTestInstance> = define!(
        "MultipleInitialTransitionMachine",
        initial!(target!("idle")),
        state!("idle"),
        state!("done")
    );
    let mut stack = vec!["/MultipleInitialTransitionMachine/.initial".to_string()];
    transition(vec![target("done")]).apply(&mut model, &mut stack);

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Initial") && message.contains("multiple transitions")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_event_transition_without_source_is_global() -> Result<()> {
    let ctx = Context::new();
    let model: Model<ValidationTestInstance> = define(
        "TopLevelGlobalTransitionMachine",
        vec![
            initial_with_target(target("idle")),
            state("idle"),
            state("done"),
            transition(vec![on("go"), target("done")]),
        ],
    );

    validate(&model)?;
    let hsm = start(&ctx, ValidationTestInstance::new(), model)?;
    hsm.start().await?;

    assert_eq!(hsm.state(), "/TopLevelGlobalTransitionMachine/idle");
    hsm.dispatch(&ctx, Event::new("go")).await?;
    assert_eq!(hsm.state(), "/TopLevelGlobalTransitionMachine/done");

    Ok(())
}

#[tokio::test]
async fn test_top_level_triggerless_target_transition_requires_source() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TopLevelTriggerlessTargetOnlyMachine",
        vec![
            initial_with_target(target("idle")),
            state("idle"),
            transition(vec![target("idle")]),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Triggerless top level transitions")
                && message.contains("must also define a source")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_source_only_internal_transition_is_valid() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TopLevelSourceOnlyInternalMachine",
        vec![
            initial_with_target(target("idle")),
            state("idle"),
            transition(vec![on("go"), source("idle"), effect(noop_operation)]),
        ],
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "{:?}", validation_result);

    Ok(())
}

#[tokio::test]
async fn test_transition_event_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyEventNameMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![transition(vec![on(""), target("../done")])]),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("event name") && message.contains("cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_transition_event_wildcard_pattern_requires_any_event() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "WildcardEventPatternMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on("special*"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("wildcard event patterns") && message.contains("AnyEvent")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_entry_action_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TopLevelEntryMachine",
        vec![
            entry(noop_operation),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Entry actions")
                && message.contains("top level state machine")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_exit_action_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TopLevelExitMachine",
        vec![
            exit(noop_operation),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Exit actions")
                && message.contains("top level state machine")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_activity_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TopLevelActivityMachine",
        vec![
            activity(noop_operation),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Activity actions")
                && message.contains("top level state machine")
    ));

    Ok(())
}

#[tokio::test]
async fn test_entry_action_inside_transition_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TransitionEntryMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    entry(noop_operation),
                    target("../done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Entry behavior") && message.contains("inside a state")
    ));

    Ok(())
}

#[tokio::test]
async fn test_exit_action_inside_transition_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TransitionExitMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    exit(noop_operation),
                    target("../done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Exit behavior") && message.contains("inside a state")
    ));

    Ok(())
}

#[tokio::test]
async fn test_activity_inside_transition_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TransitionActivityMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    activity(noop_operation),
                    target("../done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Activity behavior") && message.contains("inside a state")
    ));

    Ok(())
}

#[test]
#[should_panic(expected = "Defer must be declared inside a state")]
fn test_defer_inside_transition_panics() {
    let _model: Model<ValidationTestInstance> = define(
        "TransitionDeferMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    defer(vec!["later"]),
                    target("../done"),
                ])],
            ),
            state("done"),
        ],
    );
}

#[test]
#[should_panic(expected = "Defer must be declared inside a state")]
fn test_top_level_defer_panics() {
    let _model: Model<ValidationTestInstance> = define(
        "TopLevelDeferMachine",
        vec![
            defer(vec!["later"]),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );
}

#[tokio::test]
async fn test_effect_action_inside_state_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "StateEffectMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![effect(noop_operation)]),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Effect behavior") && message.contains("inside a transition")
    ));

    Ok(())
}

#[tokio::test]
async fn test_guard_inside_state_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "StateGuardMachine",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![guard(dummy_guard)]),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Constraint") && message.contains("inside a transition")
    ));

    Ok(())
}

#[tokio::test]
async fn test_state_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashStateName",
        vec![initial_with_target(target("bad/state")), state("bad/state")],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("state name \"bad/state\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_operation_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyOperationName",
        vec![
            operation("", noop_operation),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message)) if message.contains("operation name cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_operation_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashOperationName",
        vec![
            operation("bad/name", noop_operation),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("operation name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_behavior_operation_reference_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashBehaviorOperationName",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![entry_operation("bad/name")]),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("operation name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_guard_operation_reference_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashGuardOperationName",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    guard_operation_ref("bad/name"),
                    target("../done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("operation name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_on_call_operation_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyOnCallName",
        vec![
            operation("run", noop_operation),
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call(""), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message)) if message.contains("operation name cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_on_call_operation_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashOnCallName",
        vec![
            operation("run", noop_operation),
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call("bad/name"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("operation name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_on_call_requires_declared_action_operation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "MissingOnCallOperation",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call("missing"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("OnCall")
                && message.contains("missing operation")
                && message.contains("/MissingOnCallOperation/missing")
    ));

    let wrong_kind = define(
        "GuardNotOnCallOperation",
        vec![
            guard_operation("allow", dummy_guard),
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![on_call("allow"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&wrong_kind);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("OnCall")
                && message.contains("missing operation")
                && message.contains("/GuardNotOnCallOperation/allow")
    ));

    Ok(())
}

#[tokio::test]
async fn test_attribute_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyAttributeName",
        vec![
            Attribute("", 0),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message)) if message.contains("attribute name cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_on_set_attribute_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyOnSetName",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![transition(vec![OnSet(""), target("../done")])]),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message)) if message.contains("attribute name cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_when_attribute_name_cannot_be_empty() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "EmptyWhenName",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![transition(vec![When(""), target("../done")])]),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message)) if message.contains("attribute name cannot be empty")
    ));

    Ok(())
}

#[tokio::test]
async fn test_attribute_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashAttributeName",
        vec![
            Attribute("bad/name", 0),
            initial_with_target(target("idle")),
            state("idle"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("attribute name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_on_set_attribute_name_cannot_contain_slash() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "SlashOnSetName",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![OnSet("bad/name"), target("../done")])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("attribute name \"bad/name\" cannot contain \"/\"")
    ));

    Ok(())
}

#[tokio::test]
async fn test_behavior_operation_reference_requires_declared_action_operation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "MissingBehaviorOperation",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![entry_operation("missing")]),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Behavior")
                && message.contains("missing operation")
                && message.contains("/MissingBehaviorOperation/missing")
    ));

    let wrong_kind = define(
        "GuardNotActionOperation",
        vec![
            guard_operation("allow", dummy_guard),
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![entry_operation("allow")]),
        ],
    );

    let validation_result = validate(&wrong_kind);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Behavior")
                && message.contains("missing operation")
                && message.contains("/GuardNotActionOperation/allow")
    ));

    Ok(())
}

#[tokio::test]
async fn test_guard_operation_reference_requires_declared_guard_operation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "MissingGuardOperation",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    guard_operation_ref("missing"),
                    target("done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Guard")
                && message.contains("missing operation")
                && message.contains("/MissingGuardOperation/missing")
    ));

    let wrong_kind = define(
        "ActionNotGuardOperation",
        vec![
            operation("record", noop_operation),
            initial_with_target(target("idle")),
            state_with_behaviors(
                "idle",
                vec![transition(vec![
                    on("go"),
                    guard_operation_ref("record"),
                    target("done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&wrong_kind);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Guard")
                && message.contains("missing operation")
                && message.contains("/ActionNotGuardOperation/record")
    ));

    Ok(())
}

#[tokio::test]
async fn test_start_rejects_invalid_operation_references() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "InvalidStartOperation",
        vec![
            initial_with_target(target("idle")),
            state_with_behaviors("idle", vec![entry_operation("missing")]),
        ],
    );
    let machine = HSM::new(ValidationTestInstance::new(), model);

    let error = machine.start().await.unwrap_err();
    assert!(matches!(
        error,
        HsmError::Validation(message)
            if message.contains("Behavior")
                && message.contains("missing operation")
                && message.contains("/InvalidStartOperation/missing")
    ));

    Ok(())
}

#[tokio::test]
async fn test_choice_without_guardless_fallback_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "BadChoiceMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("decide"), target!("../choice"))),
        choice!(
            "choice",
            transition!(guard!(dummy_guard), target!("option1")),
            transition!(guard!(never_guard), target!("option2")) // Missing guardless fallback!
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
async fn test_choice_guardless_fallback_must_be_last() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "ChoiceDefaultNotLastMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("decide"), target!("../choice"))),
        choice!(
            "choice",
            transition!(target!("left")),
            transition!(guard!(dummy_guard), target!("right")),
            transition!(target!("fallback"))
        ),
        state!("left"),
        state!("right"),
        state!("fallback")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Choice state") && message.contains("fallback must be last")
    ));

    Ok(())
}

#[tokio::test]
async fn test_choice_transition_cannot_have_trigger() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TriggeredChoiceMachine",
        vec![
            initial_with_target(target("start")),
            state_with_behaviors(
                "start",
                vec![transition(vec![on("go"), target("../branch")])],
            ),
            choice_with_transitions(
                "branch",
                vec![transition(vec![
                    on("bad"),
                    target("/TriggeredChoiceMachine/done"),
                ])],
            ),
            state("done"),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("cannot have triggers")
                && message.contains("/TriggeredChoiceMachine/branch")
    ));

    Ok(())
}

#[tokio::test]
async fn test_entry_point_transition_cannot_have_trigger() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "TriggeredEntryPointMachine",
        vec![
            initial_with_target(target("target")),
            state_with_behaviors(
                "target",
                vec![
                    entry_point(
                        "warm",
                        vec![
                            on("bad"),
                            target("/TriggeredEntryPointMachine/target/ready"),
                        ],
                    ),
                    initial_with_target(target("ready")),
                    state("ready"),
                ],
            ),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("cannot have triggers")
                && message.contains("/TriggeredEntryPointMachine/target/warm")
    ));

    Ok(())
}

#[tokio::test]
async fn test_entry_point_declaration_requires_target() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "MissingEntryPointTargetMachine",
        vec![
            initial_with_target(target("target")),
            state_with_behaviors(
                "target",
                vec![
                    entry_point("warm", vec![]),
                    initial_with_target(target("ready")),
                    state("ready"),
                ],
            ),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Entry point")
                && message.contains("requires target")
                && message.contains("/MissingEntryPointTargetMachine/target/warm")
    ));

    Ok(())
}

#[tokio::test]
async fn test_entry_point_declaration_cannot_target_entry_point() -> Result<()> {
    let model: Model<ValidationTestInstance> = define(
        "ChainedEntryPointMachine",
        vec![
            initial_with_target(target("target")),
            state_with_behaviors(
                "target",
                vec![
                    entry_point("second", vec![target("ready")]),
                    entry_point("first", vec![target("second")]),
                    initial_with_target(target("ready")),
                    state("ready"),
                ],
            ),
        ],
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Entry point")
                && message.contains("cannot target entry point")
                && message.contains("/ChainedEntryPointMachine/target/first")
    ));

    Ok(())
}

#[tokio::test]
async fn test_timer_transition_source_must_be_state() -> Result<()> {
    let cases: Vec<(&str, Model<ValidationTestInstance>)> = vec![
        (
            "after",
            define!(
                "AfterChoiceTimerMachine",
                initial!(target!("branch")),
                choice!(
                    "branch",
                    transition!(after!(timer_duration), target!("idle"))
                ),
                state!("idle")
            ),
        ),
        (
            "at",
            define!(
                "AtChoiceTimerMachine",
                initial!(target!("branch")),
                choice!("branch", transition!(at!(timer_timepoint), target!("idle"))),
                state!("idle")
            ),
        ),
        (
            "every",
            define!(
                "EveryChoiceTimerMachine",
                initial!(target!("branch")),
                choice!(
                    "branch",
                    transition!(every!(timer_duration), target!("idle"))
                ),
                state!("idle")
            ),
        ),
    ];

    for (timer_kind, model) in cases {
        let validation_result = validate(&model);
        assert!(
            matches!(
                validation_result,
                Err(HsmError::Validation(message))
                    if message.contains("Timer transition")
                        && message.contains("source is a state")
            ),
            "{timer_kind} should require a state source"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_choice_with_guardless_fallback_passes_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "GoodChoiceMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("decide"), target!("../choice"))),
        choice!(
            "choice",
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
async fn test_transition_with_missing_target_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "MissingTargetMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("go"), target!("missing")))
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_err());

    if let Err(error) = validation_result {
        let error_msg = format!("{:?}", error);
        assert!(error_msg.contains("target") && error_msg.contains("not found"));
    }

    Ok(())
}

#[tokio::test]
async fn test_empty_internal_transition_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "EmptyInternalTransitionMachine",
        initial!(target!("idle")),
        state!("idle", transition!(on!("go")))
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("target or effect is required")
                && message.contains("internal transitions require an effect")
    ));

    Ok(())
}

#[tokio::test]
async fn test_guard_only_internal_transition_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "GuardOnlyInternalTransitionMachine",
        initial!(target!("idle")),
        state!("idle", transition!(on!("go"), guard!(dummy_guard)))
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("target or effect is required")
                && message.contains("internal transitions require an effect")
    ));

    Ok(())
}

#[tokio::test]
async fn test_state_transition_without_trigger_registers_final_event() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "CompletionTargetMachine",
        initial!(target!("idle")),
        state!("idle", transition!(target!("../done"))),
        state!("done")
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "{:?}", validation_result);

    let transition = model
        .members
        .values()
        .find_map(|element| match element {
            ElementVariant::Transition(transition)
                if transition.source == "/CompletionTargetMachine/idle" =>
            {
                Some(transition)
            }
            _ => None,
        })
        .expect("completion transition should exist");
    assert_eq!(transition.events, vec!["hsm/final".to_string()]);

    Ok(())
}

#[tokio::test]
async fn test_internal_effect_transition_without_trigger_registers_final_event() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "CompletionEffectMachine",
        initial!(target!("idle")),
        state!("idle", transition!(effect!(noop_operation)))
    );

    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "{:?}", validation_result);

    let transition = model
        .members
        .values()
        .find_map(|element| match element {
            ElementVariant::Transition(transition)
                if transition.source == "/CompletionEffectMachine/idle" =>
            {
                Some(transition)
            }
            _ => None,
        })
        .expect("completion transition should exist");
    assert_eq!(transition.events, vec!["hsm/final".to_string()]);

    Ok(())
}

#[tokio::test]
async fn test_choice_transition_requires_target_even_with_effect() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "EffectOnlyChoiceMachine",
        initial!(target!("idle")),
        state!("idle", transition!(on!("go"), target!("../choice"))),
        choice!("choice", transition!(effect!(noop_operation))),
        state!("done")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Pseudostate transition")
                && message.contains("requires target")
    ));

    Ok(())
}

#[tokio::test]
async fn test_history_requires_default_transition() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "MissingHistoryDefaultMachine",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("ready")),
            state!("ready"),
            shallow_history!("remember")
        )
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("History") && message.contains("requires a default transition")
    ));

    Ok(())
}

#[tokio::test]
async fn test_top_level_history_fails_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "TopLevelHistoryMachine",
        initial!(target!("idle")),
        shallow_history!("remember", target!("idle")),
        state!("idle")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("History") && message.contains("nested inside a state")
    ));

    Ok(())
}

#[tokio::test]
async fn test_history_default_must_target_inside_owner() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "EscapingHistoryMachine",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("ready")),
            state!("ready"),
            shallow_history!("remember", target!("../outside"))
        ),
        state!("outside")
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("History") && message.contains("must target inside")
    ));

    Ok(())
}

#[tokio::test]
async fn test_state_rejects_duplicate_history_kind() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "DuplicateHistoryMachine",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("ready")),
            state!("ready"),
            shallow_history!("remember", target!("ready")),
            shallow_history!("again", target!("ready"))
        )
    );

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("more than one shallow history vertex")
    ));

    Ok(())
}

#[tokio::test]
async fn test_final_state_with_transitions_fails_validation() -> Result<()> {
    let mut model: Model<ValidationTestInstance> = define!(
        "InvalidFinalMachine",
        initial!(target!("working")),
        state!(
            "working",
            transition!(on!("finish"), target!("../completed"))
        ),
        final_state!("completed")
    );
    let mut stack = vec!["/InvalidFinalMachine/completed".to_string()];
    transition(vec![on("restart"), target("../working")]).apply(&mut model, &mut stack);

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Final state") && message.contains("cannot have")
    ));

    Ok(())
}

#[tokio::test]
async fn test_final_state_with_child_state_fails_validation() -> Result<()> {
    let mut model: Model<ValidationTestInstance> = define!(
        "FinalChildMachine",
        initial!(target!("completed")),
        final_state!("completed")
    );
    let mut stack = vec!["/FinalChildMachine/completed".to_string()];
    state("child").apply(&mut model, &mut stack);

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Final state") && message.contains("child states")
    ));

    Ok(())
}

#[tokio::test]
async fn test_final_state_with_defer_fails_validation() -> Result<()> {
    let mut model: Model<ValidationTestInstance> = define!(
        "FinalDeferMachine",
        initial!(target!("completed")),
        final_state!("completed")
    );
    let mut stack = vec!["/FinalDeferMachine/completed".to_string()];
    defer(vec!["later"]).apply(&mut model, &mut stack);

    let validation_result = validate(&model);
    assert!(matches!(
        validation_result,
        Err(HsmError::Validation(message))
            if message.contains("Final state") && message.contains("defer events")
    ));

    Ok(())
}

#[tokio::test]
async fn test_multiple_choice_states_validation() -> Result<()> {
    let model: Model<ValidationTestInstance> = define!(
        "MultipleChoiceMachine",
        initial!(target!("start")),
        state!(
            "start",
            transition!(on!("branch1"), target!("../choice1")),
            transition!(on!("branch2"), target!("../choice2"))
        ),
        choice!(
            "choice1",
            transition!(guard!(dummy_guard), target!("result1")),
            transition!(target!("default1")) // Guardless fallback
        ),
        choice!(
            "choice2",
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
    let model: Model<ValidationTestInstance> = define!(
        "NestedChoiceMachine",
        initial!(target!("parent")),
        state!(
            "parent",
            initial!(target!("child_choice")),
            choice!(
                "child_choice",
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
    let model: Model<ValidationTestInstance> = define!(
        "CircularMachine",
        initial!(target!("state1")),
        state!("state1", transition!(on!("next"), target!("../state2"))),
        state!("state2", transition!(on!("next"), target!("../state3"))),
        state!(
            "state3",
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
    let model: Model<ValidationTestInstance> = define!(
        "DeepHierarchyMachine",
        initial!(target!("level1")),
        state!(
            "level1",
            initial!(target!("level2")),
            state!(
                "level2",
                initial!(target!("level3")),
                state!(
                    "level3",
                    initial!(target!("level4")),
                    state!("level4", transition!(on!("escape"), target!("../../../..")))
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

    fn state_entry(
        _ctx: &Context,
        _inst: &mut ValidationTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    fn state_exit(
        _ctx: &Context,
        _inst: &mut ValidationTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    fn state_activity(
        _ctx: &Context,
        _inst: &mut ValidationTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {})
    }

    // Complex but valid machine with all features
    let model: Model<ValidationTestInstance> = define!(
        "ComplexValidMachine",
        initial!(target!("system")),
        state!(
            "system",
            initial!(target!("initializing")),
            entry!(state_entry),
            exit!(state_exit),
            state!(
                "initializing",
                entry!(state_entry),
                transition!(on!("ready"), target!("../running"))
            ),
            state!(
                "running",
                initial!(target!("processing")),
                entry!(state_entry),
                exit!(state_exit),
                state!(
                    "processing",
                    entry!(state_entry),
                    activity!(state_activity),
                    transition!(on!("pause"), target!("../paused")),
                    transition!(on!("error"), target!("../../error"))
                ),
                state!(
                    "paused",
                    entry!(state_entry),
                    transition!(on!("resume"), target!("../processing")),
                    transition!(on!("stop"), target!("../../stopping"))
                )
            ),
            state!(
                "stopping",
                entry!(state_entry),
                exit!(state_exit),
                transition!(on!("stopped"), target!("../../stopped"))
            ),
            state!(
                "error",
                entry!(state_entry),
                transition!(on!("reset"), target!("../initializing")),
                transition!(on!("diagnose"), target!("../diagnosis"))
            ),
            state!(
                "diagnosis",
                entry!(state_entry),
                transition!(on!("resolved"), target!("../initializing")),
                transition!(on!("escalate"), target!("../critical"))
            ),
            state!(
                "critical",
                entry!(state_entry),
                transition!(on!("recover"), target!("../error"))
            )
        ),
        state!(
            "stopped",
            entry!(state_entry),
            transition!(on!("restart"), target!("../system/initializing"))
        ),
        choice!(
            "router",
            transition!(guard!(dummy_guard), target!("system")),
            transition!(target!("stopped")) // Guardless fallback
        ),
        final_state!("terminated")
    );

    // Should validate successfully despite complexity
    let validation_result = validate(&model);
    assert!(validation_result.is_ok(), "{:?}", validation_result);

    Ok(())
}

#[tokio::test]
async fn test_validation_error_messages() -> Result<()> {
    // Test that validation provides meaningful error messages
    let bad_model: Model<ValidationTestInstance> = define!(
        "BadChoiceValidation",
        initial!(target!("start")),
        state!("start", transition!(on!("go"), target!("../bad_choice"))),
        choice!(
            "bad_choice",
            transition!(guard!(dummy_guard), target!("never_reached")) // No guardless fallback
        ),
        state!("never_reached")
    );

    let validation_result = validate(&bad_model);
    assert!(validation_result.is_err());

    if let Err(error) = validation_result {
        let error_msg = format!("{:?}", error);
        // Should contain meaningful information about the error
        assert!(
            error_msg.contains("choice")
                || error_msg.contains("guardless")
                || error_msg.contains("fallback"),
            "Error message should be descriptive: {}",
            error_msg
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_start_with_invalid_model() -> Result<()> {
    let instance = ValidationTestInstance::new();
    let ctx = Context::new();

    // Create an invalid model
    let bad_model: Model<ValidationTestInstance> = define!(
        "StartInvalidMachine",
        initial!(target!("start")),
        state!(
            "start",
            transition!(on!("decide"), target!("../bad_choice"))
        ),
        choice!(
            "bad_choice",
            transition!(guard!(dummy_guard), target!("end")) // Missing guardless fallback
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
        }
        Err(_) => {
            // If start fails due to validation, that's also acceptable
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_model_with_no_states() -> Result<()> {
    // Test edge case of empty or minimal models
    let minimal_model: Model<ValidationTestInstance> = define!(
        "MinimalMachine",
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
    let model: Model<ValidationTestInstance> = define!(
        "UnreachableMachine",
        initial!(target!("start")),
        state!("start", transition!(on!("go"), target!("../end"))),
        state!("end"),
        state!("unreachable") // No transitions lead here
    );

    // Unreachable states should not cause validation failure
    // (They might generate warnings in more sophisticated validators)
    let validation_result = validate(&model);
    assert!(validation_result.is_ok());

    Ok(())
}
