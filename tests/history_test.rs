use stateforward_hsm::*;

#[derive(Debug, Default)]
struct HistoryInstance;

impl Instance for HistoryInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[tokio::test]
async fn shallow_history_restores_direct_child_then_initial() -> Result<()> {
    let ctx = Context::new();
    let model = define!(
        "ShallowHistoryRules",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("region")),
            state!(
                "region",
                initial!(target!("a1")),
                state!("a1", transition!(on!("next"), target!("../a2"))),
                state!(
                    "a2",
                    transition!(on!("leave"), target!("/ShallowHistoryRules/outside"))
                )
            ),
            shallow_history!("history", target!("region"))
        ),
        state!(
            "outside",
            transition!(
                on!("resume"),
                target!("/ShallowHistoryRules/container/history")
            )
        )
    );

    let hsm = start(&ctx, HistoryInstance::default(), model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("next")).await?;
    hsm.dispatch(&ctx, Event::new("leave")).await?;
    hsm.dispatch(&ctx, Event::new("resume")).await?;

    assert_eq!(hsm.state(), "/ShallowHistoryRules/container/region/a1");
    Ok(())
}

#[tokio::test]
async fn deep_history_restores_leaf_state() -> Result<()> {
    let ctx = Context::new();
    let model = define!(
        "DeepHistoryRules",
        initial!(target!("container")),
        state!(
            "container",
            initial!(target!("region")),
            state!(
                "region",
                initial!(target!("a1")),
                state!("a1", transition!(on!("next"), target!("../a2"))),
                state!(
                    "a2",
                    transition!(on!("leave"), target!("/DeepHistoryRules/outside"))
                )
            ),
            deep_history!("history", target!("region/a1"))
        ),
        state!(
            "outside",
            transition!(
                on!("resume"),
                target!("/DeepHistoryRules/container/history")
            )
        )
    );

    let hsm = start(&ctx, HistoryInstance::default(), model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("next")).await?;
    hsm.dispatch(&ctx, Event::new("leave")).await?;
    hsm.dispatch(&ctx, Event::new("resume")).await?;

    assert_eq!(hsm.state(), "/DeepHistoryRules/container/region/a2");
    Ok(())
}

#[tokio::test]
async fn history_uses_builder_default_before_snapshot() -> Result<()> {
    let ctx = Context::new();
    let model = define(
        "HistoryDefaultRules",
        vec![
            initial_with_target(target("outside")),
            state_with_behaviors(
                "container",
                vec![
                    shallow_history("history", vec![target("fallback")]),
                    state("fallback"),
                ],
            ),
            state_with_behaviors(
                "outside",
                vec![transition(vec![
                    on("resume"),
                    target("/HistoryDefaultRules/container/history"),
                ])],
            ),
        ],
    );

    let hsm = start(&ctx, HistoryInstance::default(), model)?;
    hsm.start().await?;
    hsm.dispatch(&ctx, Event::new("resume")).await?;

    assert_eq!(hsm.state(), "/HistoryDefaultRules/container/fallback");
    Ok(())
}
