//! 一轮对话的编排：组装提示词 → 调用后端（流式）→ 禁词过滤 → 通知前端 → 记录历史。

use crate::backends::{
    self, BackendConfig, BackendError, ChatBackend, ChatMessage, ChatOutcome, ChatRequest,
    FinishReason, Role,
};
use crate::canon;
use crate::character::{self, Character};
use crate::filter::{BannedWords, StreamFilter};
use crate::memory::{self, conversation, forget};
use crate::prompt::{self, MemorySnippet, PromptBudget, PromptParts};
use crate::state::AppState;
use crate::task_marker::{self, MarkerFilter};
use crate::usage;
use serde::Serialize;
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

/// 发给 chat 窗口的流式事件
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Delta {
        text: String,
    },
    Done {
        text: String,
        finish: FinishReason,
        /// 角色提议交给执行工具的任务（需要用户在卡片上确认）
        task: Option<String>,
    },
    Error {
        kind: String,
        /// 角色口吻的一句话
        message: String,
        /// 真实错误信息（已脱敏）
        detail: String,
        /// 用户该怎么办
        hint: String,
    },
}

/// 桌宠的表情状态（发给 pet 窗口）
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetState {
    Idle,
    Thinking,
    Talking,
    Happy,
    Sad,
    Sleep,
}

struct Active {
    id: String,
    cancel: CancellationToken,
}

/// 正在进行的生成（对话历史在数据库里，见 memory::conversation）
#[derive(Default)]
pub struct ChatState {
    active: Mutex<Option<Active>>,
}

impl ChatState {
    /// 是否正在生成回复
    pub fn is_generating(&self) -> bool {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// 开始新一轮：取消正在进行的生成，登记新的取消令牌
    fn begin(&self, id: &str) -> CancellationToken {
        let mut a = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = a.take() {
            old.cancel.cancel();
        }
        let cancel = CancellationToken::new();
        *a = Some(Active {
            id: id.to_string(),
            cancel: cancel.clone(),
        });
        cancel
    }

    fn end(&self, id: &str) {
        let mut a = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if a.as_ref().is_some_and(|x| x.id == id) {
            *a = None;
        }
    }

    /// 停止生成；返回是否真的停止了某个请求
    pub fn stop(&self, id: &str) -> bool {
        let a = self.active.lock().unwrap_or_else(|e| e.into_inner());
        match a.as_ref() {
            Some(x) if x.id == id || id.is_empty() => {
                x.cancel.cancel();
                true
            }
            _ => false,
        }
    }
}

/// 调用后端并实时过滤禁词。`on_delta` 收到的是过滤后的文本。
pub async fn generate(
    backend: &dyn ChatBackend,
    req: ChatRequest,
    banned: BannedWords,
    cancel: CancellationToken,
    on_delta: &mut (dyn FnMut(&str) + Send),
) -> Result<ChatOutcome, BackendError> {
    let mut filter = StreamFilter::new(banned.clone());
    let mut shown = String::new();
    let mut sink = |s: &str| {
        let out = filter.push(s);
        if !out.is_empty() {
            shown.push_str(&out);
            on_delta(&out);
        }
    };
    let mut outcome = backend.stream_chat(req, &mut sink, cancel).await?;
    let rest = filter.finish();
    if !rest.is_empty() {
        shown.push_str(&rest);
        on_delta(&rest);
    }
    // 以过滤后的完整文本为准（也兜底那些不走流式的后端）
    outcome.text = banned.apply(&outcome.text);
    if outcome.text.is_empty() {
        outcome.text = shown;
    }
    Ok(outcome)
}

/// 当前使用的对话后端（M2 起根据设置选择）
/// 选好的后端：实例 + 对应配置
pub struct Selected {
    pub backend: Box<dyn ChatBackend>,
    pub cfg: BackendConfig,
}

/// 按配置 id 创建后端（需要 Key 的从系统凭据管理器读取）
pub fn select_backend(state: &AppState, id: &str) -> Result<Selected, BackendError> {
    let cfg = state
        .settings()
        .backend(id)
        .cloned()
        .ok_or(BackendError::NotConfigured)?;
    let kind = backends::find_kind(&cfg.kind).ok_or(BackendError::NotConfigured)?;
    let key = if kind.needs_key {
        match state.secrets.get(&cfg.id) {
            Ok(Some(k)) => Some(k),
            Ok(None) => return Err(BackendError::NotConfigured),
            Err(e) => {
                log::warn!("读取 Key 失败：{e}");
                return Err(BackendError::NotConfigured);
            }
        }
    } else {
        None
    };
    Ok(Selected {
        backend: (kind.create)(&cfg, key, state.http.clone()),
        cfg,
    })
}

/// 调用前检查花费上限（mock 不花钱，不检查）
pub fn check_budget(state: &AppState, cfg: &BackendConfig) -> Result<(), BackendError> {
    if cfg.kind == "mock" {
        return Ok(());
    }
    let (day, month) = usage::today();
    let budget = state.settings().budget;
    usage::check_budget(&state.db(), &budget, &day, &month)
}

fn request_tokens(req: &ChatRequest) -> usize {
    backends::estimate_tokens(&req.system_text())
        + req
            .messages_for_api()
            .iter()
            .map(|m| backends::estimate_tokens(&m.content))
            .sum::<usize>()
}

/// 记录用量。中途取消等情况拿不到接口返回的用量时，按字数估算（宁可多估）。
pub fn record_usage(
    state: &AppState,
    cfg: &BackendConfig,
    purpose: &str,
    req_tokens: usize,
    out: &ChatOutcome,
) {
    let mut u = out.usage;
    if u.input_tokens == 0 && u.output_tokens == 0 && cfg.kind != "mock" {
        u.input_tokens = req_tokens as u64;
        u.output_tokens = backends::estimate_tokens(&out.text) as u64;
    }
    let cost = cfg.price.cost(&u);
    let rec = usage::Record {
        backend_id: &cfg.id,
        model: &cfg.model,
        purpose,
        usage: u,
        cost_usd: cost,
    };
    if let Err(e) = usage::record(&state.db(), &rec) {
        log::warn!("记录用量失败：{e}");
    }
}

/// 不需要流式显示的一次性调用（后台记忆整理、测试连接）：检查上限 → 调用 → 记录用量
pub async fn complete(
    state: &AppState,
    backend_id: &str,
    req: ChatRequest,
    purpose: &str,
    cancel: CancellationToken,
) -> Result<ChatOutcome, BackendError> {
    let sel = select_backend(state, backend_id)?;
    check_budget(state, &sel.cfg)?;
    let tokens = request_tokens(&req);
    let mut sink = |_: &str| {};
    let out = sel.backend.stream_chat(req, &mut sink, cancel).await?;
    record_usage(state, &sel.cfg, purpose, tokens, &out);
    Ok(out)
}

fn now_text() -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    let wd =
        ["一", "二", "三", "四", "五", "六", "日"][now.weekday().num_days_from_monday() as usize];
    format!(
        "{} 星期{} {}",
        now.format("%Y-%m-%d"),
        wd,
        now.format("%H:%M")
    )
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

/// 出错时角色说的话
pub fn voiced_error(err: &BackendError, c: Option<&Character>) -> String {
    let lines = c.map(|c| c.manifest.lines.clone()).unwrap_or_default();
    let s = seed();
    match err {
        BackendError::Network(_) | BackendError::Timeout => character::pick_line(
            &lines.offline,
            "呜……好像连不上网了，等网络好了再叫我吧。",
            s,
        ),
        BackendError::BudgetExceeded(_) => character::pick_line(
            &lines.budget,
            "今天（或这个月）的花费已经到上限啦，我先安静一会儿～",
            s,
        ),
        BackendError::NotConfigured => character::pick_line(
            &lines.not_configured,
            "我还没连上任何模型呢，先去设置里帮我填一下 Key 吧！",
            s,
        ),
        _ => character::pick_line(
            &lines.error,
            "哎呀，出了点问题……点「查看详情」看看是怎么回事吧。",
            s,
        ),
    }
}

/// 一轮对话需要的上下文
#[derive(Debug, Default)]
pub struct TurnContext {
    pub conversation_id: Option<i64>,
    /// 本次会话之前的消息（不含本轮）
    pub history: Vec<ChatMessage>,
    /// 用户档案（稳定，进可缓存的系统提示词）
    pub profile: Vec<String>,
    /// 本轮相关记忆
    pub memories: Vec<MemorySnippet>,
    /// 程序给模型的提示（比如「刚才删除了记忆」）
    pub notes: Vec<String>,
    /// 存进聊天记录的用户消息（「忘掉」指令不原样保存，免得又被记住）
    pub stored_user_text: String,
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

/// 准备一轮对话：找到/新建会话、处理「忘掉这件事」、检索相关记忆
pub fn prepare_turn(
    state: &AppState,
    character_id: &str,
    user_text: &str,
    now: i64,
    extra_notes: Vec<String>,
) -> TurnContext {
    let paused = state.settings().memory_paused;
    let db = state.db();
    let mut t = TurnContext {
        stored_user_text: user_text.to_string(),
        notes: extra_notes,
        ..Default::default()
    };
    let conv = match conversation::current_or_new(&db, character_id, now, paused) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("打开会话失败：{e}");
            return t;
        }
    };
    t.conversation_id = Some(conv);

    let forget_req = forget::detect(user_text);
    if let Some(f) = &forget_req {
        let mut dropped_turn = false;
        let target = match &f.target {
            Some(x) => Some(x.clone()),
            // 「忘掉这件事」= 刚才那一轮
            None => {
                let last = conversation::forget_last_turn(&db, conv).ok().flatten();
                dropped_turn = last.is_some();
                last
            }
        };
        let removed = target
            .as_deref()
            .map(|x| {
                forget::forget_matching(&db, x)
                    .map(|v| v.len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        log::info!("「忘掉」指令：删除了 {removed} 条记忆");
        t.stored_user_text = "（我请你忘掉了一件事）".into();
        t.notes.push(format!(
            "用户要求你忘掉一件事。程序已经删除了 {removed} 条相关的长期记忆{}。请用你的口吻简短确认已经忘掉了，不要复述被忘掉的内容。",
            if dropped_turn { "和刚才那段对话" } else { "" }
        ));
    }

    t.history = conversation::messages(&db, conv).unwrap_or_default();

    // 「忘掉」的这一轮不检索，免得把要忘的内容又带回来
    if forget_req.is_none() {
        match memory::retrieve(&db, user_text, character_id, now) {
            Ok(r) => {
                let mut used: Vec<i64> = r.profile.iter().map(|m| m.id).collect();
                t.profile = r.profile.into_iter().map(|m| m.content).collect();
                for (m, _) in r.related {
                    used.push(m.id);
                    let label = if m.kind == memory::KIND_SUMMARY {
                        format!("{} 的聊天摘要", m.day)
                    } else {
                        format!(
                            "{}，{}",
                            memory::source::label(&m.source_kind),
                            day_of(m.updated_at)
                        )
                    };
                    t.memories.push(MemorySnippet {
                        text: m.content,
                        source: label,
                        untrusted: !m.trusted,
                    });
                }
                let _ = memory::mark_used(&db, &used, now);
            }
            Err(e) => log::warn!("检索记忆失败：{e}"),
        }
    }
    t
}

pub fn emit_pet_state(app: &AppHandle, s: PetState) {
    let _ = app.emit_to("pet", "pet-state", s);
}

/// 执行一轮对话。用户消息在成功或失败时都会记入历史。
pub async fn run_turn(
    app: AppHandle,
    user_text: String,
    request_id: String,
    channel: Channel<ChatEvent>,
) {
    run_turn_with_notes(app, user_text, request_id, channel, Vec::new()).await
}

/// 同上，并附带给模型的程序提示（比如任务执行结果）
pub async fn run_turn_with_notes(
    app: AppHandle,
    user_text: String,
    request_id: String,
    channel: Channel<ChatEvent>,
    notes: Vec<String>,
) {
    let state = app.state::<AppState>();
    let cancel = state.chat.begin(&request_id);
    let character = state.current_character();
    let character_id = character
        .as_ref()
        .map(|c| c.manifest.id.clone())
        .unwrap_or_else(|| "default".into());
    // 禁词和自定义要求 = 全局 + 当前角色专属
    let prefs = state.settings().effective_prefs(&character_id);
    let now = memory::now_secs();
    let turn = prepare_turn(&state, &character_id, &user_text, now, notes);

    let req = {
        let (name, persona) = character
            .as_ref()
            .map(|c| (c.manifest.name.clone(), c.persona.clone()))
            .unwrap_or_else(|| ("小伙伴".into(), String::new()));
        // 角色往事：按用户这句话和角色上一句回复里的关键词取用
        let last_reply = turn
            .history
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .map(|m| m.content.as_str())
            .unwrap_or("");
        let canon = character
            .as_ref()
            .map(|c| canon::recall(&c.canon, &user_text, last_reply))
            .unwrap_or_default();
        let now_text = now_text();
        let parts = PromptParts {
            character_name: &name,
            persona: &persona,
            user_rules: &prefs.custom_rules,
            banned_words: &prefs.banned_words,
            profile_facts: &turn.profile,
            canon: &canon,
            memories: &turn.memories,
            notes: &turn.notes,
            history: &turn.history,
            user_text: &user_text,
            now: &now_text,
        };
        prompt::build(&parts, PromptBudget::default())
    };
    if let Some(conv) = turn.conversation_id
        && let Err(e) =
            conversation::append(&state.db(), conv, Role::User, &turn.stored_user_text, now)
    {
        log::warn!("保存聊天记录失败：{e}");
    }
    emit_pet_state(&app, PetState::Thinking);

    let chat_id = state.settings().chat_backend;
    let req_tokens = request_tokens(&req);
    let selected = select_backend(&state, &chat_id).and_then(|sel| {
        check_budget(&state, &sel.cfg)?;
        Ok(sel)
    });
    let result = match selected {
        Ok(sel) => {
            let mut talking = false;
            let ch = channel.clone();
            let app2 = app.clone();
            // 任务提议标记 [[任务：…]] 不显示给用户
            let mut marker = MarkerFilter::default();
            let r = {
                let marker = &mut marker;
                let mut on_delta = move |s: &str| {
                    let visible = marker.push(s);
                    if visible.is_empty() {
                        return;
                    }
                    if !talking {
                        talking = true;
                        emit_pet_state(&app2, PetState::Talking);
                    }
                    let _ = ch.send(ChatEvent::Delta { text: visible });
                };
                generate(
                    sel.backend.as_ref(),
                    req,
                    BannedWords::new(&prefs.banned_words),
                    cancel,
                    &mut on_delta,
                )
                .await
            };
            let rest = marker.finish();
            if !rest.is_empty() {
                let _ = channel.send(ChatEvent::Delta { text: rest });
            }
            if let Ok(out) = &r {
                record_usage(&state, &sel.cfg, usage::purpose::CHAT, req_tokens, out);
            }
            r
        }
        Err(e) => Err(e),
    };
    state.chat.end(&request_id);

    match result {
        Ok(mut out) => {
            let (visible, task) = task_marker::strip(&out.text);
            out.text = visible;
            if !out.text.is_empty()
                && let Some(conv) = turn.conversation_id
                && let Err(e) = conversation::append(
                    &state.db(),
                    conv,
                    Role::Assistant,
                    &out.text,
                    memory::now_secs(),
                )
            {
                log::warn!("保存聊天记录失败：{e}");
            }
            log::info!(
                "对话完成：输入 {} / 输出 {} token，结束原因 {:?}",
                out.usage.input_tokens,
                out.usage.output_tokens,
                out.finish
            );
            emit_pet_state(
                &app,
                if out.finish == FinishReason::Cancelled {
                    PetState::Idle
                } else {
                    PetState::Happy
                },
            );
            let _ = channel.send(ChatEvent::Done {
                text: out.text,
                finish: out.finish,
                task,
            });
        }
        Err(e) => {
            log::warn!("对话失败：{}", e.kind());
            emit_pet_state(&app, PetState::Sad);
            let _ = channel.send(ChatEvent::Error {
                kind: e.kind().into(),
                message: voiced_error(&e, character.as_ref()),
                detail: e.to_string(),
                hint: e.hint().into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::SystemBlock;
    use crate::backends::mock::MockBackend;

    fn req(text: &str) -> ChatRequest {
        ChatRequest {
            system: vec![SystemBlock {
                text: "s".into(),
                cacheable: true,
            }],
            messages: vec![ChatMessage::user(text)],
            context: None,
            max_tokens: 100,
            temperature: None,
        }
    }

    #[tokio::test]
    async fn generate_filters_banned_words_in_stream_and_result() {
        let b = MockBackend::instant();
        let mut shown = String::new();
        let mut sink = |s: &str| shown.push_str(s);
        let out = generate(
            &b,
            req("你好"),
            BannedWords::new(["mock", "测试"]),
            CancellationToken::new(),
            &mut sink,
        )
        .await
        .unwrap();
        assert!(!shown.contains("mock") && !shown.contains("测试"));
        assert!(shown.contains("****"));
        assert_eq!(out.text, shown);
    }

    #[test]
    fn begin_cancels_previous_and_stop_matches_id() {
        let s = ChatState::default();
        let c1 = s.begin("a");
        let c2 = s.begin("b");
        assert!(c1.is_cancelled());
        assert!(!s.stop("a"));
        assert!(!c2.is_cancelled());
        assert!(s.stop("b"));
        assert!(c2.is_cancelled());
        s.end("b");
        assert!(!s.stop("b"));
    }

    #[test]
    fn voiced_error_uses_character_lines() {
        assert!(voiced_error(&BackendError::Timeout, None).contains("网"));
    }
}
