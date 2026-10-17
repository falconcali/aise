mod character;
mod character_card;
mod character_think;
mod memory;
mod role;

pub use character::{CharacterProfile, DialogueExample};
pub use character_card::{CharacterCard, CharacterCardSpec, CharacterMeta};
pub use character_think::{CharacterDecision, CharacterThinkRequest};
pub use memory::{MemoryBody, MemoryEntry};
pub use role::{
    MAX_ALIASES_PER_ROLE, MAX_MEMORIES_PER_ROLE, MAX_OPENING_BYTES, MAX_ROLES_PER_STORY, MAX_ROLE_CATALOG_BYTES,
    MAX_ROLE_STATE_BYTES, RoleEntry, RoleSpec, SeedRole,
};
pub(crate) use role::{validate_role_spec, validate_runtime_roles, validate_seed_roles};
