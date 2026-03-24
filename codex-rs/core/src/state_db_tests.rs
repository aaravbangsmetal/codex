use super::*;
use crate::config::test_config;
use crate::rollout::list::parse_cursor;
use pretty_assertions::assert_eq;
use std::sync::Arc;

#[test]
fn cursor_to_anchor_normalizes_timestamp_format() {
    let uuid = Uuid::new_v4();
    let ts_str = "2026-01-27T12-34-56";
    let token = format!("{ts_str}|{uuid}");
    let cursor = parse_cursor(token.as_str()).expect("cursor should parse");
    let anchor = cursor_to_anchor(Some(&cursor)).expect("anchor should parse");

    let naive =
        NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H-%M-%S").expect("ts should parse");
    let expected_ts = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
        .with_nanosecond(0)
        .expect("nanosecond");

    assert_eq!(anchor.id, uuid);
    assert_eq!(anchor.ts, expected_ts);
}

#[tokio::test]
async fn get_state_db_reuses_cached_runtime() {
    let config = test_config();
    let runtime = init(&config).await.expect("initialize state db");
    runtime
        .mark_backfill_complete(None)
        .await
        .expect("mark backfill complete");

    let cached = get_state_db(&config).await.expect("get cached runtime");

    assert!(Arc::ptr_eq(&runtime, &cached));
}

#[tokio::test]
async fn init_creates_distinct_runtime_for_session_owned_pool() {
    let config = test_config();

    let first = init(&config).await.expect("initialize first state db");
    first
        .mark_backfill_complete(None)
        .await
        .expect("mark backfill complete");

    let second = init(&config).await.expect("initialize second state db");

    assert!(!Arc::ptr_eq(&first, &second));
}

#[tokio::test]
async fn open_if_present_without_provider_initializes_and_caches_runtime() {
    let config = test_config();
    let seeded_runtime = codex_state::StateRuntime::init(
        config.sqlite_home.clone(),
        config.model_provider_id.clone(),
    )
    .await
    .expect("initialize seeded runtime");
    seeded_runtime
        .mark_backfill_complete(None)
        .await
        .expect("mark backfill complete");
    drop(seeded_runtime);

    let opened = open_if_present(config.sqlite_home.as_path(), "")
        .await
        .expect("initialize uncached runtime");

    let reused = get_state_db(&config)
        .await
        .expect("reuse initialized runtime");

    assert!(Arc::ptr_eq(&opened, &reused));
}

#[tokio::test]
async fn create_runtime_prunes_stale_cache_entries() {
    let first = test_config();
    let second = test_config();

    let first_runtime = create_runtime(
        first.sqlite_home.as_path(),
        first.model_provider_id.as_str(),
    )
    .await
    .expect("initialize first runtime");
    drop(first_runtime);

    let _second_runtime = create_runtime(
        second.sqlite_home.as_path(),
        second.model_provider_id.as_str(),
    )
    .await
    .expect("initialize second runtime");

    let cache = STATE_DB_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(!cache.contains_key(first.sqlite_home.as_path()));
    assert!(cache.contains_key(second.sqlite_home.as_path()));
}
