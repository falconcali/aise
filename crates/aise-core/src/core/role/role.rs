use crate::core::knowledge::{EntryOrigin, RetrievalHint};
use crate::core::role::{CharacterProfile, MemoryBody, MemoryEntry};
use crate::core::{BoundedText, MemoryId, RoleId, StoryError};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    pub name: BoundedText,
    #[serde(default)]
    pub alias: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone)]
pub struct SeedRole {
    pub spec: RoleSpec,
    pub default_profile: CharacterProfile,
    pub seed_memories: BTreeMap<MemoryId, MemoryBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub id: RoleId,
    pub origin: EntryOrigin,
    pub spec: RoleSpec,
    pub profile: CharacterProfile,
    pub memories: BTreeMap<MemoryId, MemoryEntry>,
}

pub const MAX_ROLES_PER_STORY: usize = 64;
pub const MAX_ALIASES_PER_ROLE: usize = 8;
pub const MAX_ROLE_CATALOG_BYTES: usize = 32 * 1024;
pub const MAX_MEMORIES_PER_ROLE: usize = 32;
pub const MAX_ROLE_STATE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OPENING_BYTES: usize = 16 * 1024;

impl Serialize for SeedRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("SeedRole", 6)?;
        state.serialize_field("name", &self.spec.name)?;
        state.serialize_field("alias", &self.spec.alias)?;
        state.serialize_field("backstory", &self.spec.backstory)?;
        state.serialize_field("retrieval", &self.spec.retrieval)?;
        state.serialize_field("default_profile", &self.default_profile)?;
        state.serialize_field("seed_memories", &self.seed_memories)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for SeedRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "SeedRole",
            &[
                "name",
                "alias",
                "backstory",
                "retrieval",
                "default_profile",
                "seed_memories",
            ],
            SeedRoleVisitor,
        )
    }
}

struct SeedRoleVisitor;

impl<'de> Visitor<'de> for SeedRoleVisitor {
    type Value = SeedRole;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a seed role")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name = None;
        let mut alias = None;
        let mut backstory = None;
        let mut retrieval = None;
        let mut default_profile = None;
        let mut seed_memories = None;
        while let Some(field) = map.next_key::<SeedRoleField>()? {
            match field {
                SeedRoleField::Name => set_once(&mut name, "name", &mut map)?,
                SeedRoleField::Alias => set_once(&mut alias, "alias", &mut map)?,
                SeedRoleField::Backstory => set_once(&mut backstory, "backstory", &mut map)?,
                SeedRoleField::Retrieval => set_once(&mut retrieval, "retrieval", &mut map)?,
                SeedRoleField::DefaultProfile => {
                    set_once(&mut default_profile, "default_profile", &mut map)?;
                }
                SeedRoleField::SeedMemories => {
                    set_once(&mut seed_memories, "seed_memories", &mut map)?;
                }
            }
        }
        Ok(SeedRole {
            spec: RoleSpec {
                name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                alias: alias.unwrap_or_default(),
                backstory: backstory.ok_or_else(|| de::Error::missing_field("backstory"))?,
                retrieval: retrieval.ok_or_else(|| de::Error::missing_field("retrieval"))?,
            },
            default_profile: default_profile.ok_or_else(|| de::Error::missing_field("default_profile"))?,
            seed_memories: seed_memories.unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum SeedRoleField {
    Name,
    Alias,
    Backstory,
    Retrieval,
    DefaultProfile,
    SeedMemories,
}

fn set_once<'de, A, T>(
    target: &mut Option<T>,
    field: &'static str,
    map: &mut A,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
    T: Deserialize<'de>,
{
    if target.is_some() {
        return Err(de::Error::duplicate_field(field));
    }
    *target = Some(map.next_value()?);
    Ok(())
}

pub(crate) fn validate_role_spec(role_id: &RoleId, spec: &RoleSpec) -> Result<(), StoryError> {
    if spec.alias.len() > MAX_ALIASES_PER_ROLE {
        return Err(StoryError::TooManyRoleAliases {
            role_id: role_id.clone(),
            actual: spec.alias.len(),
            maximum: MAX_ALIASES_PER_ROLE,
        });
    }
    Ok(())
}

pub(crate) fn validate_seed_roles(roles: &BTreeMap<RoleId, SeedRole>) -> Result<(), StoryError> {
    validate_role_count(roles.len())?;
    let mut catalog_bytes = 0usize;
    let projected = roles
        .iter()
        .map(|(role_id, role)| {
            validate_role_spec(role_id, &role.spec)?;
            validate_memory_count(role_id, role.seed_memories.len())?;
            catalog_bytes = catalog_bytes.saturating_add(role_catalog_bytes(role_id, &role.spec));
            let memories = role
                .seed_memories
                .iter()
                .map(|(memory_id, body)| {
                    (
                        memory_id,
                        MemoryEntryProjection {
                            id: memory_id,
                            origin: EntryOrigin::Seed,
                            body,
                        },
                    )
                })
                .collect();
            Ok((
                role_id,
                RoleEntryProjection {
                    id: role_id,
                    origin: EntryOrigin::Seed,
                    spec: &role.spec,
                    profile: &role.default_profile,
                    memories,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, StoryError>>()?;
    validate_catalog_bytes(catalog_bytes)?;
    validate_state_size(&projected)
}

pub(crate) fn validate_runtime_roles(roles: &BTreeMap<RoleId, RoleEntry>) -> Result<(), StoryError> {
    validate_role_count(roles.len())?;
    let mut catalog_bytes = 0usize;
    for (role_id, role) in roles {
        validate_role_spec(role_id, &role.spec)?;
        validate_memory_count(role_id, role.memories.len())?;
        catalog_bytes = catalog_bytes.saturating_add(role_catalog_bytes(role_id, &role.spec));
    }
    validate_catalog_bytes(catalog_bytes)?;
    validate_state_size(roles)
}

fn validate_role_count(actual: usize) -> Result<(), StoryError> {
    if actual > MAX_ROLES_PER_STORY {
        return Err(StoryError::TooManyRoles {
            actual,
            maximum: MAX_ROLES_PER_STORY,
        });
    }
    Ok(())
}

fn validate_memory_count(role_id: &RoleId, actual: usize) -> Result<(), StoryError> {
    if actual > MAX_MEMORIES_PER_ROLE {
        return Err(StoryError::TooManyRoleMemories {
            role_id: role_id.clone(),
            actual,
            maximum: MAX_MEMORIES_PER_ROLE,
        });
    }
    Ok(())
}

fn role_catalog_bytes(role_id: &RoleId, spec: &RoleSpec) -> usize {
    spec.alias
        .iter()
        .fold(
            role_id.as_str().len().saturating_add(spec.name.as_str().len()),
            |total, alias| total.saturating_add(alias.as_str().len()),
        )
        .saturating_add(spec.retrieval.as_str().len())
}

fn validate_catalog_bytes(actual: usize) -> Result<(), StoryError> {
    if actual > MAX_ROLE_CATALOG_BYTES {
        return Err(StoryError::RoleCatalogTooLarge {
            actual,
            maximum: MAX_ROLE_CATALOG_BYTES,
        });
    }
    Ok(())
}

fn validate_state_size(value: &impl Serialize) -> Result<(), StoryError> {
    let mut writer = CountingWriter::new(MAX_ROLE_STATE_BYTES);
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(()),
        Err(_) if writer.exceeded => Err(StoryError::RoleStateTooLarge {
            actual: writer.written,
            maximum: MAX_ROLE_STATE_BYTES,
        }),
        Err(error) => Err(StoryError::RoleStateSerialization {
            message: error.to_string(),
        }),
    }
}

#[derive(Serialize)]
struct RoleEntryProjection<'a> {
    id: &'a RoleId,
    origin: EntryOrigin,
    spec: &'a RoleSpec,
    profile: &'a CharacterProfile,
    memories: BTreeMap<&'a MemoryId, MemoryEntryProjection<'a>>,
}

#[derive(Serialize)]
struct MemoryEntryProjection<'a> {
    id: &'a MemoryId,
    origin: EntryOrigin,
    body: &'a MemoryBody,
}

struct CountingWriter {
    maximum: usize,
    written: usize,
    exceeded: bool,
}

impl CountingWriter {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            written: 0,
            exceeded: false,
        }
    }
}

impl Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() > self.maximum.saturating_sub(self.written) {
            self.written = self.maximum.saturating_add(1);
            self.exceeded = true;
            return Err(io::Error::other("role state size exceeded"));
        }
        self.written = self.written.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/role_tests.rs"]
mod tests;
