#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MiddleTruncation<'a> {
    pub prefix: &'a str,
    pub suffix: &'a str,
    pub removed_bytes: usize,
    pub removed_chars: usize,
}

impl MiddleTruncation<'_> {
    pub fn into_string(self, marker: &str) -> String {
        let mut out =
            String::with_capacity(self.prefix.len() + marker.len() + self.suffix.len() + 1);
        out.push_str(self.prefix);
        out.push_str(marker);
        out.push_str(self.suffix);
        out
    }
}

const APPROX_BYTES_PER_TOKEN: usize = 4;

/// Truncate the middle of a UTF-8 string to fit within `max_bytes`, preserving
/// a prefix and suffix on character boundaries.
pub fn truncate_middle_with_byte_budget(s: &str, max_bytes: usize) -> Option<MiddleTruncation<'_>> {
    if s.is_empty() || s.len() <= max_bytes {
        return None;
    }

    if max_bytes == 0 {
        return Some(MiddleTruncation {
            prefix: "",
            suffix: "",
            removed_bytes: s.len(),
            removed_chars: s.chars().count(),
        });
    }

    let (left_budget, right_budget) = split_budget(max_bytes);
    let (removed_chars, prefix, suffix) = split_string(s, left_budget, right_budget);

    Some(MiddleTruncation {
        prefix,
        suffix,
        removed_bytes: s.len().saturating_sub(max_bytes),
        removed_chars,
    })
}

/// Truncate a string to `max_bytes` using a character-count marker.
pub fn truncate_middle_chars(s: &str, max_bytes: usize) -> String {
    let Some(truncation) = truncate_middle_with_byte_budget(s, max_bytes) else {
        return s.to_string();
    };

    let removed_chars = u64::try_from(truncation.removed_chars).unwrap_or(u64::MAX);
    let marker = format!("…{removed_chars} chars truncated…");
    truncation.into_string(&marker)
}

/// Truncate the middle of a UTF-8 string to at most `max_tokens` approximate
/// tokens, preserving the beginning and the end.
pub fn truncate_middle_with_token_budget(s: &str, max_tokens: usize) -> (String, Option<u64>) {
    if s.is_empty() {
        return (String::new(), None);
    }

    if max_tokens > 0 && s.len() <= approx_bytes_for_tokens(max_tokens) {
        return (s.to_string(), None);
    }

    let Some(truncation) = truncate_middle_with_byte_budget(s, approx_bytes_for_tokens(max_tokens))
    else {
        return (s.to_string(), None);
    };

    let removed_tokens = approx_tokens_from_byte_count(truncation.removed_bytes);
    let marker = format!("…{removed_tokens} tokens truncated…");
    let truncated = truncation.into_string(&marker);
    let total_tokens = u64::try_from(approx_token_count(s)).unwrap_or(u64::MAX);

    if truncated == s {
        (truncated, None)
    } else {
        (truncated, Some(total_tokens))
    }
}

pub fn approx_token_count(text: &str) -> usize {
    let len = text.len();
    len.saturating_add(APPROX_BYTES_PER_TOKEN.saturating_sub(1)) / APPROX_BYTES_PER_TOKEN
}

pub fn approx_bytes_for_tokens(tokens: usize) -> usize {
    tokens.saturating_mul(APPROX_BYTES_PER_TOKEN)
}

pub fn approx_tokens_from_byte_count(bytes: usize) -> u64 {
    let bytes_u64 = bytes as u64;
    bytes_u64.saturating_add((APPROX_BYTES_PER_TOKEN as u64).saturating_sub(1))
        / (APPROX_BYTES_PER_TOKEN as u64)
}

fn split_string(s: &str, beginning_bytes: usize, end_bytes: usize) -> (usize, &str, &str) {
    if s.is_empty() {
        return (0, "", "");
    }

    let len = s.len();
    let tail_start_target = len.saturating_sub(end_bytes);
    let mut prefix_end = 0usize;
    let mut suffix_start = len;
    let mut removed_chars = 0usize;
    let mut suffix_started = false;

    for (idx, ch) in s.char_indices() {
        let char_end = idx + ch.len_utf8();
        if char_end <= beginning_bytes {
            prefix_end = char_end;
            continue;
        }

        if idx >= tail_start_target {
            if !suffix_started {
                suffix_start = idx;
                suffix_started = true;
            }
            continue;
        }

        removed_chars = removed_chars.saturating_add(1);
    }

    if suffix_start < prefix_end {
        suffix_start = prefix_end;
    }

    let before = &s[..prefix_end];
    let after = &s[suffix_start..];

    (removed_chars, before, after)
}

fn split_budget(budget: usize) -> (usize, usize) {
    let left = budget / 2;
    (left, budget - left)
}

#[cfg(test)]
#[path = "truncate_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "truncate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "truncate_split_tests.rs"]
mod split_tests;
