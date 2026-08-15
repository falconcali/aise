use crate::config::ContextPreparationConfig;
use crate::context::error::ContextError;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::text_matcher::{TextMatcher, normalize_match_text, term_matches};
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{EntitySignal, RetrievalSignalOrigin, RetrievalSignals, TopicSignal};

pub struct RetrievalSignalBuilder {
    config: ContextPreparationConfig,
    topic_matcher: TextMatcher,
}

impl RetrievalSignalBuilder {
    pub fn new(config: ContextPreparationConfig) -> Self {
        Self {
            config,
            topic_matcher: TextMatcher,
        }
    }

    pub fn build(&self, snapshot: &StoryReadSnapshot, player_input: &str) -> Result<RetrievalSignals, ContextError> {
        let scene = snapshot.current_scene();
        let mut entities = Vec::new();
        let mut topics = Vec::new();
        let mut present = scene.present_role_ids.clone();
        present.sort();
        present.dedup();

        self.push_text_matches(
            player_input,
            snapshot,
            RetrievalSignalOrigin::PlayerInput,
            0,
            &mut entities,
            &mut topics,
        )?;
        self.push_structured_scene(scene, &mut entities)?;
        self.push_text_matches(
            scene.description.as_str(),
            snapshot,
            RetrievalSignalOrigin::Scene,
            1,
            &mut entities,
            &mut topics,
        )?;
        let recent_limit = self.config.recent_segments_for_signals.min(2);
        for segment in snapshot.story_continuity().recent_segments().iter().rev().take(recent_limit) {
            self.push_text_matches(
                segment.text.as_str(),
                snapshot,
                RetrievalSignalOrigin::RecentStory,
                3,
                &mut entities,
                &mut topics,
            )?;
        }
        entities.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.entity.cmp(&right.entity))
                .then_with(|| left.origin.cmp(&right.origin))
        });
        entities.dedup_by(|left, right| left.entity == right.entity);
        if entities.len() > self.config.max_signal_entities {
            return Err(ContextError::SignalLimitExceeded {
                limit: "max_signal_entities",
            });
        }
        topics.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.topic.cmp(&right.topic))
                .then_with(|| left.origin.cmp(&right.origin))
        });
        topics.dedup_by(|left, right| left.topic == right.topic);
        if topics.len() > self.config.max_signal_topics {
            return Err(ContextError::SignalLimitExceeded {
                limit: "max_signal_topics",
            });
        }

        Ok(RetrievalSignals {
            scene_key: scene.scene_key.clone(),
            location_key: scene.location_key.clone(),
            present_role_ids: present,
            entities,
            topics,
        })
    }

    fn push_structured_scene(
        &self,
        scene: &crate::domain::story_instance::state::CurrentScene,
        entities: &mut Vec<EntitySignal>,
    ) -> Result<(), ContextError> {
        entities.push(EntitySignal {
            entity: KnowledgeEntity::Scene(scene.scene_key.clone()),
            origin: RetrievalSignalOrigin::Scene,
            priority: 1,
        });
        entities.push(EntitySignal {
            entity: KnowledgeEntity::Location(scene.location_key.clone()),
            origin: RetrievalSignalOrigin::Scene,
            priority: 1,
        });
        for role_id in &scene.present_role_ids {
            entities.push(EntitySignal {
                entity: KnowledgeEntity::Role(role_id.clone()),
                origin: RetrievalSignalOrigin::Scene,
                priority: 1,
            });
        }
        if entities.len() > self.config.max_signal_entities {
            return Err(ContextError::SignalLimitExceeded {
                limit: "max_signal_entities",
            });
        }
        Ok(())
    }

    fn push_text_matches(
        &self,
        text: &str,
        snapshot: &StoryReadSnapshot,
        origin: RetrievalSignalOrigin,
        priority: u8,
        entities: &mut Vec<EntitySignal>,
        topics: &mut Vec<TopicSignal>,
    ) -> Result<(), ContextError> {
        let haystack = normalize_match_text(text);
        if haystack.is_empty() {
            return Ok(());
        }
        for (role_id, role) in snapshot.roles() {
            let label = normalize_match_text(role.role_label.as_str());
            let name = normalize_match_text(role.effective_profile.name.as_str());
            if term_matches(&haystack, &label) || term_matches(&haystack, &name) {
                entities.push(EntitySignal {
                    entity: KnowledgeEntity::Role(role_id.clone()),
                    origin,
                    priority,
                });
            }
        }
        for entity in snapshot.entity_catalog() {
            let key = entity_match_text(entity);
            if term_matches(&haystack, &normalize_match_text(&key)) {
                entities.push(EntitySignal {
                    entity: entity.clone(),
                    origin,
                    priority,
                });
            }
        }
        for topic in self.topic_matcher.match_topics(text, snapshot.topic_dictionary()) {
            topics.push(TopicSignal {
                topic,
                origin,
                priority,
            });
        }
        Ok(())
    }
}

fn entity_match_text(entity: &KnowledgeEntity) -> String {
    match entity {
        KnowledgeEntity::World(key) => key.as_str().to_owned(),
        KnowledgeEntity::Role(id) => id.as_str().to_owned(),
        KnowledgeEntity::Location(key) => key.as_str().to_owned(),
        KnowledgeEntity::Scene(key) => key.as_str().to_owned(),
        KnowledgeEntity::NarrativeNode(key) => key.as_str().to_owned(),
        KnowledgeEntity::Event(key) => key.as_str().to_owned(),
    }
}
