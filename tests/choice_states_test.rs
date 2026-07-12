use stateforward_hsm::*;
/**
 * @fileoverview Test choice pseudostates
 * Tests dynamic branching based on runtime conditions using choice pseudostates
 */
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
pub struct ChoiceTestInstance {
    pub log: Vec<String>,
    pub data: std::collections::HashMap<String, i32>,
    pub config: std::collections::HashMap<String, String>,
}

impl ChoiceTestInstance {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            data: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
        }
    }

    pub fn log_action(&mut self, action: &str) {
        self.log.push(action.to_string());
    }

    pub fn set_value(&mut self, value: i32) {
        self.data.insert("value".to_string(), value);
    }

    pub fn get_value(&self) -> i32 {
        *self.data.get("value").unwrap_or(&0)
    }

    pub fn set_direction(&mut self, direction: &str) {
        self.config
            .insert("direction".to_string(), direction.to_string());
    }

    pub fn get_direction(&self) -> &str {
        self.config
            .get("direction")
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

impl Instance for ChoiceTestInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Choice transition effects
fn going_to_choice(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("going-to-choice");
    Box::pin(async move {})
}

fn chose_low(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("chose-low");
    Box::pin(async move {})
}

fn chose_medium(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("chose-medium");
    Box::pin(async move {})
}

fn chose_high(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("chose-high");
    Box::pin(async move {})
}

// Entry actions
fn low_entry(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("low-entry");
    Box::pin(async move {})
}

fn medium_entry(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("medium-entry");
    Box::pin(async move {})
}

fn high_entry(
    _ctx: &Context,
    inst: &mut ChoiceTestInstance,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.log_action("high-entry");
    Box::pin(async move {})
}

// Guard functions
fn value_less_than_3(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
    inst.get_value() < 3
}

fn value_3_to_7(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
    let value = inst.get_value();
    value >= 3 && value < 7
}

fn direction_left(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
    inst.get_direction() == "left"
}

fn direction_right(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
    inst.get_direction() == "right"
}

#[tokio::test]
async fn test_basic_choice_pseudostate_with_guards() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    instance.set_value(5);
    let ctx = Context::new();

    let model = define!(
        "BasicChoiceMachine",
        initial!(target!("start")),
        state!(
            "start",
            transition!(
                on!("decide"),
                target!("../decision"),
                effect!(going_to_choice)
            )
        ),
        choice!(
            "decision",
            transition!(
                guard!(value_less_than_3),
                target!("low"),
                effect!(chose_low)
            ),
            transition!(
                guard!(value_3_to_7),
                target!("medium"),
                effect!(chose_medium)
            ),
            transition!(target!("high"), effect!(chose_high))
        ),
        state!("low", entry!(low_entry)),
        state!("medium", entry!(medium_entry)),
        state!("high", entry!(high_entry))
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Trigger choice evaluation
    let decide_event = Event::new("decide");
    hsm.dispatch(&ctx, decide_event).await;

    // Should choose medium branch
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(
        inst.log,
        vec!["going-to-choice", "chose-medium", "medium-entry"]
    );
    assert_eq!(hsm.state(), "/BasicChoiceMachine/medium");

    Ok(())
}

#[tokio::test]
async fn test_choice_with_different_guard_outcomes() -> Result<()> {
    let test_cases = vec![
        (1, "/ChoiceTestMachine/low", "chose-low"),
        (5, "/ChoiceTestMachine/medium", "chose-medium"),
        (10, "/ChoiceTestMachine/high", "chose-high"),
    ];

    for (value, expected_state, expected_effect) in test_cases {
        let mut instance = ChoiceTestInstance::new();
        instance.set_value(value);
        let ctx = Context::new();

        let model = define!(
            "ChoiceTestMachine",
            initial!(target!("choice")),
            choice!(
                "choice",
                transition!(
                    guard!(value_less_than_3),
                    target!("low"),
                    effect!(chose_low)
                ),
                transition!(
                    guard!(value_3_to_7),
                    target!("medium"),
                    effect!(chose_medium)
                ),
                transition!(target!("high"), effect!(chose_high))
            ),
            state!("low"),
            state!("medium"),
            state!("high")
        );

        let hsm = start(&ctx, instance, model)?;
        hsm.start().await;

        let instance = hsm.instance().read().unwrap();
        let inst = instance
            .as_any()
            .downcast_ref::<ChoiceTestInstance>()
            .unwrap();
        assert!(inst.log.contains(&expected_effect.to_string()));
        assert_eq!(hsm.state(), expected_state);
    }

    Ok(())
}

#[tokio::test]
async fn test_choice_in_hierarchical_state() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    instance.set_direction("left");
    let ctx = Context::new();

    fn routed_left(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("routed-left");
        Box::pin(async move {})
    }

    fn routed_right(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("routed-right");
        Box::pin(async move {})
    }

    fn routed_center(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("routed-center");
        Box::pin(async move {})
    }

    fn left_entry(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("left-entry");
        Box::pin(async move {})
    }

    fn right_entry(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("right-entry");
        Box::pin(async move {})
    }

    fn center_entry(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("center-entry");
        Box::pin(async move {})
    }

    let model = define!(
        "HierarchicalChoiceMachine",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("router")),
            choice!(
                "router",
                transition!(
                    guard!(direction_left),
                    target!("left"),
                    effect!(routed_left)
                ),
                transition!(
                    guard!(direction_right),
                    target!("right"),
                    effect!(routed_right)
                ),
                transition!(target!("center"), effect!(routed_center))
            ),
            state!("left", entry!(left_entry)),
            state!("right", entry!(right_entry)),
            state!("center", entry!(center_entry))
        )
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["routed-left", "left-entry"]);
    assert_eq!(hsm.state(), "/HierarchicalChoiceMachine/container/left");

    Ok(())
}

#[tokio::test]
async fn test_choice_with_complex_guard_conditions() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    instance
        .config
        .insert("enabled".to_string(), "true".to_string());
    instance
        .config
        .insert("priority".to_string(), "2".to_string());
    instance
        .config
        .insert("mode".to_string(), "auto".to_string());
    let ctx = Context::new();

    fn disabled_guard(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
        inst.config.get("enabled").unwrap_or(&"false".to_string()) != "true"
    }

    fn high_priority_manual_guard(
        _ctx: &Context,
        inst: &ChoiceTestInstance,
        _event: &Event,
    ) -> bool {
        let enabled = inst.config.get("enabled").unwrap_or(&"false".to_string()) == "true";
        let priority: i32 = inst
            .config
            .get("priority")
            .unwrap_or(&"0".to_string())
            .parse()
            .unwrap_or(0);
        let mode = inst.config.get("mode").map(|s| s.as_str()).unwrap_or("");
        enabled && priority > 5 && mode == "manual"
    }

    fn auto_mode_guard(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
        let enabled = inst.config.get("enabled").unwrap_or(&"false".to_string()) == "true";
        let mode = inst.config.get("mode").map(|s| s.as_str()).unwrap_or("");
        enabled && mode == "auto"
    }

    fn disabled_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("disabled-path");
        Box::pin(async move {})
    }

    fn high_priority_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("high-priority-manual");
        Box::pin(async move {})
    }

    fn automatic_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("automatic-mode");
        Box::pin(async move {})
    }

    fn default_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("default-fallback");
        Box::pin(async move {})
    }

    let model = define!(
        "ComplexChoiceMachine",
        initial!(target!("choice")),
        choice!(
            "choice",
            transition!(
                guard!(disabled_guard),
                target!("disabled"),
                effect!(disabled_effect)
            ),
            transition!(
                guard!(high_priority_manual_guard),
                target!("highpriority"),
                effect!(high_priority_effect)
            ),
            transition!(
                guard!(auto_mode_guard),
                target!("automatic"),
                effect!(automatic_effect)
            ),
            transition!(target!("default"), effect!(default_effect))
        ),
        state!("disabled"),
        state!("highpriority"),
        state!("automatic"),
        state!("default")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should choose automatic mode
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["automatic-mode"]);
    assert_eq!(hsm.state(), "/ComplexChoiceMachine/automatic");

    Ok(())
}

#[tokio::test]
async fn test_choice_with_event_data_evaluation() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    let ctx = Context::new();

    fn urgent_guard(_ctx: &Context, _inst: &ChoiceTestInstance, event: &Event) -> bool {
        if let Some(event_type) = event.get_data::<String>() {
            event_type == "urgent"
        } else {
            false
        }
    }

    fn normal_guard(_ctx: &Context, _inst: &ChoiceTestInstance, event: &Event) -> bool {
        if let Some(event_type) = event.get_data::<String>() {
            event_type == "normal"
        } else {
            false
        }
    }

    fn urgent_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("urgent-processing");
        Box::pin(async move {})
    }

    fn normal_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("normal-processing");
        Box::pin(async move {})
    }

    fn fallback_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("fallback-processing");
        Box::pin(async move {})
    }

    let model = define!(
        "EventChoiceMachine",
        initial!(target!("waiting")),
        state!("waiting", transition!(on!("process"), target!("../router"))),
        choice!(
            "router",
            transition!(
                guard!(urgent_guard),
                target!("urgent"),
                effect!(urgent_effect)
            ),
            transition!(
                guard!(normal_guard),
                target!("normal"),
                effect!(normal_effect)
            ),
            transition!(target!("fallback"), effect!(fallback_effect))
        ),
        state!("urgent"),
        state!("normal"),
        state!("fallback")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Send event with urgent data
    let process_event = Event::new("process").with_data("urgent".to_string());
    hsm.dispatch(&ctx, process_event).await;

    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["urgent-processing"]);
    assert_eq!(hsm.state(), "/EventChoiceMachine/urgent");

    Ok(())
}

#[tokio::test]
async fn test_nested_choice_pseudostates() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    instance.data.insert("level1".to_string(), 1); // true
    instance
        .config
        .insert("level2".to_string(), "b".to_string());
    let ctx = Context::new();

    fn level1_guard(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
        *inst.data.get("level1").unwrap_or(&0) > 0
    }

    fn level2_a_guard(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
        inst.config.get("level2").unwrap_or(&"".to_string()) == "a"
    }

    fn level2_b_guard(_ctx: &Context, inst: &ChoiceTestInstance, _event: &Event) -> bool {
        inst.config.get("level2").unwrap_or(&"".to_string()) == "b"
    }

    fn level1_true_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("level1-true");
        Box::pin(async move {})
    }

    fn level1_false_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("level1-false");
        Box::pin(async move {})
    }

    fn level2_a_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("level2-a");
        Box::pin(async move {})
    }

    fn level2_b_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("level2-b");
        Box::pin(async move {})
    }

    fn level2_other_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("level2-other");
        Box::pin(async move {})
    }

    let model = define!(
        "NestedChoiceMachine",
        initial!(target!("level1choice")),
        choice!(
            "level1choice",
            transition!(
                guard!(level1_guard),
                target!("level2choice"),
                effect!(level1_true_effect)
            ),
            transition!(target!("level1false"), effect!(level1_false_effect))
        ),
        choice!(
            "level2choice",
            transition!(
                guard!(level2_a_guard),
                target!("result_a"),
                effect!(level2_a_effect)
            ),
            transition!(
                guard!(level2_b_guard),
                target!("result_b"),
                effect!(level2_b_effect)
            ),
            transition!(target!("result_other"), effect!(level2_other_effect))
        ),
        state!("level1false"),
        state!("result_a"),
        state!("result_b"),
        state!("result_other")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    // Should follow level1 choice then level2 choice
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["level1-true", "level2-b"]);
    assert_eq!(hsm.state(), "/NestedChoiceMachine/result_b");

    Ok(())
}

#[tokio::test]
async fn test_choice_with_side_effects_in_guards() -> Result<()> {
    let mut instance = ChoiceTestInstance::new();
    let ctx = Context::new();

    fn guard1(_ctx: &Context, _inst: &ChoiceTestInstance, _event: &Event) -> bool {
        false
    }

    fn guard2(_ctx: &Context, _inst: &ChoiceTestInstance, _event: &Event) -> bool {
        true
    }

    fn guard3(_ctx: &Context, _inst: &ChoiceTestInstance, _event: &Event) -> bool {
        true
    }

    fn path2_effect(
        _ctx: &Context,
        inst: &mut ChoiceTestInstance,
        _event: &Event,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        inst.log_action("path2-effect");
        Box::pin(async move {})
    }

    let model = define!(
        "SideEffectChoiceMachine",
        initial!(target!("choice")),
        choice!(
            "choice",
            transition!(guard!(guard1), target!("path1")),
            transition!(guard!(guard2), target!("path2"), effect!(path2_effect)),
            transition!(guard!(guard3), target!("path3")),
            transition!(target!("path3"))
        ),
        state!("path1"),
        state!("path2"),
        state!("path3")
    );

    let hsm = start(&ctx, instance, model)?;
    hsm.start().await?;

    // Should take path2 which has effect
    let instance = hsm.instance().read().unwrap();
    let inst = instance
        .as_any()
        .downcast_ref::<ChoiceTestInstance>()
        .unwrap();
    assert_eq!(inst.log, vec!["path2-effect"]);
    assert_eq!(hsm.state(), "/SideEffectChoiceMachine/path2");

    Ok(())
}
