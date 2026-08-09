use crate::config::ContextPreparationConfig;
use crate::context::error::ContextError;
use crate::context::topic_matcher::{TopicMatcher, normalize_match_text, term_matches};
use crate::core::turn_data::{EntitySignal, RetrievalSignalOrigin, RetrievalSignals, TopicSignal};
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::StoryRoleKey;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use std::collections::BTreeSet;

pub struct RetrievalSignalBuilder {
    config: ContextPreparationConfig,
    topic_matcher: TopicMatcher,
}

impl RetrievalSignalBuilder {
    pub fn new(config: ContextPreparationConfig) -> Self {
        Self {
            config,
            topic_matcher: TopicMatcher,
        }
    }

    pub fn build(&self, snapshot: &StoryReadSnapshot, player_input: &str) -> Result<RetrievalSignals, ContextError> {
        let scene = snapshot.current_scene();
        let mut entities = Vec::new();
        let mut topics = Vec::new();
        let mut present = scene.present_character_ids.clone();
        present.sort();
        present.dedup();
        let mut active_role_keys: Vec<StoryRoleKey> = present
            .iter()
            .filter_map(|id| snapshot.character_states().get(id).map(|state| state.role_key.clone()))
            .collect();
        active_role_keys.sort();
        active_role_keys.dedup();

        self.push_text_matches(
            player_input,
            snapshot,
            RetrievalSignalOrigin::PlayerInput,
            0,
            &mut entities,
            &mut topics,
        )?;
        self.push_structured_scene(scene, snapshot, &mut entities)?;
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
        let summary = snapshot.story_continuity().summary().text.as_str();
        if !summary.trim().is_empty() {
            self.push_text_matches(summary, snapshot, RetrievalSignalOrigin::Summary, 4, &mut entities, &mut topics)?;
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
            present_character_ids: present,
            active_role_keys,
            entities,
            topics,
        })
    }

    fn push_structured_scene(
        &self,
        scene: &crate::domain::story_instance::state::CurrentScene,
        snapshot: &StoryReadSnapshot,
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
        for character_id in &scene.present_character_ids {
            entities.push(EntitySignal {
                entity: KnowledgeEntity::Character(character_id.clone()),
                origin: RetrievalSignalOrigin::Scene,
                priority: 1,
            });
            if let Some(state) = snapshot.character_states().get(character_id) {
                entities.push(EntitySignal {
                    entity: KnowledgeEntity::Role(state.role_key.clone()),
                    origin: RetrievalSignalOrigin::Scene,
                    priority: 1,
                });
            }
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
        let mut role_hits: BTreeSet<StoryRoleKey> = BTreeSet::new();
        for (role_key, role) in snapshot.role_definitions() {
            let label = normalize_match_text(role.role_label.as_str());
            if term_matches(&haystack, &label) {
                role_hits.insert(role_key.clone());
            }
        }
        for (character_id, card) in snapshot.character_cards() {
            let name = normalize_match_text(card.meta.name.as_str());
            if term_matches(&haystack, &name) {
                if let Some(state) = snapshot.character_states().get(character_id) {
                    role_hits.insert(state.role_key.clone());
                }
            }
        }
        for role_key in role_hits {
            entities.push(EntitySignal {
                entity: KnowledgeEntity::Role(role_key),
                origin,
                priority,
            });
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
        KnowledgeEntity::Role(key) => key.as_str().to_owned(),
        KnowledgeEntity::Character(id) => id.as_str().to_owned(),
        KnowledgeEntity::Location(key) => key.as_str().to_owned(),
        KnowledgeEntity::Scene(key) => key.as_str().to_owned(),
        KnowledgeEntity::NarrativeNode(key) => key.as_str().to_owned(),
        KnowledgeEntity::Event(key) => key.as_str().to_owned(),
    }
}
