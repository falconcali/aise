pub fn estimate_text_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    let estimated = chars.div_ceil(4);
    estimated.max(1)
}
