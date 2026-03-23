use super::MiddleTruncation;
use super::split_string;
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
