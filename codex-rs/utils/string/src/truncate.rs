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
mod tests {
    use super::MiddleTruncation;
    use super::split_string;
    use super::truncate_middle_chars;
    use super::truncate_middle_with_byte_budget;
    use pretty_assertions::assert_eq;

    #[test]
    fn split_string_works() {
        assert_eq!(split_string("hello world", 5, 5), (1, "hello", "world"));
        assert_eq!(split_string("abc", 0, 0), (3, "", ""));
    }

    #[test]
    fn split_string_handles_empty_string() {
        assert_eq!(split_string("", 4, 4), (0, "", ""));
    }

    #[test]
    fn split_string_only_keeps_prefix_when_tail_budget_is_zero() {
        assert_eq!(split_string("abcdef", 3, 0), (3, "abc", ""));
    }

    #[test]
    fn split_string_only_keeps_suffix_when_prefix_budget_is_zero() {
        assert_eq!(split_string("abcdef", 0, 3), (3, "", "def"));
    }

    #[test]
    fn split_string_handles_overlapping_budgets_without_removal() {
        assert_eq!(split_string("abcdef", 4, 4), (0, "abcd", "ef"));
    }

    #[test]
    fn split_string_respects_utf8_boundaries() {
        assert_eq!(split_string("😀abc😀", 5, 5), (1, "😀a", "c😀"));

        assert_eq!(split_string("😀😀😀😀😀", 1, 1), (5, "", ""));
        assert_eq!(split_string("😀😀😀😀😀", 7, 7), (3, "😀", "😀"));
        assert_eq!(split_string("😀😀😀😀😀", 8, 8), (1, "😀😀", "😀😀"));
    }

    #[test]
    fn truncate_middle_with_byte_budget_returns_none_under_limit() {
        assert_eq!(truncate_middle_with_byte_budget("example output", 32), None);
    }

    #[test]
    fn truncate_middle_with_byte_budget_reports_removed_content() {
        assert_eq!(
            truncate_middle_with_byte_budget("hello world", 10),
            Some(MiddleTruncation {
                prefix: "hello",
                suffix: "world",
                removed_bytes: 1,
                removed_chars: 1,
            })
        );
    }

    #[test]
    fn truncate_middle_with_byte_budget_handles_zero_budget() {
        assert_eq!(
            truncate_middle_with_byte_budget("abc", 0),
            Some(MiddleTruncation {
                prefix: "",
                suffix: "",
                removed_bytes: 3,
                removed_chars: 3,
            })
        );
    }

    #[test]
    fn truncate_middle_chars_handles_utf8_content() {
        let s = "😀😀😀😀😀😀😀😀😀😀\nsecond line with text\n";
        assert_eq!(
            truncate_middle_chars(s, 20),
            "😀😀…21 chars truncated…with text\n"
        );
    }
}
