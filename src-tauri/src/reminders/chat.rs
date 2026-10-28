//! 聊天里吩咐提醒：解析 → 角色复述一句 → 气泡里等用户点「好」（保存在 `commands::reminder_save`）。
//! 只在吩咐的那一刻最多调用一次模型（本地规则解析不了时）；离线 / mock 时本地规则照样能用。

use super::model::{self, ModelOutcome};
use super::parse::{self, Outcome, Parsed};
use super::{Draft, draft_from, now_local};
use crate::backends::{FinishReason, Role};
use crate::chat::{self, ChatEvent, PetState};
use crate::memory::{self, conversation};
use crate::state::AppState;
use crate::usage;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

enum Reply {
    Draft(Draft),
    Unclear,
}

/// 用模型解析（只在本地规则解析不了、且有可用模型时）
async fn ask_model(st: &AppState, text: &str) -> Option<ModelOutcome> {
    let s = st.settings();
    if s.chat_backend == "mock" {
        return None;
    }
    let now = now_local();
    let req = model::request(text, now);
    let out = tokio::time::timeout(
        Duration::from_secs(30),
        chat::complete(
            st,
            &s.chat_backend,
            req,
            usage::purpose::CHAT,
            CancellationToken::new(),
        ),
    )
    .await;
    match out {
        Ok(Ok(o)) => Some(model::parse_reply(&o.text, now)),
        Ok(Err(e)) => {
            log::info!("模型解析提醒失败：{}", e.kind());
            None
        }
        Err(_) => None,
    }
}

/// 处理一句「提醒我……」。返回 false = 其实不是在设提醒，交给普通聊天。
pub async fn handle(
    app: &AppHandle,
    text: &str,
    request_id: &str,
    channel: &Channel<ChatEvent>,
) -> bool {
    let st = app.state::<AppState>();
    let reply = match parse::parse(text, now_local()) {
        Outcome::NotReminder => return false,
        Outcome::Ok(p) => Reply::Draft(draft_from(&st, &p, now_local())),
        Outcome::Unclear => {
            st.chat.begin(request_id);
            chat::emit_pet_state(app, PetState::Thinking);
            let m = ask_model(&st, text).await;
            st.chat.end(request_id);
            match m {
                Some(ModelOutcome::NotReminder) => return false,
                Some(ModelOutcome::Ok(p)) => Reply::Draft(draft_from(&st, &p, now_local())),
                Some(ModelOutcome::Unclear) | None => Reply::Unclear,
            }
        }
    };
    let (say, draft, form) = match reply {
        Reply::Draft(d) => {
            let say = st
                .line(
                    "reminder.confirm",
                    &[
                        ("内容", d.reminder.content.clone()),
                        ("时间", d.when.clone()),
                    ],
                )
                .unwrap_or_else(|| format!("{}提醒你{}，这样可以吗？", d.when, d.reminder.content));
            (say, Some(d), false)
        }
        Reply::Unclear => {
            let say = st.line("reminder.unclear", &[]).unwrap_or_else(|| {
                "我没太听懂什么时候提醒你……可以换个说法，比如「30 分钟后提醒我收衣服」，或者点下面手动设置。".into()
            });
            (say, None, true)
        }
    };
    // 记进聊天记录：之后聊天时角色知道刚才在设提醒
    if let Some(c) = st.current_character() {
        let now = memory::now_secs();
        let paused = st.settings().memory_paused;
        let db = st.db();
        if let Ok(conv) = conversation::current_or_new(&db, &c.manifest.id, now, paused) {
            let _ = conversation::append(&db, conv, Role::User, text, now);
            let _ = conversation::append(&db, conv, Role::Assistant, &say, now);
        }
    }
    chat::emit_pet_state(app, PetState::Happy);
    let _ = channel.send(ChatEvent::Delta { text: say.clone() });
    let _ = channel.send(ChatEvent::Done {
        text: say,
        finish: FinishReason::Stop,
        task: None,
        actions: Vec::new(),
        reminder: draft,
        reminder_form: form,
    });
    true
}

/// 测试用：本地解析 + 复述（不需要 Tauri）
pub fn local_draft(st: &AppState, text: &str) -> Option<(Parsed, Draft)> {
    match parse::parse(text, now_local()) {
        Outcome::Ok(p) => {
            let d = draft_from(st, &p, now_local());
            Some((p, d))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chiaki_confirms_in_her_own_words() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        st.update_settings(|s| s.character_id = "nanami-chiaki".into());
        let (p, d) = local_draft(&st, "半小时后提醒我收衣服").unwrap();
        assert_eq!(p.content, "收衣服");
        assert!(d.when.starts_with("30 分钟后"));
        // 到时说的话在创建时就定好了，用的是千秋自己的台词
        assert!(d.reminder.line.contains("收衣服"));
        let chiaki = crate::lines::Lines::load_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../characters/nanami-chiaki/lines"),
        );
        assert!(chiaki.has("reminder.fire"));
        let say = st
            .line(
                "reminder.confirm",
                &[("内容", "收衣服".into()), ("时间", d.when.clone())],
            )
            .unwrap();
        assert!(say.contains("收衣服") && say.contains("30 分钟后"));
        // 没写台词的角色用通用台词
        st.update_settings(|s| s.character_id = "nailong".into());
        assert!(st.line("reminder.unclear", &[]).is_some());
        assert!(local_draft(&st, "今天天气真好").is_none());
    }
}
