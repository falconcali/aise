//! Browser sessions: HTTP resources that own one story each (R-ARCH-02).
//! Lives in the server layer; the engine only knows stories.

pub mod error;
pub mod model;
pub mod registry;

pub use error::SessionError;
pub use model::{Session, SessionId, SessionInfo};
pub use registry::SessionRegistry;
