//! 计划窗口（以及聊天、设置里的入口）用的命令。都要登记在 build.rs 并在对应窗口的 capability 里授权。

use super::ai::{self, Draft, Fixed, Preview};
use super::supervise::{self, PlanSettings};
use super::*;
use crate::state::AppState;
use crate::usage;
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Manager, State};
use tokio_util::sync::CancellationToken;

type CmdResult<T> = Result<T, String>;

pub(super) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub(super) fn now() -> i64 {
    crate::memory::now_secs()
}

/// 计划窗口打开时要的东西：她是谁、长什么样、先显示哪一页、设置
#[derive(Serialize)]
pub struct Boot {
    pub character: crate::commands::CharacterView,
    /// 她的几张图（idle / happy / think …，有哪张给哪张）
    pub images: BTreeMap<String, String>,
    pub view: Option<String>,
    pub settings: PlanSettings,
    pub today: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayPayload {
    #[serde(flatten)]
    pub view: TodayView,
    /// 她对今天的一句话
    pub say: String,
}

/// 打开计划窗口（可以指定先显示哪一页：today / new / plan:<id> / settings）
#[tauri::command]
pub async fn plans_open(app: AppHandle, view: Option<String>) -> CmdResult<()> {
    crate::desktop::open_plans(&app, view).map_err(err)
}

#[tauri::command]
pub fn plan_boot(state: State<'_, AppState>) -> Boot {
    let images = state
        .current_character()
        .map(|c| crate::character::appearance(&c).images)
        .unwrap_or_default();
    Boot {
        character: crate::commands::character_view(&state),
        images,
        view: state
            .plans
            .view
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take(),
        settings: state.settings().plans,
        today: day_str(today()),
    }
}

#[tauri::command]
pub fn plan_today(state: State<'_, AppState>) -> TodayPayload {
    let view = today_view(&state.db(), today(), now());
    let live: Vec<&TaskView> = view
        .tasks
        .iter()
        .filter(|t| t.task.status != "skipped")
        .collect();
    let done = live.iter().filter(|t| t.task.status == "done").count();
    let say = supervise::header_line(&state, live.len(), done, view.streak);
    TodayPayload { view, say }
}

#[tauri::command]
pub fn plan_list(state: State<'_, AppState>) -> Vec<PlanSummary> {
    summaries(&state.db(), today())
}

#[tauri::command]
pub fn plan_get(state: State<'_, AppState>, id: i64) -> CmdResult<PlanView> {
    let db = state.db();
    ensure_day(&db, today(), now());
    plan_view(&db, id, today()).ok_or_else(|| "这个计划已经不在了".into())
}

/// 别的进行中的计划，每天平均占多少分钟（给模型参考总量）
pub(super) fn load(st: &AppState, except: i64) -> Vec<(String, u32)> {
    active_plans(&st.db())
        .into_iter()
        .filter(|p| p.id != except)
        .map(|p| (p.title.clone(), p.week_minutes.iter().sum::<u32>() / 7))
        .take(8)
        .collect()
}

/// 调一次模型（失败返回 None 和原因）
pub(super) async fn ask(
    st: &AppState,
    req: crate::backends::ChatRequest,
) -> Result<String, String> {
    ask_within(st, req, 90).await
}

/// 调一次模型，最多等 `secs` 秒（计划书的路线图输出长，要多等一会儿）
pub(super) async fn ask_within(
    st: &AppState,
    req: crate::backends::ChatRequest,
    secs: u64,
) -> Result<String, String> {
    let s = st.settings();
    let out = tokio::time::timeout(
        StdDuration::from_secs(secs),
        crate::chat::complete(
            st,
            &s.chat_backend,
            req,
            usage::purpose::CHAT,
            CancellationToken::new(),
        ),
    )
    .await;
    match out {
        Ok(Ok(o)) => Ok(o.text),
        Ok(Err(e)) => {
            log::info!("计划安排调用模型失败：{}", e.kind());
            Err(e.hint().to_string())
        }
        Err(_) => Err("等了太久没有回应".into()),
    }
}

/// 新建计划：理解用户写的内容，给出一份草稿（还没保存）
#[tauri::command]
pub async fn plan_draft(app: AppHandle, text: String, fixed: Fixed) -> CmdResult<Draft> {
    simple_draft(&app, text, fixed).await
}

/// 简单计划的草稿（一个计划、调一次模型）
pub(super) async fn simple_draft(app: &AppHandle, text: String, fixed: Fixed) -> CmdResult<Draft> {
    let text: String = text.trim().chars().take(4000).collect();
    if text.is_empty() && fixed.title.trim().is_empty() {
        return Err("先写一写想做什么计划吧".into());
    }
    let st = app.state::<AppState>();
    let day = today();
    let req = ai::request(&text, &fixed, day, &load(&st, 0), None);
    crate::chat::emit_pet_state(app, crate::chat::PetState::Thinking);
    let reply = ask(&st, req).await;
    crate::chat::emit_pet_state(app, crate::chat::PetState::Idle);
    let (answer, fail) = match reply {
        Ok(r) => match ai::parse_reply(&r) {
            Some(a) => (Some(a), None),
            None => (None, Some("模型的回答没看懂".to_string())),
        },
        Err(e) => (None, Some(e)),
    };
    let mut d = ai::compose(&text, &fixed, answer, day);
    if let Some(why) = fail {
        d.notes.insert(
            0,
            format!("这次没让模型帮忙（{why}），先用本地规则拆了一下，可以自己改。"),
        );
    }
    Ok(d)
}

/// 重新安排（可以写一句要求）：只动还没做完的内容，结果是草稿，确认后用 `plan_apply` 保存
#[tauri::command]
pub async fn plan_replan(app: AppHandle, id: i64, text: String) -> CmdResult<Draft> {
    let st = app.state::<AppState>();
    let day = today();
    let (plan, items, spent) = {
        let db = st.db();
        let p = get_plan(&db, id)
            .map_err(err)?
            .ok_or("这个计划已经不在了")?;
        (p, list_items(&db, id).map_err(err)?, item_spent(&db, id))
    };
    let current = ai::Current {
        plan: plan.clone(),
        done: items
            .iter()
            .filter(|i| i.status == "done")
            .map(|i| i.title.clone())
            .collect(),
        left: items
            .iter()
            .filter(|i| i.status == "todo")
            .map(|i| {
                (
                    i.title.clone(),
                    i.detail.clone(),
                    item_left(i, spent.get(&i.id).copied().unwrap_or(0)),
                )
            })
            .collect(),
    };
    let text: String = text.trim().chars().take(1000).collect();
    let req = ai::request(
        &text,
        &Fixed::default(),
        day,
        &load(&st, id),
        Some(&current),
    );
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Thinking);
    let reply = ask(&st, req).await;
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
    let mut notes = Vec::new();
    let mut new_plan = plan.clone();
    let mut new_items: Vec<Item> = items
        .iter()
        .filter(|i| i.status != "todo")
        .cloned()
        .collect();
    match reply.map(|r| ai::parse_reply(&r)) {
        Ok(Some(a)) if plan.mode == Mode::Habit || !a.items.is_empty() => {
            if let Some(w) = a.week_minutes {
                new_plan.week_minutes = w;
            }
            if let Some(e) = a.end_day.filter(|e| *e >= day) {
                new_plan.end_day = day_str(e);
            }
            if crate::proactive::parse_hm(&a.remind_at).is_some() {
                new_plan.remind_at = a.remind_at.clone();
            }
            if !a.strategy.is_empty() {
                new_plan.strategy = a.strategy.clone();
            }
            notes.extend(a.assumptions.clone());
            if a.items.is_empty() {
                // 习惯类没给新的几项：原来的照旧
                new_items.extend(items.iter().filter(|i| i.status == "todo").cloned());
            }
            for (t, d, m) in a.items {
                // 名字没变的沿用原来那条（已经做了的分钟还算数）
                let old = items.iter().find(|i| {
                    i.status == "todo" && i.title == t && !new_items.iter().any(|n| n.id == i.id)
                });
                let mut it = old.cloned().unwrap_or_default();
                it.title = t;
                it.detail = d;
                if m > 0 {
                    // 内容类：模型给的是「还剩多久」，加上已经做了的才是总量；习惯类是每次多久
                    let done = match (plan.mode, old) {
                        (Mode::Content, Some(o)) => {
                            spent.get(&o.id).copied().unwrap_or(0).min(o.minutes)
                        }
                        _ => 0,
                    };
                    it.minutes = m + done;
                }
                if let Ok(x) = it.sanitize(plan.mode) {
                    new_items.push(x);
                }
            }
        }
        Ok(_) => {
            notes.push("模型没给出新的安排，内容先保持原样，可以直接在下面改。".into());
            new_items = items.clone();
        }
        Err(e) => {
            notes.push(format!(
                "这次没让模型帮忙（{e}），内容先保持原样，可以直接在下面改。"
            ));
            new_items = items.clone();
        }
    }
    let plan = new_plan.sanitize(day)?;
    let preview = ai::preview(&plan, &new_items, day);
    Ok(Draft {
        plan,
        items: new_items,
        source: "ai".into(),
        notes,
        preview,
    })
}

/// 草稿改了之后重新推算（不保存）
#[tauri::command]
pub fn plan_preview(plan: Plan, items: Vec<Item>) -> CmdResult<Preview> {
    let day = today();
    let plan = plan.sanitize(day)?;
    let items: Vec<Item> = items
        .into_iter()
        .take(MAX_ITEMS)
        .filter_map(|i| i.sanitize(plan.mode).ok())
        .collect();
    Ok(ai::preview(&plan, &items, day))
}

/// 保存计划（新建或修改）。`items` 为空 = 内容不变（只改参数）。保存后重排今天。
#[tauri::command]
pub fn plan_apply(app: AppHandle, plan: Plan, items: Option<Vec<Item>>) -> CmdResult<i64> {
    let st = app.state::<AppState>();
    let day = today();
    let mut plan = plan.sanitize(day)?;
    let items: Option<Vec<Item>> = items
        .map(|v| {
            v.into_iter()
                .take(MAX_ITEMS)
                .map(|i| i.sanitize(plan.mode))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let created = plan.id == 0;
    let id = {
        let db = st.db();
        if created {
            // 新建的线要指向真有的计划书
            if let Some(g) = plan.goal_id
                && goal::get_goal(&db, g).map_err(err)?.is_none()
            {
                plan.goal_id = None;
            }
            let n = list_plans(&db)
                .map_err(err)?
                .iter()
                .filter(|p| p.status != "archived")
                .count();
            if n >= MAX_PLANS {
                return Err(format!(
                    "计划太多啦（最多 {MAX_PLANS} 个），先归档或删掉一些"
                ));
            }
        } else {
            let old = get_plan(&db, plan.id)
                .map_err(err)?
                .ok_or("这个计划已经不在了")?;
            plan.created_at = old.created_at;
            plan.sort = old.sort;
            // 属于哪个计划书不能从这里改
            plan.goal_id = old.goal_id;
        }
        let tx = db.unchecked_transaction().map_err(err)?;
        let id = save_plan(&tx, &plan, now()).map_err(err)?;
        if let Some(items) = &items {
            set_items(&tx, id, items).map_err(err)?;
        }
        // 内容类的全做完了、又加了新内容：重新开始
        if plan.status == "done"
            && plan.mode == Mode::Content
            && list_items(&tx, id)
                .map_err(err)?
                .iter()
                .any(|i| i.status == "todo")
        {
            set_plan_status(&tx, id, "active", now()).map_err(err)?;
        }
        tx.commit().map_err(err)?;
        let saved = get_plan(&db, id).map_err(err)?.ok_or("保存失败")?;
        rearrange_any(&db, &saved, day, now()).map_err(err)?;
        id
    };
    if created {
        let first = {
            let db = st.db();
            day_tasks(&db, &day_str(day))
                .into_iter()
                .find(|t| t.plan_id == Some(id))
                .map(|t| t.title)
        };
        let label = plan.title.clone();
        let say = match &first {
            Some(f) => st
                .line(
                    "plan.created",
                    &[("计划", label.clone()), ("事项", f.clone())],
                )
                .unwrap_or_else(|| format!("「{label}」的计划开始啦，今天先做「{f}」。")),
            None => st
                .line("plan.created_later", &[("计划", label.clone())])
                .unwrap_or_else(|| format!("「{label}」的计划记下了，到时候我陪你一起。")),
        };
        // 记进聊天记录：之后聊天她知道你开始了这个计划
        crate::chat::record_exchange(&st, None, &say);
        if st.settings().plans.celebrate {
            let mut item =
                crate::proactive::Item::new(crate::proactive::Kind::Random, say, now() + 60);
            item.ephemeral = true;
            crate::proactive::say_now(&app, item);
        }
    }
    supervise::changed(&app);
    Ok(id)
}

#[tauri::command]
pub fn plan_set_status(app: AppHandle, id: i64, status: String) -> CmdResult<()> {
    if !matches!(status.as_str(), "active" | "paused" | "done" | "archived") {
        return Err("不认识的状态".into());
    }
    let st = app.state::<AppState>();
    {
        let db = st.db();
        let p = get_plan(&db, id)
            .map_err(err)?
            .ok_or("这个计划已经不在了")?;
        set_plan_status(&db, id, &status, now()).map_err(err)?;
        // 暂停 / 归档：今天排好还没动的撤掉；恢复：重新排今天
        let mut p = p;
        p.status = status.clone();
        rearrange_any(&db, &p, today(), now()).map_err(err)?;
    }
    supervise::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn plan_delete(app: AppHandle, id: i64) -> CmdResult<()> {
    delete_plan(&app.state::<AppState>().db(), id).map_err(err)?;
    supervise::changed(&app);
    Ok(())
}

/// 新加或修改一项。修改过的事项算用户的安排（改计划时不会被重排掉）。
#[tauri::command]
pub fn plan_task_save(app: AppHandle, task: Task) -> CmdResult<i64> {
    let st = app.state::<AppState>();
    let day = today();
    let mut t = task.sanitize(day)?;
    let id = {
        let db = st.db();
        if t.id > 0 {
            let old = get_task(&db, t.id)
                .map_err(err)?
                .ok_or("这一项已经不在了")?;
            // 只改界面上能改的；其余沿用
            let mut x = old.clone();
            x.title = t.title;
            x.minutes = t.minutes;
            x.start = t.start;
            x.note = t.note;
            if x.title != old.title || x.minutes != old.minutes {
                x.source = "user".into();
            }
            if x.start != old.start {
                x.reminded = x.day == day_str(day) && start_passed(&x.start, now());
            }
            if t.day != old.day {
                if x.orig_day.is_empty() {
                    x.orig_day = old.day.clone();
                }
                x.day = t.day;
                x.source = "user".into();
            }
            save_task(&db, &x, now()).map_err(err)?
        } else {
            if let Some(pid) = t.plan_id
                && get_plan(&db, pid).map_err(err)?.is_none()
            {
                t.plan_id = None;
            }
            if let Some(iid) = t.item_id
                && get_item(&db, iid)
                    .map_err(err)?
                    .is_none_or(|i| Some(i.plan_id) != t.plan_id)
            {
                t.item_id = None;
            }
            t.source = "user".into();
            t.status = "todo".into();
            t.spent = 0;
            t.done_at = None;
            // 加的时候开始时间就已经过了：不再「到点提醒」
            t.reminded = t.day == day_str(day) && start_passed(&t.start, now());
            t.sort = db
                .query_row(
                    "SELECT COALESCE(MAX(sort), 0) + 1 FROM plan_tasks WHERE day = ?1",
                    [&t.day],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            save_task(&db, &t, now()).map_err(err)?
        }
    };
    supervise::changed(&app);
    Ok(id)
}

/// 做完一项：`spent` 实际用了多久（空 = 按计划），`finished` 对应的内容是不是做完了（空 = 按排的时候算的）
#[tauri::command]
pub fn plan_task_done(
    app: AppHandle,
    id: i64,
    spent: Option<u32>,
    finished: Option<bool>,
) -> CmdResult<DoneEffect> {
    let st = app.state::<AppState>();
    let (eff, title, plan_title) = {
        let db = st.db();
        let eff = complete_task(&db, id, spent, finished, now())?;
        let t = get_task(&db, id).map_err(err)?.ok_or("这一项已经不在了")?;
        let pt = t
            .plan_id
            .and_then(|p| get_plan(&db, p).ok().flatten())
            .map(|p| p.title)
            .unwrap_or_default();
        (eff, t.title, pt)
    };
    supervise::on_done(&app, &title, &plan_title, eff);
    supervise::changed(&app);
    Ok(eff)
}

/// 改回没做 / 跳过
#[tauri::command]
pub fn plan_task_status(app: AppHandle, id: i64, status: String) -> CmdResult<()> {
    set_task_status(&app.state::<AppState>().db(), id, &status, now())?;
    supervise::changed(&app);
    Ok(())
}

/// 挪到另一天（YYYY-MM-DD）
#[tauri::command]
pub fn plan_task_move(app: AppHandle, id: i64, day: String) -> CmdResult<()> {
    let to = parse_day(&day).ok_or("日期不对")?;
    if to < today() {
        return Err("不能挪到已经过去的日子".into());
    }
    move_task(&app.state::<AppState>().db(), id, to, now())?;
    supervise::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn plan_task_delete(app: AppHandle, id: i64) -> CmdResult<()> {
    delete_task(&app.state::<AppState>().db(), id).map_err(err)?;
    supervise::changed(&app);
    Ok(())
}

/// 写一条备注（`plan_id` 空 = 写在这一天上）
#[tauri::command]
pub fn plan_note_add(app: AppHandle, note: Note) -> CmdResult<i64> {
    let content: String = note.content.trim().chars().take(2000).collect();
    if content.is_empty() {
        return Err("备注是空的".into());
    }
    let st = app.state::<AppState>();
    let id = {
        let db = st.db();
        let plan_id = match note.plan_id {
            Some(p) if get_plan(&db, p).map_err(err)?.is_some() => Some(p),
            _ => None,
        };
        let day = parse_day(&note.day).unwrap_or_else(today);
        add_note(
            &db,
            &Note {
                id: 0,
                plan_id,
                day: day_str(day),
                content,
                created_at: 0,
            },
            now(),
        )
        .map_err(err)?
    };
    supervise::changed(&app);
    Ok(id)
}

#[tauri::command]
pub fn plan_note_delete(app: AppHandle, id: i64) -> CmdResult<()> {
    delete_note(&app.state::<AppState>().db(), id).map_err(err)?;
    supervise::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn plan_settings_save(app: AppHandle, settings: PlanSettings) -> CmdResult<PlanSettings> {
    let st = app.state::<AppState>();
    let s = st.update_settings(|s| s.plans = settings);
    crate::desktop::broadcast_settings_changed(&app);
    st.plans.changed.notify_one();
    Ok(s.plans)
}

/// 开始专注计时（`minutes` 为空 = 用设置里的默认时长）
#[tauri::command]
pub fn plan_focus_start(
    app: AppHandle,
    task_id: i64,
    minutes: Option<u32>,
) -> CmdResult<supervise::FocusView> {
    let m = minutes.unwrap_or(app.state::<AppState>().settings().plans.focus_minutes);
    supervise::start_focus(&app, task_id, m, 0)
}

/// 停止专注：`done` = 顺便把这一项记成做完（按实际专注的时间）
#[tauri::command]
pub fn plan_focus_stop(app: AppHandle, done: bool) -> CmdResult<()> {
    supervise::stop_focus(&app, done)
}

#[tauri::command]
pub fn plan_focus_state(state: State<'_, AppState>) -> Option<supervise::FocusView> {
    supervise::focus_view(&state)
}
