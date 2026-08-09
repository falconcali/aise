use aise::CoordinatorConfig;
use aise::core::turn_contract::TurnCancellation;
use aise::domain::ids::StoryId;
use aise::runtime::StoryTurnCoordinator;
use std::time::{Duration, Instant};

#[tokio::test]
async fn story_coordinator_serializes_same_story() {
    let coordinator = StoryTurnCoordinator::new(&CoordinatorConfig::default());
    let story = StoryId::try_new("story-coord").unwrap();
    let first = coordinator
        .acquire(&story, Instant::now() + Duration::from_secs(2), &TurnCancellation::new())
        .await
        .expect("first");
    let second = tokio::spawn({
        let coordinator = coordinator.clone();
        let story = story.clone();
        async move {
            coordinator
                .acquire(&story, Instant::now() + Duration::from_secs(2), &TurnCancellation::new())
                .await
        }
    });
    drop(first);
    second.await.expect("join").expect("second");
}
