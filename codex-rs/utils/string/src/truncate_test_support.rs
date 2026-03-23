use super::approx_bytes_for_tokens;
use super::truncate_middle_chars;
use super::truncate_middle_with_token_budget as truncate_middle_tokens;

#[derive(Clone, Copy)]
pub(super) enum TruncationPolicy {
    Bytes(usize),
    Tokens(usize),
}

pub(super) fn formatted_truncate_text(content: &str, policy: TruncationPolicy) -> String {
    if content.len() <= byte_budget(policy) {
        return content.to_string();
    }

    let total_lines = content.lines().count();
    let result = truncate_text(content, policy);
    format!("Total output lines: {total_lines}\n\n{result}")
}

pub(super) fn truncate_text(content: &str, policy: TruncationPolicy) -> String {
    match policy {
        TruncationPolicy::Bytes(bytes) => truncate_middle_chars(content, bytes),
        TruncationPolicy::Tokens(tokens) => truncate_middle_tokens(content, tokens).0,
    }
}

pub(super) fn truncate_with_token_budget(
    content: &str,
    policy: TruncationPolicy,
) -> (String, Option<u64>) {
    match policy {
        TruncationPolicy::Bytes(bytes) => (truncate_middle_chars(content, bytes), None),
        TruncationPolicy::Tokens(tokens) => truncate_middle_tokens(content, tokens),
    }
}

fn byte_budget(policy: TruncationPolicy) -> usize {
    match policy {
        TruncationPolicy::Bytes(bytes) => bytes,
        TruncationPolicy::Tokens(tokens) => approx_bytes_for_tokens(tokens),
    }
}
