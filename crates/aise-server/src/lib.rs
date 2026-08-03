#![forbid(unsafe_code)]

//! `aise-server` — web transport over the `aise` engine.
//!
//! HTTP/SSE + static frontend + session resources. No engine logic here;
//! see the `aise` crate for that.

pub mod api;
pub mod app;
pub mod config;
pub mod error;
pub mod session;

pub use api::{AppState, router};
pub use app::build_engine;
pub use config::ServerConfig;
