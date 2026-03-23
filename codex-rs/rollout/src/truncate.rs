#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TruncationPolicy {
    Bytes(usize),
}

impl TruncationPolicy {
    fn byte_budget(self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes,
        }
    }
}

pub(crate) fn truncate_text(content: &str, policy: TruncationPolicy) -> String {
    truncate_with_byte_estimate(content, policy)
}

fn truncate_with_byte_estimate(s: &str, policy: TruncationPolicy) -> String {
    if s.is_empty() {
        return String::new();
    }

    let total_chars = s.chars().count();
    let max_bytes = policy.byte_budget();
    if max_bytes == 0 {
        let marker = format_truncation_marker(
            policy,
            removed_units_for_source(policy, s.len(), total_chars),
        );
        return marker;
    }
    if s.len() <= max_bytes {
        return s.to_string();
    }

    let total_bytes = s.len();
    let (left_budget, right_budget) = split_budget(max_bytes);
    let (removed_chars, left, right) = split_string(s, left_budget, right_budget);
    let marker = format_truncation_marker(
        policy,
        removed_units_for_source(policy, total_bytes.saturating_sub(max_bytes), removed_chars),
    );

    assemble_truncated_output(left, right, marker.as_str())
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

fn format_truncation_marker(policy: TruncationPolicy, removed_count: u64) -> String {
    match policy {
        TruncationPolicy::Bytes(_) => format!("…{removed_count} chars truncated…"),
    }
}

fn split_budget(budget: usize) -> (usize, usize) {
    let left = budget / 2;
    (left, budget - left)
}

fn removed_units_for_source(
    policy: TruncationPolicy,
    _removed_bytes: usize,
    removed_chars: usize,
) -> u64 {
    match policy {
        TruncationPolicy::Bytes(_) => u64::try_from(removed_chars).unwrap_or(u64::MAX),
    }
}

fn assemble_truncated_output(prefix: &str, suffix: &str, marker: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + marker.len() + suffix.len() + 1);
    out.push_str(prefix);
    out.push_str(marker);
    out.push_str(suffix);
    out
}
