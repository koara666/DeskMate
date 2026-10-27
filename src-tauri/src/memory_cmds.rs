//! 设置窗口「记忆」页的命令：查看、搜索、编辑、删除、手动添加、暂停、导出、清空。

use crate::memory::{self, ListFilter, ListResult, NewMemory, backup, source};
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct ScopeOption {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct MemoryOverview {
    pub stats: memory::Stats,
    pub paused: bool,
    pub last_backup: Option<String>,
    /// 可选的范围：global + 每个角色
    pub scopes: Vec<ScopeOption>,
}

#[tauri::command]
pub fn memory_overview(state: State<'_, AppState>) -> MemoryOverview {
    let mut scopes = vec![ScopeOption {
        id: memory::SCOPE_GLOBAL.into(),
        name: "关于你（所有角色共享）".into(),
    }];
    scopes.extend(state.characters().into_iter().map(|c| ScopeOption {
        id: c.manifest.id,
        name: format!("和{}之间", c.manifest.name),
    }));
    MemoryOverview {
        stats: memory::stats(&state.db()),
        paused: state.settings().memory_paused,
        last_backup: backup::latest_backup(&state.paths.backups()),
        scopes,
    }
}

#[tauri::command]
pub fn memory_list(state: State<'_, AppState>, filter: ListFilter) -> CmdResult<ListResult> {
    memory::list(&state.db(), &filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_update(
    state: State<'_, AppState>,
    id: i64,
    content: String,
    importance: i64,
) -> CmdResult<()> {
    memory::update(&state.db(), id, &content, importance, memory::now_secs())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_delete(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    memory::delete(&state.db(), id).map_err(|e| e.to_string())
}

/// 用户手动添加一条事实（可信来源）
#[tauri::command]
pub fn memory_add(
    state: State<'_, AppState>,
    content: String,
    scope: String,
    importance: i64,
) -> CmdResult<i64> {
    let m = NewMemory {
        kind: memory::KIND_FACT.into(),
        content,
        scope,
        category: "other".into(),
        source_kind: source::USER_EDIT.into(),
        source_ref: String::new(),
        importance,
        day: String::new(),
    };
    memory::insert(&state.db(), &m, memory::now_secs()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_set_paused(state: State<'_, AppState>, paused: bool) {
    state.update_settings(|s| s.memory_paused = paused);
    log::info!("记忆{}", if paused { "已暂停" } else { "已恢复" });
}

/// 一键清空：必须传入确认文字「清空」（界面上还会再弹一次确认）
#[tauri::command]
pub fn memory_clear_all(state: State<'_, AppState>, confirm: String) -> CmdResult<()> {
    if confirm.trim() != "清空" {
        return Err("确认文字不对，没有清空".into());
    }
    // 清空前先备份一份，万一点错还能找回来
    {
        let db = state.db();
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = state
            .paths
            .backups()
            .join(format!("before-clear-{stamp}.db"));
        crate::db::vacuum_into(&db, &path).map_err(|e| e.to_string())?;
        memory::clear_all(&db).map_err(|e| e.to_string())?;
    }
    log::info!("用户清空了全部记忆和聊天记录");
    Ok(())
}

fn export_markdown(items: &[memory::Memory]) -> String {
    let mut md = String::from("# DeskMate 记忆导出\n\n");
    md.push_str(&format!(
        "导出时间：{}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M")
    ));
    for (title, kind) in [
        ("长期事实", memory::KIND_FACT),
        ("聊天摘要", memory::KIND_SUMMARY),
    ] {
        md.push_str(&format!("## {title}\n\n"));
        for m in items.iter().filter(|m| m.kind == kind) {
            let scope = if m.scope == memory::SCOPE_GLOBAL {
                "关于你".to_string()
            } else {
                format!("角色 {}", m.scope)
            };
            let status = if m.status == "active" {
                ""
            } else {
                "（历史版本）"
            };
            md.push_str(&format!(
                "- {}{status}  \n  范围：{scope}｜重要度：{}｜来源：{}{}\n",
                m.content,
                m.importance,
                source::label(&m.source_kind),
                if m.day.is_empty() {
                    String::new()
                } else {
                    format!("｜日期：{}", m.day)
                }
            ));
        }
        md.push('\n');
    }
    md
}

/// 导出全部记忆（JSON + 可读的 Markdown）到数据目录的 exports 文件夹，并打开该文件夹
#[tauri::command]
pub fn memory_export(_app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let all = memory::list(
        &state.db(),
        &ListFilter {
            include_history: true,
            limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let dir = state.paths.root().join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let json = serde_json::to_string_pretty(&all.items).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("memory-{stamp}.json")), json).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!("memory-{stamp}.md")),
        export_markdown(&all.items),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    }
    Ok(dir.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_export_is_readable() {
        let m = memory::Memory {
            id: 1,
            kind: "fact".into(),
            content: "用户叫小林".into(),
            scope: "global".into(),
            category: "profile".into(),
            source_kind: "chat".into(),
            source_ref: String::new(),
            trusted: true,
            importance: 5,
            created_at: 0,
            updated_at: 0,
            last_used_at: None,
            use_count: 0,
            status: "active".into(),
            superseded_by: None,
            day: String::new(),
        };
        let md = export_markdown(&[m]);
        assert!(md.contains("## 长期事实"));
        assert!(md.contains("- 用户叫小林"));
        assert!(md.contains("范围：关于你｜重要度：5｜来源：聊天"));
    }
}
