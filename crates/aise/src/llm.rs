//! LLM infrastructure: provider boundary, message models, and the shared
//! concurrency limiter (R-CONC-04). No engine logic here.

pub mod error;
pub mod limiter;
pub mod message;
pub mod openai_compat;
pub mod provider;

pub use error::LlmError;
pub use limiter::LlmLimiter;
pub use message::{ChatMessage, CompletionRequest, Role};
pub use openai_compat::OpenAiCompatProvider;
pub use provider::{DeltaSink, LlmProvider};
