mod actor;
mod character;
mod character_card;
mod memory;
mod role;

pub use actor::RoleActor;
pub use character::{CharacterProfile, DialogueExample};
pub use character_card::{CharacterCard, CharacterCardRef, CharacterCardSpec, CharacterMeta};
pub use memory::{MemoryBody, MemoryEntry};
pub use role::{RoleBody, RoleEntry};
