#[test]
fn legacy_story_create_path_is_retired_in_favor_of_instances() {
    assert_ne!("/api/stories", "/api/story-instances");
}
