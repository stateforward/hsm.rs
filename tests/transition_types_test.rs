use rust::*;
/**
 * @fileoverview Test different types of transitions
 * Tests external, internal, self, and local transitions with proper entry/exit behavior
 */
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
pub struct TransitionTestInstance {
    pub log: Vec<String>,
    pub counter: i32,
}

impl TransitionTestInstance {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            counter: 0,
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.log.push(action.to_string());
    }

    pub fn increment(&mut self) {
        self.counter += 1;
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }
}

impl Instance for TransitionTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Entry/Exit functions
fn state_entry(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.increment();
    inst.log_action(&format!("state-entry-{}", inst.counter));
    Box::pin(async move {})
}

fn state_exit(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action(&format!("state-exit-{}", inst.counter));
    inst.reset();
    Box::pin(async move {})
}

fn parent_entry(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("parent-entry");
    Box::pin(async move {})
}

fn parent_exit(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("parent-exit");
    Box::pin(async move {})
}

fn child_entry(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child-entry");
    Box::pin(async move {})
}

fn child_exit(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child-exit");
    Box::pin(async move {})
}

fn sibling_entry(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("sibling-entry");
    Box::pin(async move {})
}

fn sibling_exit(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("sibling-exit");
    Box::pin(async move {})
}

// Effect functions
fn external_effect(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("external-effect");
    Box::pin(async move {})
}

fn internal_effect(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("internal-effect");
    Box::pin(async move {})
}

fn self_effect(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("self-effect");
    Box::pin(async move {})
}

fn local_effect(
    _ctx: &Context,
    inst: &mut TransitionTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("local-effect");
    Box::pin(async move {})
}

#[tokio::test]
async fn test_external_transitions() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "ExternalTransitionMachine",
        initial!(target!("state1")),
        state!(
            "state1",
            entry!(state_entry),
            exit!(state_exit),
            transition!(
                on!("external"),
                target!("../state2"),
                effect!(external_effect)
            )
        ),
        state!(
            "state2",
            entry!(state_entry),
            exit!(state_exit),
            transition!(on!("back"), target!("../state1"), effect!(external_effect))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["state-entry-1"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    // External transition should exit source and enter target
    let external_event = Event::new("external");
    hsm.dispatch(&ctx, external_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "state-entry-1",
            "state-exit-1",
            "external-effect",
            "state-entry-1"
        ]
    );
    assert_eq!(hsm.state(), "/ExternalTransitionMachine/state2");

    Ok(())
}

#[tokio::test]
async fn test_internal_transitions() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "InternalTransitionMachine",
        initial!(target!("active")),
        state!(
            "active",
            entry!(state_entry),
            exit!(state_exit),
            transition!(on!("internal"), effect!(internal_effect))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["state-entry-1"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    // Internal transition should NOT exit/enter the state
    let internal_event = Event::new("internal");
    hsm.dispatch(&ctx, internal_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["state-entry-1", "internal-effect"]);
    assert_eq!(inst.counter, 1); // Should not re-enter
    assert_eq!(hsm.state(), "/InternalTransitionMachine/active");

    Ok(())
}

#[tokio::test]
async fn test_self_transitions() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "SelfTransitionMachine",
        initial!(target!("counter")),
        state!(
            "counter",
            entry!(state_entry),
            exit!(state_exit),
            transition!(on!("self"), target!("."), effect!(self_effect))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entry
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["state-entry-1"]);
    assert_eq!(inst.counter, 1);
    drop(instance);

    // Self transition should exit and re-enter the same state
    let self_event = Event::new("self");
    hsm.dispatch(&ctx, self_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "state-entry-1",
            "state-exit-1",
            "self-effect",
            "state-entry-1"
        ]
    );
    assert_eq!(inst.counter, 1); // Reset then incremented again
    assert_eq!(hsm.state(), "/SelfTransitionMachine/counter");

    Ok(())
}

#[tokio::test]
async fn test_local_transitions() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "LocalTransitionMachine",
        initial!(target!("parent")),
        state!(
            "parent",
            initial!(target!("child")),
            entry!(parent_entry),
            exit!(parent_exit),
            state!(
                "child",
                entry!(child_entry),
                exit!(child_exit),
                transition!(on!("local"), target!("../sibling"), effect!(local_effect))
            ),
            state!("sibling", entry!(sibling_entry), exit!(sibling_exit))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initial entries
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["parent-entry", "child-entry"]);
    drop(instance);

    // Local transition should not exit parent, just transition between children
    let local_event = Event::new("local");
    hsm.dispatch(&ctx, local_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "parent-entry",
            "child-entry",
            "child-exit",
            "local-effect",
            "sibling-entry"
        ]
    );
    assert_eq!(hsm.state(), "/LocalTransitionMachine/parent/sibling");

    Ok(())
}

#[tokio::test]
async fn test_transition_to_nested_child() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    fn outer_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("outer-entry");
        Box::pin(async move {})
    }

    fn outer_exit(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("outer-exit");
        Box::pin(async move {})
    }

    fn inner_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("inner-entry");
        Box::pin(async move {})
    }

    fn deep_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("deep-entry");
        Box::pin(async move {})
    }

    fn deep_exit(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("deep-exit");
        Box::pin(async move {})
    }

    let model = define!(
        "NestedTransitionMachine",
        initial!(target!("start")),
        state!(
            "start",
            transition!(on!("dive"), target!("../outer/inner/deep"))
        ),
        state!(
            "outer",
            entry!(outer_entry),
            exit!(outer_exit),
            state!(
                "inner",
                entry!(inner_entry),
                state!(
                    "deep",
                    entry!(deep_entry),
                    exit!(deep_exit),
                    transition!(on!("surface"), target!("../../../start"))
                )
            )
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Transition to deeply nested state
    let dive_event = Event::new("dive");
    hsm.dispatch(&ctx, dive_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["outer-entry", "inner-entry", "deep-entry"]);
    assert_eq!(hsm.state(), "/NestedTransitionMachine/outer/inner/deep");
    drop(instance);

    // Transition back to start - should exit all nested states
    let surface_event = Event::new("surface");
    hsm.dispatch(&ctx, surface_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "outer-entry",
            "inner-entry",
            "deep-entry",
            "deep-exit",
            "outer-exit"
        ]
    );
    assert_eq!(hsm.state(), "/NestedTransitionMachine/start");

    Ok(())
}

#[tokio::test]
async fn test_transition_types_with_guards() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    fn counter_guard(_ctx: &Context, inst: &TransitionTestInstance, _event: &Event) -> bool {
        inst.counter > 3
    }

    let model = define!(
        "GuardedTransitionMachine",
        initial!(target!("waiting")),
        state!(
            "waiting",
            entry!(state_entry),
            transition!(
                on!("try_external"),
                guard!(counter_guard),
                target!("../success"),
                effect!(external_effect)
            ),
            transition!(
                on!("try_external"),
                target!("../failed"),
                effect!(external_effect)
            ),
            transition!(
                on!("try_internal"),
                guard!(counter_guard),
                effect!(internal_effect)
            ),
            transition!(on!("increment"), effect!(state_entry))
        ),
        state!("success"),
        state!("failed")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Initially counter is 1, guard should fail
    let try_event = Event::new("try_external");
    hsm.dispatch(&ctx, try_event).await;
    assert_eq!(hsm.state(), "/GuardedTransitionMachine/failed");

    // Create new instance and model for second test
    let instance2 = TransitionTestInstance::new();
    let model2 = define!(
        "GuardedTransitionMachine2",
        initial!(target!("waiting")),
        state!(
            "waiting",
            entry!(state_entry),
            transition!(
                on!("try_external"),
                guard!(counter_guard),
                target!("../success"),
                effect!(external_effect)
            ),
            transition!(
                on!("try_external"),
                target!("../failed"),
                effect!(external_effect)
            ),
            transition!(
                on!("try_internal"),
                guard!(counter_guard),
                effect!(internal_effect)
            ),
            transition!(on!("increment"), effect!(state_entry))
        ),
        state!("success"),
        state!("failed")
    );
    let hsm2 = start(&ctx, instance2, model2)?;
    hsm2.start().await;

    for _ in 0..3 {
        let inc_event = Event::new("increment");
        hsm2.dispatch(&ctx, inc_event).await;
    }

    // Now counter should be 4, guard should pass
    let try_event2 = Event::new("try_external");
    hsm2.dispatch(&ctx, try_event2).await;
    assert_eq!(hsm2.state(), "/GuardedTransitionMachine2/success");

    Ok(())
}

#[tokio::test]
async fn test_already_active_ancestor_states() -> Result<()> {
    let instance = TransitionTestInstance::new();
    let ctx = Context::new();

    fn parent1_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("parent1-entry");
        Box::pin(async move {})
    }

    fn parent1_exit(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("parent1-exit");
        Box::pin(async move {})
    }

    fn child1_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("child1-entry");
        Box::pin(async move {})
    }

    fn child1_exit(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("child1-exit");
        Box::pin(async move {})
    }

    fn child2_entry(
        _ctx: &Context,
        inst: &mut TransitionTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("child2-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "AncestorActiveMachine",
        initial!(target!("parent1")),
        state!(
            "parent1",
            initial!(target!("child1")),
            entry!(parent1_entry),
            exit!(parent1_exit),
            state!(
                "child1",
                entry!(child1_entry),
                exit!(child1_exit),
                transition!(on!("to_child2"), target!("../child2"))
            ),
            state!(
                "child2",
                entry!(child2_entry),
                transition!(on!("to_parent1"), target!(".."))
            )
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["parent1-entry", "child1-entry"]);
    drop(instance);

    // Transition within parent1 - parent1 should NOT re-enter
    let to_child2_event = Event::new("to_child2");
    hsm.dispatch(&ctx, to_child2_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec![
            "parent1-entry",
            "child1-entry",
            "child1-exit",
            "child2-entry"
        ]
    );
    drop(instance);

    // Transition to parent1 from child2 - should exit child2 but NOT re-enter parent1
    let to_parent1_event = Event::new("to_parent1");
    hsm.dispatch(&ctx, to_parent1_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<TransitionTestInstance>()
        .unwrap();
    // Parent1 was already active, so no re-entry
    assert_eq!(
        inst.log,
        vec![
            "parent1-entry",
            "child1-entry",
            "child1-exit",
            "child2-entry" // No parent1-exit or parent1-entry here
        ]
    );

    Ok(())
}
