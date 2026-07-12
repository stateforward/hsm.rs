use stateforward_hsm::*;
/**
 * @fileoverview Test hierarchical states and nested state transitions
 * Tests complex state hierarchies, entry/exit order, and transitions between nested states
 */
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
pub struct HierarchicalTestInstance {
    pub log: Vec<String>,
    pub data: std::collections::HashMap<String, String>,
}

impl HierarchicalTestInstance {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            data: std::collections::HashMap::new(),
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.log.push(action.to_string());
    }
}

impl Instance for HierarchicalTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Entry/Exit functions for hierarchy testing
fn parent_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("parent-entry");
    Box::pin(async move {})
}

fn parent_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("parent-exit");
    Box::pin(async move {})
}

fn child1_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child1-entry");
    Box::pin(async move {})
}

fn child1_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child1-exit");
    Box::pin(async move {})
}

fn child2_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child2-entry");
    Box::pin(async move {})
}

fn child2_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("child2-exit");
    Box::pin(async move {})
}

fn level1_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level1-entry");
    Box::pin(async move {})
}

fn level1_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level1-exit");
    Box::pin(async move {})
}

fn level2_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level2-entry");
    Box::pin(async move {})
}

fn level2_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level2-exit");
    Box::pin(async move {})
}

fn level3_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level3-entry");
    Box::pin(async move {})
}

fn level3_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level3-exit");
    Box::pin(async move {})
}

fn level4_entry(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level4-entry");
    Box::pin(async move {})
}

fn level4_exit(
    _ctx: &Context,
    inst: &mut HierarchicalTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("level4-exit");
    Box::pin(async move {})
}

#[tokio::test]
async fn test_simple_hierarchical_states() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "HierarchicalMachine",
        initial!(target!("parent")),
        state!(
            "parent",
            initial!(target!("child1")),
            entry!(parent_entry),
            exit!(parent_exit),
            state!(
                "child1",
                entry!(child1_entry),
                exit!(child1_exit),
                transition!(on!("next"), target!("../child2"))
            ),
            state!("child2", entry!(child2_entry), exit!(child2_exit))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should enter parent first, then child1
    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(inst.log, vec!["parent-entry", "child1-entry"]);
    }
    assert_eq!(hsm.current_state(), "/HierarchicalMachine/parent/child1");

    // Transition between siblings
    let next_event = Event::new("next");
    hsm.dispatch(&ctx, next_event).await?;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(
            inst.log,
            vec![
                "parent-entry",
                "child1-entry",
                "child1-exit",
                "child2-entry"
            ]
        );
    }
    assert_eq!(hsm.current_state(), "/HierarchicalMachine/parent/child2");

    Ok(())
}

#[tokio::test]
async fn test_deep_hierarchy() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    let model = define!(
        "DeepHierarchy",
        initial!(target!("level1")),
        state!(
            "level1",
            initial!(target!("level2")),
            entry!(level1_entry),
            exit!(level1_exit),
            state!(
                "level2",
                initial!(target!("level3")),
                entry!(level2_entry),
                exit!(level2_exit),
                state!(
                    "level3",
                    initial!(target!("level4")),
                    entry!(level3_entry),
                    exit!(level3_exit),
                    state!("level4", entry!(level4_entry), exit!(level4_exit))
                )
            )
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should enter all levels in order
    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(
            inst.log,
            vec![
                "level1-entry",
                "level2-entry",
                "level3-entry",
                "level4-entry"
            ]
        );
    }
    assert_eq!(
        hsm.current_state(),
        "/DeepHierarchy/level1/level2/level3/level4"
    );

    Ok(())
}

#[tokio::test]
async fn test_cross_level_transitions() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    fn a_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("a-entry");
        Box::pin(async move {})
    }

    fn a_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("a-exit");
        Box::pin(async move {})
    }

    fn a1_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("a1-entry");
        Box::pin(async move {})
    }

    fn a1_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("a1-exit");
        Box::pin(async move {})
    }

    fn a2_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("a2-entry");
        Box::pin(async move {})
    }

    fn b_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("b-entry");
        Box::pin(async move {})
    }

    fn b_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("b-exit");
        Box::pin(async move {})
    }

    fn b2_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("b2-entry");
        Box::pin(async move {})
    }

    fn b2_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("b2-exit");
        Box::pin(async move {})
    }

    let model = define!(
        "CrossLevelMachine",
        initial!(target!("a")),
        state!(
            "a",
            initial!(target!("a1")),
            entry!(a_entry),
            exit!(a_exit),
            state!(
                "a1",
                entry!(a1_entry),
                exit!(a1_exit),
                transition!(on!("toB"), target!("../../b/b2"))
            ),
            state!("a2", entry!(a2_entry))
        ),
        state!(
            "b",
            initial!(target!("b1")),
            entry!(b_entry),
            exit!(b_exit),
            state!("b1"),
            state!(
                "b2",
                entry!(b2_entry),
                exit!(b2_exit),
                transition!(on!("toA"), target!("../../a/a2"))
            )
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear(); // Clear initial entries
    }

    // Transition from a1 to b2
    let to_b_event = Event::new("toB");
    hsm.dispatch(&ctx, to_b_event).await;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(inst.log, vec!["a1-exit", "a-exit", "b-entry", "b2-entry"]);
    }
    assert_eq!(hsm.current_state(), "/CrossLevelMachine/b/b2");

    // Transition from b2 to a2
    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear();
    }
    let to_a_event = Event::new("toA");
    hsm.dispatch(&ctx, to_a_event).await;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(inst.log, vec!["b2-exit", "b-exit", "a-entry", "a2-entry"]);
    }
    assert_eq!(hsm.current_state(), "/CrossLevelMachine/a/a2");

    Ok(())
}

#[tokio::test]
async fn test_local_transitions_within_hierarchy() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    fn container_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("container-entry");
        Box::pin(async move {})
    }

    fn container_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("container-exit");
        Box::pin(async move {})
    }

    fn inner_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("inner-entry");
        Box::pin(async move {})
    }

    fn inner_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("inner-exit");
        Box::pin(async move {})
    }

    let model = define!(
        "LocalTransitionMachine",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("inner")),
            entry!(container_entry),
            exit!(container_exit),
            state!("inner", entry!(inner_entry), exit!(inner_exit)),
            transition!(on!("restart"), target!("inner"))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;
    assert_eq!(
        hsm.current_state(),
        "/LocalTransitionMachine/container/inner"
    );

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear();
    }

    // Local transition should exit inner and re-enter it
    let restart_event = Event::new("restart");
    eprintln!("Before restart: state = {}", hsm.current_state());
    hsm.dispatch(&ctx, restart_event).await;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(inst.log, vec!["inner-exit", "inner-entry"]);
    }
    assert_eq!(
        hsm.current_state(),
        "/LocalTransitionMachine/container/inner"
    );

    Ok(())
}

#[tokio::test]
async fn test_event_bubbling_in_hierarchical_states() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    fn outer_handled_bubble(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("outer-handled-bubble");
        Box::pin(async move {})
    }

    fn inner_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("inner-entry");
        Box::pin(async move {})
    }

    fn handled_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("handled-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "EventBubblingMachine",
        initial!(target!("outer")),
        state!(
            "outer",
            initial!(target!("middle")),
            transition!(
                on!("bubble"),
                target!("handled"),
                effect!(outer_handled_bubble)
            ),
            state!(
                "middle",
                initial!(target!("inner")),
                state!("inner", entry!(inner_entry))
            ),
            state!("handled", entry!(handled_entry))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;
    assert_eq!(
        hsm.current_state(),
        "/EventBubblingMachine/outer/middle/inner"
    );

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear();
    }

    // Event should bubble up from inner through middle to outer
    let bubble_event = Event::new("bubble");
    hsm.dispatch(&ctx, bubble_event).await;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(inst.log, vec!["outer-handled-bubble", "handled-entry"]);
    }
    assert_eq!(hsm.current_state(), "/EventBubblingMachine/outer/handled");

    Ok(())
}

#[tokio::test]
async fn test_multiple_parallel_hierarchies() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    fn branch1_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("branch1-entry");
        Box::pin(async move {})
    }

    fn branch1_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("branch1-exit");
        Box::pin(async move {})
    }

    fn leaf1_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("leaf1-entry");
        Box::pin(async move {})
    }

    fn leaf1_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("leaf1-exit");
        Box::pin(async move {})
    }

    fn branch2_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("branch2-entry");
        Box::pin(async move {})
    }

    fn branch2_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("branch2-exit");
        Box::pin(async move {})
    }

    fn leaf2_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("leaf2-entry");
        Box::pin(async move {})
    }

    fn leaf2_exit(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("leaf2-exit");
        Box::pin(async move {})
    }

    let model = define!(
        "ParallelHierarchies",
        initial!(target!("branch1")),
        state!(
            "branch1",
            initial!(target!("leaf1")),
            entry!(branch1_entry),
            exit!(branch1_exit),
            state!(
                "leaf1",
                entry!(leaf1_entry),
                exit!(leaf1_exit),
                transition!(on!("switch"), target!("../../branch2/leaf2"))
            )
        ),
        state!(
            "branch2",
            initial!(target!("leaf2")),
            entry!(branch2_entry),
            exit!(branch2_exit),
            state!(
                "leaf2",
                entry!(leaf2_entry),
                exit!(leaf2_exit),
                transition!(on!("switch"), target!("../../branch1/leaf1"))
            )
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;
    assert_eq!(hsm.current_state(), "/ParallelHierarchies/branch1/leaf1");

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear();
    }

    // Switch between branches
    let switch_event = Event::new("switch");
    hsm.dispatch(&ctx, switch_event).await?;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(
            inst.log,
            vec!["leaf1-exit", "branch1-exit", "branch2-entry", "leaf2-entry"]
        );
    }
    assert_eq!(hsm.current_state(), "/ParallelHierarchies/branch2/leaf2");

    // Switch back
    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        inst.log.clear();
    }
    let switch_back_event = Event::new("switch");
    hsm.dispatch(&ctx, switch_back_event).await?;

    {
        let mut binding = hsm.instance_mut();
        let inst = binding
            .as_any_mut()
            .downcast_mut::<HierarchicalTestInstance>()
            .unwrap();
        assert_eq!(
            inst.log,
            vec!["leaf2-exit", "branch2-exit", "branch1-entry", "leaf1-entry"]
        );
    }
    assert_eq!(hsm.current_state(), "/ParallelHierarchies/branch1/leaf1");

    Ok(())
}

#[tokio::test]
async fn test_absolute_vs_relative_path_targeting() -> Result<()> {
    let instance = HierarchicalTestInstance::new();
    let ctx = Context::new();

    fn other_entry(
        _ctx: &Context,
        inst: &mut HierarchicalTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("other-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "PathTargetingMachine",
        initial!(target!("root")),
        state!(
            "root",
            initial!(target!("child")),
            state!(
                "child",
                initial!(target!("grandchild")),
                state!("grandchild", transition!(on!("relative"), target!(".."))),
                transition!(on!("absolute"), target!("/PathTargetingMachine/root/other"))
            ),
            state!("other", entry!(other_entry))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;
    assert_eq!(
        hsm.current_state(),
        "/PathTargetingMachine/root/child/grandchild"
    );

    // Test relative path
    let relative_event = Event::new("relative");
    hsm.dispatch(&ctx, relative_event).await?;
    assert_eq!(hsm.current_state(), "/PathTargetingMachine/root/child");

    // Test absolute path
    let absolute_event = Event::new("absolute");
    hsm.dispatch(&ctx, absolute_event).await?;
    assert_eq!(hsm.current_state(), "/PathTargetingMachine/root/other");

    Ok(())
}
