//! 后台维护：每 5 分钟醒来一次（空闲时几乎不占 CPU）。
//! 1. 每天第一次：备份数据库（保留 7 份）。
//! 2. 已经结束的会话：用后台模型（建议最便宜的）提取摘要和事实。
//!    - 暂停记忆期间产生的会话：不提取，直接删除。
//!    - 记忆暂停中：先不处理，恢复后再提取。
//!    - 没配置模型或到了花费上限：留到下次。

use crate::backends::BackendError;
use crate::chat;
use crate::memory::{self, backup, conversation, extract};
use crate::state::AppState;
use crate::usage;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

const FIRST_DELAY: Duration = Duration::from_secs(60);
const INTERVAL: Duration = Duration::from_secs(300);
/// 每轮最多处理几次会话（控制花费）
const BATCH: i64 = 3;

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_DELAY).await;
        loop {
            run_once(&app).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

pub async fn run_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let (day, _) = usage::today();
        let db = state.db();
        match backup::daily_backup(&db, &state.paths.backups(), &day) {
            Ok(Some(p)) => log::info!("已备份数据库：{}", p.display()),
            Ok(None) => {}
            Err(e) => log::warn!("备份数据库失败：{e}"),
        }
    }
    let n = process_pending(&state).await;
    if n > 0 {
        log::info!("整理了 {n} 次会话的记忆");
    }
}

/// 处理已结束的会话，返回成功整理的数量
pub async fn process_pending(state: &AppState) -> usize {
    let now = memory::now_secs();
    let pending = conversation::pending(&state.db(), now, BATCH).unwrap_or_default();
    let mut done = 0;
    for conv in pending {
        if conv.no_memory {
            if let Err(e) = conversation::delete_conversation(&state.db(), conv.id) {
                log::warn!("删除暂停期间的会话失败：{e}");
            }
            continue;
        }
        if state.settings().memory_paused {
            break;
        }
        match process_one(state, &conv).await {
            Ok(()) => done += 1,
            // 没有可用模型 / 到了花费上限：这轮先停，不算失败
            Err(Some(BackendError::NotConfigured | BackendError::BudgetExceeded(_))) => break,
            Err(e) => {
                log::warn!(
                    "整理会话 {} 的记忆失败：{}",
                    conv.id,
                    e.map(|e| e.kind().to_string())
                        .unwrap_or_else(|| "解析失败".into())
                );
                let _ = conversation::mark_extracted(&state.db(), conv.id, false);
            }
        }
    }
    done
}

fn day_of(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

/// 整理一次会话。Err(None) 表示模型回复无法解析。
async fn process_one(
    state: &AppState,
    conv: &conversation::PendingConversation,
) -> Result<(), Option<BackendError>> {
    let transcript = conversation::messages(&state.db(), conv.id).unwrap_or_default();
    let user_text: String = transcript
        .iter()
        .filter(|m| m.role == crate::backends::Role::User)
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if user_text.trim().is_empty() {
        let _ = conversation::mark_extracted(&state.db(), conv.id, true);
        return Ok(());
    }
    let name = state
        .characters()
        .into_iter()
        .find(|c| c.manifest.id == conv.character_id)
        .map(|c| c.manifest.name)
        .unwrap_or_else(|| conv.character_id.clone());
    // 和这次聊天相关的已有事实：模型据此判断重复和冲突，也只能更新这些
    let known: Vec<memory::Memory> = {
        let query: String = user_text.chars().take(600).collect();
        memory::retrieve(&state.db(), &query, &conv.character_id, memory::now_secs())
            .map(|r| {
                r.profile
                    .into_iter()
                    .chain(r.related.into_iter().map(|(m, _)| m))
                    .filter(|m| m.kind == memory::KIND_FACT)
                    .take(15)
                    .collect()
            })
            .unwrap_or_default()
    };
    let req = extract::build_request(&name, &transcript, &known);
    let s = state.settings();
    let backend_id = if s.background_backend.is_empty() {
        s.chat_backend.clone()
    } else {
        s.background_backend.clone()
    };
    let out = tokio::time::timeout(
        Duration::from_secs(90),
        chat::complete(
            state,
            &backend_id,
            req,
            usage::purpose::MEMORY,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap_or(Err(BackendError::Timeout))
    .map_err(Some)?;
    let ex = extract::parse(&out.text).map_err(|e| {
        log::warn!("记忆提取结果无法解析：{e}");
        None
    })?;
    let rep = extract::apply(
        &state.db(),
        conv.id,
        &conv.character_id,
        &day_of(conv.started_at),
        &ex,
        &known,
        memory::now_secs(),
    )
    .map_err(|e| {
        log::warn!("写入记忆失败：{e}");
        None
    })?;
    log::info!(
        "会话 {}：新增事实 {}，更新 {}，重复 {}，拒绝 {}，摘要 {}",
        conv.id,
        rep.facts_added,
        rep.updated,
        rep.duplicates,
        rep.rejected,
        rep.summary_added
    );
    let _ = conversation::mark_extracted(&state.db(), conv.id, true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Role;
    use crate::paths::DataPaths;
    use crate::secrets::MemoryStore;

    fn test_state(dir: &std::path::Path) -> AppState {
        let paths = DataPaths::from_root(dir);
        paths.ensure().unwrap();
        let conn = crate::db::open(&paths.db(), &paths.backups()).unwrap();
        let builtin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters");
        AppState::new(paths, conn, Some(builtin), Box::new(MemoryStore::default()))
    }

    #[tokio::test]
    async fn chat_extract_retrieve_forget_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path());
        // 默认聊天后端是 mock：不联网、不花钱
        let t0 = memory::now_secs() - 3 * 3600;
        {
            let db = state.db();
            let conv = conversation::current_or_new(&db, "nailong", t0, false).unwrap();
            conversation::append(&db, conv, Role::User, "你好，我叫小林，我住在杭州", t0).unwrap();
            conversation::append(&db, conv, Role::Assistant, "你好小林！", t0 + 5).unwrap();
            conversation::append(&db, conv, Role::User, "我的密码是 abc12345", t0 + 10).unwrap();
        }
        assert_eq!(process_pending(&state).await, 1);
        let stats = memory::stats(&state.db());
        assert_eq!(stats.facts, 2, "叫小林、住在杭州");
        assert_eq!(stats.summaries, 1);
        let all = memory::list(&state.db(), &memory::ListFilter::default()).unwrap();
        assert!(
            !all.items.iter().any(|m| m.content.contains("abc12345")),
            "密码不会进入记忆"
        );
        // 已处理的会话不会重复处理
        assert_eq!(process_pending(&state).await, 0);

        // 下一轮对话：名字作为用户档案注入，所有角色共享
        let turn = chat::prepare_turn(
            &state,
            "naiwa",
            "你还记得我住在哪吗",
            memory::now_secs(),
            vec![],
        );
        assert!(turn.profile.iter().any(|p| p.contains("小林")));
        assert!(turn.memories.iter().any(|m| m.text.contains("杭州")));
        assert!(turn.history.is_empty(), "新角色、新会话，没有短期历史");

        // 「忘掉」：删除相关记忆，指令本身不原样存进聊天记录
        let turn = chat::prepare_turn(
            &state,
            "naiwa",
            "忘掉我住在杭州这件事",
            memory::now_secs(),
            vec![],
        );
        assert_eq!(turn.stored_user_text, "（我请你忘掉了一件事）");
        assert!(
            turn.notes[0].contains("删除了 2 条"),
            "事实和提到它的摘要都删掉"
        );
        assert!(turn.memories.is_empty());
        let all = memory::list(&state.db(), &memory::ListFilter::default()).unwrap();
        assert!(!all.items.iter().any(|m| m.content.contains("住在杭州")));
        assert!(all.items.iter().any(|m| m.content.contains("小林")));
    }

    #[tokio::test]
    async fn paused_memory_is_not_extracted_and_deleted_after_end() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path());
        state.update_settings(|s| s.memory_paused = true);
        let t0 = memory::now_secs() - 3 * 3600;
        {
            let db = state.db();
            let conv = conversation::current_or_new(&db, "nailong", t0, true).unwrap();
            conversation::append(&db, conv, Role::User, "我叫小林", t0).unwrap();
        }
        process_pending(&state).await;
        assert_eq!(memory::stats(&state.db()).facts, 0);
        assert_eq!(
            memory::stats(&state.db()).conversations,
            0,
            "暂停期间的会话结束后被删除"
        );
    }
}
