use crate::domain::asset::ids::{InstanceSettingKey, RelationshipKind};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::RoleId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastPolicy {
    #[default]
    Open,
    IncidentalOnly,
    Closed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceSettings {
    #[serde(default)]
    pub cast_policy: CastPolicy,
    #[serde(default)]
    pub values: BTreeMap<InstanceSettingKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationshipKey {
    pub source_role_id: RoleId,
    pub target_role_id: RoleId,
    pub kind: RelationshipKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipState {
    pub source_role_id: RoleId,
    pub target_role_id: RoleId,
    pub kind: RelationshipKind,
    pub trust: i16,
}

impl RelationshipState {
    pub fn key(&self) -> RelationshipKey {
        RelationshipKey {
            source_role_id: self.source_role_id.clone(),
            target_role_id: self.target_role_id.clone(),
            kind: self.kind.clone(),
        }
    }
}
