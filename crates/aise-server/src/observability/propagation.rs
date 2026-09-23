pub const DETECTOR_OVERLAP_BYTES: usize = 256;

const REDACTED: &str = "[REDACTED]";
const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "api_key",
    "api-key",
    "apikey",
    "secret_key",
    "secret-key",
    "client_secret",
    "access_token",
    "refresh_token",
    "password",
];
const SECRET_PREFIXES: &[&str] = &["sk-", "pk-lf-", "sk-lf-"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingMasker {
    max_output_bytes: usize,
    redact_pii: bool,
}

impl StreamingMasker {
    pub fn new(max_output_bytes: usize, redact_pii: bool) -> Self {
        Self {
            max_output_bytes,
            redact_pii,
        }
    }

    pub fn mask(&self, value: &str) -> String {
        let input_limit = self.max_output_bytes.saturating_add(DETECTOR_OVERLAP_BYTES);
        let mut masked = utf8_prefix(value, input_limit).to_owned();
        for key in SENSITIVE_KEYS {
            masked = mask_assignment(masked, key);
        }
        for prefix in SECRET_PREFIXES {
            masked = mask_prefixed_token(masked, prefix);
        }
        if self.redact_pii {
            masked = mask_pii(masked);
        }
        utf8_prefix(&masked, self.max_output_bytes).to_owned()
    }

    pub fn mask_chunks<'a>(&self, chunks: impl IntoIterator<Item = &'a str>) -> String {
        let input_limit = self.max_output_bytes.saturating_add(DETECTOR_OVERLAP_BYTES);
        let mut input = String::with_capacity(input_limit);
        for chunk in chunks {
            let remaining = input_limit.saturating_sub(input.len());
            if remaining == 0 {
                break;
            }
            input.push_str(utf8_prefix(chunk, remaining));
        }
        self.mask(&input)
    }

    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }
}

fn mask_assignment(mut value: String, key: &str) -> String {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative) = lower[search_from..].find(key) else {
            return value;
        };
        let key_start = search_from + relative;
        if key_start > 0 && is_identifier(lower.as_bytes()[key_start - 1]) {
            search_from = key_start + key.len();
            continue;
        }
        let mut cursor = key_start + key.len();
        while cursor < value.len() && matches!(value.as_bytes()[cursor], b' ' | b'\t' | b'"' | b'\'') {
            cursor += 1;
        }
        if cursor >= value.len() || !matches!(value.as_bytes()[cursor], b':' | b'=') {
            search_from = key_start + key.len();
            continue;
        }
        cursor += 1;
        while cursor < value.len() && matches!(value.as_bytes()[cursor], b' ' | b'\t') {
            cursor += 1;
        }
        let quote = value
            .as_bytes()
            .get(cursor)
            .copied()
            .filter(|byte| matches!(byte, b'"' | b'\''));
        let value_start = cursor + usize::from(quote.is_some());
        let value_end = assignment_end(&value, value_start, quote);
        value.replace_range(value_start..value_end, REDACTED);
        search_from = value_start + REDACTED.len();
    }
}

fn assignment_end(value: &str, start: usize, quote: Option<u8>) -> usize {
    let bytes = value.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if quote.is_some_and(|quote| byte == quote) || (quote.is_none() && matches!(byte, b'\r' | b'\n' | b',' | b';'))
        {
            break;
        }
        cursor += 1;
    }
    cursor
}

fn mask_prefixed_token(mut value: String, prefix: &str) -> String {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative) = lower[search_from..].find(prefix) else {
            return value;
        };
        let start = search_from + relative;
        if start > 0 && is_secret_token(value.as_bytes()[start - 1]) {
            search_from = start + prefix.len();
            continue;
        }
        let mut end = start + prefix.len();
        while end < value.len() && is_secret_token(value.as_bytes()[end]) {
            end += 1;
        }
        if end == start + prefix.len() {
            search_from = end;
            continue;
        }
        value.replace_range(start..end, REDACTED);
        search_from = start + REDACTED.len();
    }
}

fn mask_pii(value: String) -> String {
    let mut result = String::with_capacity(value.len());
    let mut token_start = 0;
    for (index, character) in value.char_indices() {
        if character.is_whitespace() || matches!(character, ',' | ';' | '"' | '\'' | '<' | '>') {
            push_masked_token(&mut result, &value[token_start..index]);
            result.push(character);
            token_start = index + character.len_utf8();
        }
    }
    push_masked_token(&mut result, &value[token_start..]);
    result
}

fn push_masked_token(result: &mut String, token: &str) {
    let email = token
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    let digit_count = token.bytes().filter(u8::is_ascii_digit).count();
    let phone = digit_count >= 7
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'(' | b')' | b'.'));
    if email || phone {
        result.push_str(REDACTED);
    } else {
        result.push_str(token);
    }
}

fn utf8_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

const fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

const fn is_secret_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

#[cfg(test)]
#[path = "tests/propagation_tests.rs"]
mod tests;
