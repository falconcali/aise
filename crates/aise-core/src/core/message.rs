use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
    System,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChatMessage {
    pub role: ChatMessageRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::User,
            content: content.into(),
        }
    }
}
