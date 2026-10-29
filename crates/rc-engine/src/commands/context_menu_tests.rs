use crate::test_support;
use crate::{Command, EngineEvent, PanelId};

#[tokio::test]
async fn the_context_menu_is_announced_for_its_panel() {
    let (_root, engine) = test_support::engine(&[]).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ContextMenu {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("announces");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::ContextMenu {
            panel: PanelId::Right
        })
    );
}

#[tokio::test]
async fn without_a_panel_the_active_one_s_menu_is_wanted() {
    let (_root, engine) = test_support::engine(&[]).await;
    let active = engine.active_panel();
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ContextMenu { panel: None })
        .await
        .expect("announces");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::ContextMenu { panel: active })
    );
}
