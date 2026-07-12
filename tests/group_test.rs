use stateforward_hsm::*;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Default)]
struct GroupInstance;

impl Instance for GroupInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Debug, Default)]
struct GroupDataInstance {
    started_data: Vec<String>,
    changes: Vec<AttributeChange>,
    calls: Vec<String>,
}

impl Instance for GroupDataInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn group_model() -> Model<GroupInstance> {
    Define(
        "GroupMachine",
        vec![
            Initial(vec![Target("idle")]),
            State("idle", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    )
}

fn capture_group_start_data(
    _ctx: &Context,
    instance: &mut GroupDataInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    if let Some(value) = event.get_data::<String>() {
        instance.started_data.push(value.clone());
    }
    Box::pin(async {})
}

fn capture_group_change(
    _ctx: &Context,
    instance: &mut GroupDataInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let change = event
        .get_data::<AttributeChange>()
        .expect("set event should carry AttributeChange payload");
    instance.changes.push(change.clone());
    Box::pin(async {})
}

fn record_group_call(
    _ctx: &Context,
    instance: &mut GroupDataInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    instance.calls.push(event.name.clone());
    Box::pin(async {})
}

fn group_data_model() -> Model<GroupDataInstance> {
    Define(
        "GroupDataMachine",
        vec![
            Attribute("count", 0),
            Operation("record", record_group_call),
            Initial(vec![Target("idle")]),
            State(
                "idle",
                vec![
                    Entry(capture_group_start_data),
                    Transition(vec![OnSet("count"), Effect(capture_group_change)]),
                    Transition(vec![On("go"), Target("../done")]),
                ],
            ),
            State("done", vec![]),
        ],
    )
}

fn configured(id: &str) -> RuntimeConfig {
    let mut config = Config();
    config.ID = Some(id.to_string());
    config
}

#[tokio::test]
async fn group_get_set_and_call_use_member_runtime_access() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartedWithConfig(
        &ctx,
        GroupDataInstance::default(),
        group_data_model(),
        configured("alpha"),
    )
    .await?;
    let bravo = StartedWithConfig(
        &ctx,
        GroupDataInstance::default(),
        group_data_model(),
        configured("bravo"),
    )
    .await?;
    let group = MakeGroupWithID("access", vec![alpha.clone(), bravo.clone()]);

    assert_eq!(Get(&ctx, &group, "count"), Some(AttributeValue::Int(0)));
    Set(&ctx, &group, "count", 1)?;
    assert_eq!(group.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(alpha.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(bravo.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(alpha.instance().read().unwrap().changes.len(), 1);
    assert_eq!(bravo.instance().read().unwrap().changes.len(), 1);

    Stop(&ctx, &bravo).await?;
    group.Set("count", 2)?;
    assert_eq!(alpha.Get("count"), Some(AttributeValue::Int(2)));
    assert_eq!(bravo.Get("count"), Some(AttributeValue::Int(1)));
    assert_eq!(alpha.instance().read().unwrap().changes.len(), 2);
    assert_eq!(bravo.instance().read().unwrap().changes.len(), 1);

    Call(&ctx, &group, "record").await?;
    assert_eq!(
        alpha.instance().read().unwrap().calls,
        vec!["/GroupDataMachine/record".to_string()],
    );
    assert!(bravo.instance().read().unwrap().calls.is_empty());

    Ok(())
}

#[tokio::test]
async fn group_flattens_members_and_dispatches_only_started_machines() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartedWithConfig(&ctx, GroupInstance, group_model(), configured("alpha")).await?;
    let bravo = StartedWithConfig(
        alpha.context(),
        GroupInstance,
        group_model(),
        configured("bravo"),
    )
    .await?;
    let unstarted = StartWithConfig(
        bravo.context(),
        GroupInstance,
        group_model(),
        configured("unstarted"),
    )?;

    let nested = MakeGroup(vec![bravo.clone()]);
    let group = Group::with_id_and_members(
        "fleet",
        vec![
            alpha.clone().into(),
            nested.into(),
            unstarted.clone().into(),
        ],
    );

    assert_eq!(ID(&group), "fleet");
    assert_eq!(Name(&group), "fleet");
    assert_eq!(QualifiedName(&group), "fleet");
    assert_eq!(group.len(), 3);
    assert_eq!(
        group.state(),
        vec![
            "/GroupMachine/idle".to_string(),
            "/GroupMachine/idle".to_string(),
            "/GroupMachine".to_string(),
        ]
    );

    Dispatch(&ctx, &group, Event::new("go")).await?;

    assert_eq!(alpha.state(), "/GroupMachine/done");
    assert_eq!(bravo.state(), "/GroupMachine/done");
    assert_eq!(unstarted.state(), "/GroupMachine");

    Ok(())
}

#[tokio::test]
async fn group_start_and_restart_fan_out_runtime_data_to_members() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartWithConfig(
        &ctx,
        GroupDataInstance::default(),
        group_data_model(),
        configured("alpha"),
    )?;
    let bravo = StartWithConfig(
        &ctx,
        GroupDataInstance::default(),
        group_data_model(),
        configured("bravo"),
    )?;
    let group = MakeGroupWithID("data", vec![alpha.clone(), bravo.clone()]);

    StartWithData(&group, "boot".to_string()).await?;
    assert_eq!(
        group.state(),
        vec![
            "/GroupDataMachine/idle".to_string(),
            "/GroupDataMachine/idle".to_string(),
        ]
    );
    assert_eq!(
        alpha.instance().read().unwrap().started_data,
        vec!["boot".to_string()],
    );
    assert_eq!(
        bravo.instance().read().unwrap().started_data,
        vec!["boot".to_string()],
    );

    Dispatch(&ctx, &group, Event::new("go")).await?;
    assert_eq!(
        group.state(),
        vec![
            "/GroupDataMachine/done".to_string(),
            "/GroupDataMachine/done".to_string(),
        ]
    );

    RestartWithData(&ctx, &group, "again".to_string()).await?;
    assert_eq!(
        group.state(),
        vec![
            "/GroupDataMachine/idle".to_string(),
            "/GroupDataMachine/idle".to_string(),
        ]
    );
    assert_eq!(
        alpha.instance().read().unwrap().started_data,
        vec!["boot".to_string(), "again".to_string()],
    );
    assert_eq!(
        bravo.instance().read().unwrap().started_data,
        vec!["boot".to_string(), "again".to_string()],
    );

    Stop(&ctx, &group).await?;
    group.StartWithData("manual".to_string()).await?;
    assert_eq!(
        alpha.instance().read().unwrap().started_data,
        vec![
            "boot".to_string(),
            "again".to_string(),
            "manual".to_string(),
        ],
    );
    assert_eq!(
        bravo.instance().read().unwrap().started_data,
        vec![
            "boot".to_string(),
            "again".to_string(),
            "manual".to_string(),
        ],
    );

    Ok(())
}

#[tokio::test]
async fn group_snapshot_restart_and_stop_preserve_member_order() -> Result<()> {
    let ctx = Context::new();
    let alpha = StartedWithConfig(&ctx, GroupInstance, group_model(), configured("alpha")).await?;
    let bravo = StartedWithConfig(
        alpha.context(),
        GroupInstance,
        group_model(),
        configured("bravo"),
    )
    .await?;
    let group = MakeGroupWithID("started", vec![alpha.clone(), bravo.clone()]);

    Dispatch(&ctx, &group, Event::new("go")).await?;

    let snapshots: Vec<Snapshot> = TakeSnapshot(&ctx, &group)?;
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.ID.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "bravo"],
    );
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.State.as_str())
            .collect::<Vec<_>>(),
        vec!["/GroupMachine/done", "/GroupMachine/done"],
    );

    Restart(&ctx, &group).await?;
    assert_eq!(
        group.state(),
        vec![
            "/GroupMachine/idle".to_string(),
            "/GroupMachine/idle".to_string(),
        ]
    );

    Stop(&ctx, &group).await?;
    assert_eq!(
        group.state(),
        vec!["/GroupMachine".to_string(), "/GroupMachine".to_string()]
    );

    Dispatch(&ctx, &group, Event::new("go")).await?;
    assert_eq!(
        group.state(),
        vec!["/GroupMachine".to_string(), "/GroupMachine".to_string()]
    );

    Ok(())
}
