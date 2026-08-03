#![forbid(unsafe_code)]

//! `aise` — AI Story Engine core library.
//!
//! Pipeline-driven, turn-based narrative engine: one player input triggers one
//! complete Story Turn orchestrated by `TurnRuntime` (see
//! `doc/design/Architecture.md`). This crate is an index only (R-CODE-01).

pub mod character;
pub mod config;
pub mod context;
pub mod domain;
pub mod engine;
pub mod error;
pub mod llm;
pub mod persistence;
pub mod planning;
pub mod runtime;
pub mod story;
pub mod validation;

pub use config::{AiseConfig, LlmConfig, StorageConfig, TurnConfig};
pub use engine::{AiseEngine, TurnEvent, TurnEventSink, TurnResult};
pub use error::AiseError;
