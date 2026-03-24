use std::ops::Mul;

use crate::models::FunctionCallOutputContentItem;
use crate::openai_models::TruncationMode;
use crate::openai_models::TruncationPolicyConfig;
pub use crate::protocol::TruncationPolicy;
pub use codex_utils_string::approx_bytes_for_tokens;
pub use codex_utils_string::approx_token_count;
pub use codex_utils_string::approx_tokens_from_byte_count;
use codex_utils_string::truncate_middle_chars;
use codex_utils_string::truncate_middle_with_token_budget;

impl From<TruncationPolicyConfig> for TruncationPolicy {
    fn from(config: TruncationPolicyConfig) -> Self {
        match config.mode {
            TruncationMode::Bytes => Self::Bytes(config.limit as usize),
            TruncationMode::Tokens => Self::Tokens(config.limit as usize),
        }
    }
}

impl TruncationPolicy {
    pub fn token_budget(&self) -> usize {
        match self {
            TruncationPolicy::Bytes(bytes) => {
                usize::try_from(approx_tokens_from_byte_count(*bytes)).unwrap_or(usize::MAX)
            }
            TruncationPolicy::Tokens(tokens) => *tokens,
        }
    }

    pub fn byte_budget(&self) -> usize {
        match self {
            TruncationPolicy::Bytes(bytes) => *bytes,
            TruncationPolicy::Tokens(tokens) => approx_bytes_for_tokens(*tokens),
        }
    }
}

impl Mul<f64> for TruncationPolicy {
    type Output = Self;

    fn mul(self, multiplier: f64) -> Self::Output {
        match self {
            TruncationPolicy::Bytes(bytes) => {
                TruncationPolicy::Bytes((bytes as f64 * multiplier).ceil() as usize)
            }
            TruncationPolicy::Tokens(tokens) => {
                TruncationPolicy::Tokens((tokens as f64 * multiplier).ceil() as usize)
            }
        }
    }
}

pub fn formatted_truncate_text(content: &str, policy: TruncationPolicy) -> String {
    if content.len() <= policy.byte_budget() {
        return content.to_string();
    }

    let total_lines = content.lines().count();
    let result = truncate_text(content, policy);
    format!("Total output lines: {total_lines}\n\n{result}")
}

pub fn truncate_text(content: &str, policy: TruncationPolicy) -> String {
    match policy {
        TruncationPolicy::Bytes(bytes) => truncate_middle_chars(content, bytes),
        TruncationPolicy::Tokens(tokens) => truncate_middle_with_token_budget(content, tokens).0,
    }
}

pub fn formatted_truncate_text_content_items_with_policy(
    items: &[FunctionCallOutputContentItem],
    policy: TruncationPolicy,
) -> (Vec<FunctionCallOutputContentItem>, Option<usize>) {
    let mut combined = String::new();
    let mut saw_text_segment = false;
    for item in items {
        let FunctionCallOutputContentItem::InputText { text } = item else {
            continue;
        };

        if saw_text_segment {
            combined.push('\n');
        }
        combined.push_str(text);
        saw_text_segment = true;
    }

    if !saw_text_segment {
        return (items.to_vec(), None);
    }

    if combined.len() <= policy.byte_budget() {
        return (items.to_vec(), None);
    }

    let mut out = vec![FunctionCallOutputContentItem::InputText {
        text: formatted_truncate_text(&combined, policy),
    }];
    out.extend(items.iter().filter_map(|item| match item {
        FunctionCallOutputContentItem::InputImage { image_url, detail } => {
            Some(FunctionCallOutputContentItem::InputImage {
                image_url: image_url.clone(),
                detail: *detail,
            })
        }
        FunctionCallOutputContentItem::InputText { .. } => None,
    }));

    (out, Some(approx_token_count(&combined)))
}

pub fn truncate_function_output_items_with_policy(
    items: &[FunctionCallOutputContentItem],
    policy: TruncationPolicy,
) -> Vec<FunctionCallOutputContentItem> {
    let mut out: Vec<FunctionCallOutputContentItem> = Vec::with_capacity(items.len());
    let mut remaining_budget = match policy {
        TruncationPolicy::Bytes(_) => policy.byte_budget(),
        TruncationPolicy::Tokens(_) => policy.token_budget(),
    };
    let mut omitted_text_items = 0usize;

    for item in items {
        match item {
            FunctionCallOutputContentItem::InputText { text } => {
                if remaining_budget == 0 {
                    omitted_text_items += 1;
                    continue;
                }

                let cost = match policy {
                    TruncationPolicy::Bytes(_) => text.len(),
                    TruncationPolicy::Tokens(_) => approx_token_count(text),
                };

                if cost <= remaining_budget {
                    out.push(FunctionCallOutputContentItem::InputText { text: text.clone() });
                    remaining_budget = remaining_budget.saturating_sub(cost);
                } else {
                    let snippet_policy = match policy {
                        TruncationPolicy::Bytes(_) => TruncationPolicy::Bytes(remaining_budget),
                        TruncationPolicy::Tokens(_) => TruncationPolicy::Tokens(remaining_budget),
                    };
                    let snippet = truncate_text(text, snippet_policy);
                    if snippet.is_empty() {
                        omitted_text_items += 1;
                    } else {
                        out.push(FunctionCallOutputContentItem::InputText { text: snippet });
                    }
                    remaining_budget = 0;
                }
            }
            FunctionCallOutputContentItem::InputImage { image_url, detail } => {
                out.push(FunctionCallOutputContentItem::InputImage {
                    image_url: image_url.clone(),
                    detail: *detail,
                });
            }
        }
    }

    if omitted_text_items > 0 {
        out.push(FunctionCallOutputContentItem::InputText {
            text: format!("[omitted {omitted_text_items} text items ...]"),
        });
    }

    out
}

pub fn approx_tokens_from_byte_count_i64(bytes: i64) -> i64 {
    if bytes <= 0 {
        return 0;
    }

    let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
    i64::try_from(approx_tokens_from_byte_count(bytes)).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FunctionCallOutputContentItem;
    use pretty_assertions::assert_eq;

    fn text_item(text: &str) -> FunctionCallOutputContentItem {
        FunctionCallOutputContentItem::InputText {
            text: text.to_string(),
        }
    }

    fn image_item(image_url: &str) -> FunctionCallOutputContentItem {
        FunctionCallOutputContentItem::InputImage {
            image_url: image_url.to_string(),
            detail: None,
        }
    }

    #[test]
    fn truncate_bytes_less_than_placeholder_returns_placeholder() {
        let content = "example output";

        assert_eq!(
            "Total output lines: 1\n\n…13 chars truncated…t",
            formatted_truncate_text(content, TruncationPolicy::Bytes(1)),
        );
    }

    #[test]
    fn truncate_tokens_less_than_placeholder_returns_placeholder() {
        let content = "example output";

        assert_eq!(
            "Total output lines: 1\n\nex…3 tokens truncated…ut",
            formatted_truncate_text(content, TruncationPolicy::Tokens(1)),
        );
    }

    #[test]
    fn truncate_tokens_under_limit_returns_original() {
        let content = "example output";

        assert_eq!(
            content,
            formatted_truncate_text(content, TruncationPolicy::Tokens(10)),
        );
    }

    #[test]
    fn truncate_bytes_under_limit_returns_original() {
        let content = "example output";

        assert_eq!(
            content,
            formatted_truncate_text(content, TruncationPolicy::Bytes(20)),
        );
    }

    #[test]
    fn truncate_tokens_over_limit_returns_truncated() {
        let content = "this is an example of a long output that should be truncated";

        assert_eq!(
            "Total output lines: 1\n\nthis is an…10 tokens truncated… truncated",
            formatted_truncate_text(content, TruncationPolicy::Tokens(5)),
        );
    }

    #[test]
    fn truncate_bytes_over_limit_returns_truncated() {
        let content = "this is an example of a long output that should be truncated";

        assert_eq!(
            "Total output lines: 1\n\nthis is an exam…30 chars truncated…ld be truncated",
            formatted_truncate_text(content, TruncationPolicy::Bytes(30)),
        );
    }

    #[test]
    fn truncate_bytes_reports_original_line_count_when_truncated() {
        let content =
            "this is an example of a long output that should be truncated\nalso some other line";

        assert_eq!(
            "Total output lines: 2\n\nthis is an exam…51 chars truncated…some other line",
            formatted_truncate_text(content, TruncationPolicy::Bytes(30)),
        );
    }

    #[test]
    fn truncate_tokens_reports_original_line_count_when_truncated() {
        let content =
            "this is an example of a long output that should be truncated\nalso some other line";

        assert_eq!(
            "Total output lines: 2\n\nthis is an example o…11 tokens truncated…also some other line",
            formatted_truncate_text(content, TruncationPolicy::Tokens(10)),
        );
    }

    #[test]
    fn truncate_middle_bytes_handles_utf8_content() {
        let s = "😀😀😀😀😀😀😀😀😀😀\nsecond line with text\n";
        let out = truncate_text(s, TruncationPolicy::Bytes(20));
        assert_eq!(out, "😀😀…21 chars truncated…with text\n");
    }

    #[test]
    fn content_items_within_budget_are_unchanged() {
        let items = vec![text_item("alpha"), text_item(""), text_item("beta")];

        let (output, original_token_count) =
            formatted_truncate_text_content_items_with_policy(&items, TruncationPolicy::Bytes(32));

        assert_eq!(output, items);
        assert_eq!(original_token_count, None);
    }

    #[test]
    fn byte_truncation_merges_text_and_keeps_images() {
        let items = vec![
            text_item("abcd"),
            image_item("img:one"),
            text_item("efgh"),
            text_item("ijkl"),
            image_item("img:two"),
        ];

        let (output, original_token_count) =
            formatted_truncate_text_content_items_with_policy(&items, TruncationPolicy::Bytes(8));

        assert_eq!(
            output,
            vec![
                text_item("Total output lines: 3\n\nabcd…6 chars truncated…ijkl"),
                image_item("img:one"),
                image_item("img:two"),
            ]
        );
        assert_eq!(original_token_count, Some(4));
    }

    #[test]
    fn token_truncation_merges_all_text_segments() {
        let items = vec![text_item("abcdefgh"), text_item("ijklmnop")];

        let (output, original_token_count) =
            formatted_truncate_text_content_items_with_policy(&items, TruncationPolicy::Tokens(2));

        assert_eq!(
            output,
            vec![text_item(
                "Total output lines: 2\n\nabcd…3 tokens truncated…mnop"
            )]
        );
        assert_eq!(original_token_count, Some(5));
    }

    #[test]
    fn global_item_truncation_preserves_prefix_and_summarizes_omissions() {
        let chunk = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.\n";
        let limit = approx_token_count(chunk) * 3;
        let items = vec![
            text_item(chunk),
            text_item(chunk),
            image_item("img:mid"),
            text_item(&chunk.repeat(10)),
            text_item(chunk),
            text_item(chunk),
        ];

        let output =
            truncate_function_output_items_with_policy(&items, TruncationPolicy::Tokens(limit));

        assert_eq!(output.len(), 5);
        assert_eq!(output[0], items[0]);
        assert_eq!(output[1], items[1]);
        assert_eq!(output[2], items[2]);

        let truncated_text = match &output[3] {
            FunctionCallOutputContentItem::InputText { text } => text,
            other => panic!("unexpected truncated item: {other:?}"),
        };
        assert!(
            truncated_text.contains("tokens truncated"),
            "expected marker in truncated snippet: {truncated_text}"
        );

        assert_eq!(output[4], text_item("[omitted 2 text items ...]"));
    }

    #[test]
    fn byte_count_conversion_clamps_non_positive_values() {
        assert_eq!(approx_tokens_from_byte_count_i64(/*bytes*/ -1), 0);
        assert_eq!(approx_tokens_from_byte_count_i64(/*bytes*/ 0), 0);
        assert_eq!(approx_tokens_from_byte_count_i64(/*bytes*/ 5), 2);
    }
}
