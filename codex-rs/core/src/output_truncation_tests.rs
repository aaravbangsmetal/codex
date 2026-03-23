use super::TruncationPolicy;
use super::approx_token_count;
use super::approx_tokens_from_byte_count_i64;
use super::formatted_truncate_text_content_items_with_policy;
use super::truncate_function_output_items_with_policy;
use codex_protocol::models::FunctionCallOutputContentItem;
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
    assert_eq!(approx_tokens_from_byte_count_i64(-1), 0);
    assert_eq!(approx_tokens_from_byte_count_i64(0), 0);
    assert_eq!(approx_tokens_from_byte_count_i64(5), 2);
}
