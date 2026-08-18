use crate::domain::asset::story_pack::StoryProfile;
use crate::domain::asset::validation::BoundedText;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StoryProfilePromptView {
    pub title: BoundedText,
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}

impl StoryProfilePromptView {
    pub fn new(title: &BoundedText, profile: &StoryProfile) -> Self {
        Self {
            title: title.clone(),
            language: profile.language.clone(),
            genre: profile.genre.clone(),
            themes: profile.themes.clone(),
            tone: profile.style.tone.clone(),
            point_of_view: profile.style.point_of_view.clone(),
            tense: profile.style.tense.clone(),
        }
    }
}

pub fn render_story_profile_view(view: &StoryProfilePromptView) -> String {
    let mut lines = vec![
        field("title", view.title.as_str()),
        field("language", view.language.as_str()),
    ];
    if !view.genre.is_empty() {
        lines.push(list_field("genre", &view.genre));
    }
    if !view.themes.is_empty() {
        lines.push(list_field("themes", &view.themes));
    }
    if !view.tone.is_empty() {
        lines.push(list_field("tone", &view.tone));
    }
    lines.push(field("point_of_view", view.point_of_view.as_str()));
    lines.push(field("tense", view.tense.as_str()));
    lines.join("\n")
}

fn field(name: &str, value: &str) -> String {
    format!("{name}: {}", quoted(value))
}

fn list_field(name: &str, values: &[BoundedText]) -> String {
    format!(
        "{name}: [{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
#[path = "tests/story_profile_view_tests.rs"]
mod tests;
