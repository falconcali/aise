pub fn estimate_text_tokens(text: &str) -> u64 {
    let characters = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    characters.saturating_add(3).checked_div(4).unwrap_or(0).max(1)
}
