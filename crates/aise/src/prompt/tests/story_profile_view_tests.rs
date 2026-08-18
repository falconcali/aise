use super::*;
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "text", 256).unwrap()
}

fn profile() -> StoryProfile {
    StoryProfile {
        language: text("en"),
        genre: vec![text("mystery")],
        themes: vec![text("betrayal")],
        style: StoryStyle {
            tone: vec![text("tense")],
            point_of_view: text("third_limited"),
            tense: text("past"),
        },
    }
}

#[test]
fn render_story_profile_view_renders_title_as_first_line() {
    let view = StoryProfilePromptView::new(&text("The Lodge Keeper"), &profile());
    let rendered = render_story_profile_view(&view);
    let first_line = rendered.lines().next().unwrap();
    assert_eq!(first_line, "title: \"The Lodge Keeper\"");
}

#[test]
fn render_story_profile_view_includes_all_fields() {
    let view = StoryProfilePromptView::new(&text("Title"), &profile());
    let rendered = render_story_profile_view(&view);
    assert!(rendered.contains("language: \"en\""));
    assert!(rendered.contains("genre: [\"mystery\"]"));
    assert!(rendered.contains("themes: [\"betrayal\"]"));
    assert!(rendered.contains("tone: [\"tense\"]"));
    assert!(rendered.contains("point_of_view: \"third_limited\""));
    assert!(rendered.contains("tense: \"past\""));
}

#[test]
fn render_story_profile_view_omits_empty_optional_lists() {
    let mut profile = profile();
    profile.genre.clear();
    profile.themes.clear();
    profile.style.tone.clear();
    let view = StoryProfilePromptView::new(&text("Title"), &profile);
    let rendered = render_story_profile_view(&view);
    assert!(!rendered.contains("genre:"));
    assert!(!rendered.contains("themes:"));
    assert!(!rendered.contains("tone:"));
}
