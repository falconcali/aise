mod actor;
mod character;
mod character_card;
mod character_think;
mod memory;
mod role;

pub use actor::RoleActor;
pub use character::{CharacterProfile, DialogueExample};
pub use character_card::{CharacterCard, CharacterCardRef, CharacterCardSpec, CharacterMeta};
pub use character_think::{CharacterDecision, CharacterThinkRequest, ResolvedRole};
pub use memory::{MemoryBody, MemoryEntry};
pub use role::{RoleBody, RoleEntry};
