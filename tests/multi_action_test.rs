use rust::*;
/**
 * @fileoverview Tests for multiple action behaviors in HSM
 * Tests entry, exit, effect, and activity macros with multiple functions
 */
use std::time::Duration;

// Test instance for multiple action tests
#[derive(Debug)]
pub struct MultiActionTestInstance {
    pub action_log: Vec<String>,
    pub action_count: i32,
    pub state_transitions: Vec<String>,
    pub activities_started: Vec<String>,
    pub cleanup_tasks: Vec<String>,
}

impl MultiActionTestInstance {
    pub fn new() -> Self {
        Self {
            action_log: Vec::new(),
            action_count: 0,
            state_transitions: Vec::new(),
            activities_started: Vec::new(),
            cleanup_tasks: Vec::new(),
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.action_log.push(action.to_string());
        self.action_count += 1;
    }

    pub fn log_transition(&mut self, transition: &str) {
        self.state_transitions.push(transition.to_string());
    }

    pub fn start_activity(&mut self, activity: &str) {
        self.activities_started.push(activity.to_string());
    }

    pub fn add_cleanup_task(&mut self, task: &str) {
        self.cleanup_tasks.push(task.to_string());
    }
}

impl Instance for MultiActionTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Entry action functions
fn entry_setup(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("entry_setup");
    Box::pin(async move {})
}

fn entry_configure(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("entry_configure");
    Box::pin(async move {})
}

fn entry_start(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("entry_start");
    Box::pin(async move {})
}

// Exit action functions
fn exit_cleanup(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("exit_cleanup");
    inst.add_cleanup_task("cleanup");
    Box::pin(async move {})
}

fn exit_save(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("exit_save");
    inst.add_cleanup_task("save");
    Box::pin(async move {})
}

fn exit_shutdown(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("exit_shutdown");
    inst.add_cleanup_task("shutdown");
    Box::pin(async move {})
}

// Effect action functions
fn effect_validate(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("effect_validate");
    inst.log_transition("validate");
    Box::pin(async move {})
}

fn effect_transform(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("effect_transform");
    inst.log_transition("transform");
    Box::pin(async move {})
}

fn effect_notify(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("effect_notify");
    inst.log_transition("notify");
    Box::pin(async move {})
}

// Activity functions
fn activity_monitor(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("activity_monitor");
    inst.start_activity("monitor");
    Box::pin(async move {
        // Simulate monitoring work
        tokio::time::sleep(Duration::from_millis(1)).await;
    })
}

fn activity_process(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("activity_process");
    inst.start_activity("process");
    Box::pin(async move {
        // Simulate processing work
        tokio::time::sleep(Duration::from_millis(1)).await;
    })
}

fn activity_report(
    _ctx: &Context,
    inst: &mut MultiActionTestInstance,
    _event: &Event,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    inst.log_action("activity_report");
    inst.start_activity("report");
    Box::pin(async move {
        // Simulate reporting work
        tokio::time::sleep(Duration::from_millis(1)).await;
    })
}

#[test]
fn test_multiple_entry_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test multiple entry actions in single macro
    let model: Model<MultiActionTestInstance> = define!(
        "MultiEntryMachine",
        initial!(target!("active")),
        state!("active", entry!(entry_setup, entry_configure, entry_start))
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Multi-entry machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-entry machine should start");
}

#[test]
fn test_multiple_exit_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test multiple exit actions in single macro
    let model: Model<MultiActionTestInstance> = define!(
        "MultiExitMachine",
        initial!(target!("running")),
        state!(
            "running",
            exit!(exit_cleanup, exit_save, exit_shutdown),
            transition!(on!("stop"), target!("../stopped"))
        ),
        state!("stopped")
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Multi-exit machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-exit machine should start");
}

#[test]
fn test_multiple_effect_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test multiple effect actions in single macro
    let model: Model<MultiActionTestInstance> = define!(
        "MultiEffectMachine",
        initial!(target!("idle")),
        state!(
            "idle",
            transition!(
                on!("process"),
                effect!(effect_validate, effect_transform, effect_notify),
                target!("../done")
            )
        ),
        state!("done")
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Multi-effect machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-effect machine should start");
}

#[tokio::test]
async fn test_multiple_activity_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test multiple activity actions in single macro
    let model: Model<MultiActionTestInstance> = define!(
        "MultiActivityMachine",
        initial!(target!("working")),
        state!(
            "working",
            activity!(activity_monitor, activity_process, activity_report),
            transition!(on!("complete"), target!("../finished"))
        ),
        state!("finished")
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Multi-activity machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Multi-activity machine should start");

    // Allow activities to start
    tokio::time::sleep(Duration::from_millis(10)).await;
}

#[test]
fn test_comprehensive_multi_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test state with multiple actions of different types
    let model: Model<MultiActionTestInstance> = define!(
        "ComprehensiveMachine",
        initial!(target!("processing")),
        state!(
            "processing",
            entry!(entry_setup, entry_configure),
            exit!(exit_cleanup, exit_save),
            activity!(activity_monitor, activity_process),
            transition!(
                on!("complete"),
                effect!(effect_validate, effect_notify),
                target!("../completed")
            )
        ),
        state!("completed")
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Comprehensive machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Comprehensive machine should start");
}

#[test]
fn test_mixed_action_syntax() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test mixing old and new syntax (both should work)
    let model: Model<MultiActionTestInstance> = define!(
        "MixedSyntaxMachine",
        initial!(target!("state1")),
        state!(
            "state1",
            entry!(entry_setup),            // Single action (old syntax)
            exit!(exit_cleanup, exit_save), // Multiple actions (new syntax)
            transition!(on!("next"), target!("../state2"))
        ),
        state!(
            "state2",
            entry!(entry_configure, entry_start), // Multiple actions (new syntax)
            exit!(exit_shutdown),                 // Single action (new syntax but works)
            transition!(
                on!("process"),
                effect!(effect_validate), // Single effect
                target!("../state3")
            )
        ),
        state!(
            "state3",
            transition!(
                on!("multi_effect"),
                effect!(effect_transform, effect_notify), // Multiple effects
                target!("../state1")
            )
        )
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Mixed syntax machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(hsm_result.is_ok(), "Mixed syntax machine should start");
}

#[test]
fn test_macro_variations() {
    let _instance = MultiActionTestInstance::new();

    // Test all macro variations compile
    let _entry1: Box<dyn PartialElement<MultiActionTestInstance>> = entry!(entry_setup);
    let _entry2: Box<dyn PartialElement<MultiActionTestInstance>> =
        entry!(entry_setup, entry_configure);
    let _entry3: Box<dyn PartialElement<MultiActionTestInstance>> =
        entry!(entry_setup, entry_configure, entry_start);

    let _exit1: Box<dyn PartialElement<MultiActionTestInstance>> = exit!(exit_cleanup);
    let _exit2: Box<dyn PartialElement<MultiActionTestInstance>> = exit!(exit_cleanup, exit_save);
    let _exit3: Box<dyn PartialElement<MultiActionTestInstance>> =
        exit!(exit_cleanup, exit_save, exit_shutdown);

    let _effect1: Box<dyn PartialElement<MultiActionTestInstance>> = effect!(effect_validate);
    let _effect2: Box<dyn PartialElement<MultiActionTestInstance>> =
        effect!(effect_validate, effect_transform);
    let _effect3: Box<dyn PartialElement<MultiActionTestInstance>> =
        effect!(effect_validate, effect_transform, effect_notify);

    let _activity1: Box<dyn PartialElement<MultiActionTestInstance>> = activity!(activity_monitor);
    let _activity2: Box<dyn PartialElement<MultiActionTestInstance>> =
        activity!(activity_monitor, activity_process);
    let _activity3: Box<dyn PartialElement<MultiActionTestInstance>> =
        activity!(activity_monitor, activity_process, activity_report);

    // All variations should compile
    assert!(true, "All macro variations should compile");
}

#[test]
fn test_hierarchical_multi_actions() {
    let instance = MultiActionTestInstance::new();
    let ctx = Context::new();

    // Test hierarchical states with multiple actions
    let model: Model<MultiActionTestInstance> = define!(
        "HierarchicalMultiMachine",
        initial!(target!("system/subsystem")),
        state!(
            "system",
            entry!(entry_setup, entry_configure),
            exit!(exit_cleanup, exit_save),
            state!(
                "subsystem",
                entry!(entry_start),
                exit!(exit_shutdown),
                activity!(activity_monitor, activity_process)
            )
        )
    );

    let validation_result = validate(&model);
    assert!(
        validation_result.is_ok(),
        "Hierarchical multi-action machine should validate"
    );

    let hsm_result = start(&ctx, instance, model);
    assert!(
        hsm_result.is_ok(),
        "Hierarchical multi-action machine should start"
    );
}

#[test]
fn test_function_signatures() {
    // Test that all function signatures are correct for multiple action support

    // Entry functions: (ctx, inst, event) -> Pin<Box<dyn Future<Output = ()> + Send>>
    let _entry_fn: fn(
        &Context,
        &mut MultiActionTestInstance,
        &Event,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = entry_setup;

    // Exit functions: same signature
    let _exit_fn: fn(
        &Context,
        &mut MultiActionTestInstance,
        &Event,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = exit_cleanup;

    // Effect functions: same signature
    let _effect_fn: fn(
        &Context,
        &mut MultiActionTestInstance,
        &Event,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = effect_validate;

    // Activity functions: same signature
    let _activity_fn: fn(
        &Context,
        &mut MultiActionTestInstance,
        &Event,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = activity_monitor;

    // All signatures should match the reference exactly
    assert!(true, "All function signatures should match reference");
}

#[test]
fn test_action_patterns() {
    // Test common multi-action patterns
    let instance = MultiActionTestInstance::new();

    // Pattern 1: Sequential entry actions (setup -> configure -> start)
    let model1: Model<MultiActionTestInstance> = define!(
        "SequentialPattern",
        initial!(target!("init")),
        state!("init", entry!(entry_setup, entry_configure, entry_start))
    );

    // Pattern 2: Cleanup exit actions (save -> cleanup -> shutdown)
    let model2: Model<MultiActionTestInstance> = define!(
        "CleanupPattern",
        initial!(target!("running")),
        state!("running", exit!(exit_save, exit_cleanup, exit_shutdown))
    );

    // Pattern 3: Transition pipeline (validate -> transform -> notify)
    let model3: Model<MultiActionTestInstance> = define!(
        "PipelinePattern",
        initial!(target!("start")),
        state!(
            "start",
            transition!(
                on!("process"),
                effect!(effect_validate, effect_transform, effect_notify),
                target!("../end")
            )
        ),
        state!("end")
    );

    // All patterns should validate
    assert!(
        validate(&model1).is_ok(),
        "Sequential pattern should validate"
    );
    assert!(validate(&model2).is_ok(), "Cleanup pattern should validate");
    assert!(
        validate(&model3).is_ok(),
        "Pipeline pattern should validate"
    );
}
