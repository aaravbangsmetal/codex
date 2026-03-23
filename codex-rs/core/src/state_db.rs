use codex_protocol::ThreadId;
use codex_protocol::dynamic_tools::DynamicToolSpec;
pub use codex_rollout::db::StateDbHandle;
pub use codex_rollout::db::apply_rollout_items;
pub use codex_rollout::db::find_rollout_path_by_id;
pub use codex_rollout::db::list_thread_ids_db;
pub use codex_rollout::db::list_threads_db;
pub use codex_rollout::db::normalize_cwd_for_state_db;
pub use codex_rollout::db::open_if_present;
pub use codex_rollout::db::persist_dynamic_tools;
pub use codex_rollout::db::read_repair_rollout_path;
pub use codex_rollout::db::reconcile_rollout;
pub use codex_rollout::db::touch_thread_updated_at;
pub use codex_state::LogEntry;
use tracing::warn;

use crate::config::Config;

pub(crate) async fn init(config: &Config) -> Option<StateDbHandle> {
    codex_rollout::db::init(config).await
}

pub async fn get_state_db(config: &Config) -> Option<StateDbHandle> {
    codex_rollout::db::get_state_db(config).await
}

pub async fn get_dynamic_tools(
    context: Option<&codex_state::StateRuntime>,
    thread_id: ThreadId,
    stage: &str,
) -> Option<Vec<DynamicToolSpec>> {
    let ctx = context?;
    match ctx.get_dynamic_tools(thread_id).await {
        Ok(tools) => tools,
        Err(err) => {
            warn!("state db get_dynamic_tools failed during {stage}: {err}");
            None
        }
    }
}

pub async fn mark_thread_memory_mode_polluted(
    context: Option<&codex_state::StateRuntime>,
    thread_id: ThreadId,
    stage: &str,
) {
    let Some(ctx) = context else {
        return;
    };
    if let Err(err) = ctx.mark_thread_memory_mode_polluted(thread_id).await {
        warn!("state db mark_thread_memory_mode_polluted failed during {stage}: {err}");
    }
}

#[cfg(test)]
fn cursor_to_anchor(cursor: Option<&crate::rollout::list::Cursor>) -> Option<codex_state::Anchor> {
    use chrono::DateTime;
    use chrono::NaiveDateTime;
    use chrono::Timelike;
    use chrono::Utc;
    use uuid::Uuid;

    let cursor = cursor?;
    let value = serde_json::to_value(cursor).ok()?;
    let cursor_str = value.as_str()?;
    let (ts_str, id_str) = cursor_str.split_once('|')?;
    if id_str.contains('|') {
        return None;
    }
    let id = Uuid::parse_str(id_str).ok()?;
    let ts = if let Ok(naive) = NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H-%M-%S") {
        DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
    } else if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
        dt.with_timezone(&Utc)
    } else {
        return None;
    }
    .with_nanosecond(0)?;
    Some(codex_state::Anchor { ts, id })
}

#[cfg(test)]
#[path = "state_db_tests.rs"]
mod tests;
