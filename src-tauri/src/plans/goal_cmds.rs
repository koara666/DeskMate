//! 计划书的命令（计划窗口用）。都要登记在 build.rs 并在 capabilities/plans.json 里授权。
//! 模型只在用户点了的时候调：新建（先问 → 路线图 → 细化最近三周）、调整下周、按改过的原文重新理解。

use super::ai::{Draft, Fixed};
use super::cmds::{ask_within, err, load, now, simple_draft};

/// 路线图输出长（几千字），细化次之：比普通的计划多等一会儿
const ROADMAP_SECS: u64 = 240;
const EXPAND_SECS: u64 = 180;
use super::goal::{self, GoalFull, GoalSummary, GoalView, Milestone, Qa, Viz};
use super::goal_ai::{self, Question, Reply};
use super::supervise;
use super::*;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;

/// 还没保存的计划书（新建、调整下周、重新理解的结果）
#[derive(Debug, Clone, Serialize)]
pub struct GoalDraft {
    pub full: GoalFull,
    pub viz: Viz,
    /// ai（模型排的）/ local（本地规则）
    pub source: String,
    /// 她的假设、提示
    pub notes: Vec<String>,
}

/// 「让她来安排」的结果：简单的计划、先问几个问题、或者一份计划书
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Think {
    Plan { draft: Box<Draft> },
    Questions { questions: Vec<Question> },
    Goal { draft: Box<GoalDraft> },
}

/// 告诉计划窗口她在想哪一步
fn stage(app: &AppHandle, s: &str) {
    let _ = app.emit_to("plans", "plans-thinking", s.to_string());
}

/// 第二步：把最近三周会排到的粗内容细化（模型不行就本地按要点拆）
async fn first_expand(app: &AppHandle, full: &mut GoalFull, notes: &mut Vec<String>) {
    let st = app.state::<AppState>();
    let day = today();
    let soon = goal::draft_soon(full, day, goal_ai::FIRST_EXPAND_DAYS);
    let segs = goal_ai::segments(full, &soon, &HashMap::new());
    if segs.is_empty() {
        return;
    }
    stage(app, "细化最近三周…");
    let req = goal_ai::expand_request(full, &segs, None, "", day);
    let got = ask_within(&st, req, EXPAND_SECS)
        .await
        .map(|r| goal_ai::parse_expand(&r));
    match got {
        Ok(Some((got, _))) => {
            goal_ai::apply_expand(full, &segs, &got, &HashMap::new());
        }
        other => {
            let n = goal_ai::local_expand(full, &segs, &HashMap::new());
            let why = match other {
                Err(e) => e,
                _ => "回答没看懂".into(),
            };
            if n > 0 {
                notes.push(format!(
                    "细化最近的内容时没让模型帮忙（{why}），先按要点拆了一下。"
                ));
            }
        }
    }
}

/// 新建：简单的计划照旧一步到位；复杂的先问（可以跳过）、再出计划书。
/// `answers` 为空 = 第一次（允许先问）；给了（哪怕是空数组）= 回答完 / 跳过了。`complex` 空 = 让她判断
#[tauri::command]
pub async fn plan_think(
    app: AppHandle,
    text: String,
    fixed: Fixed,
    answers: Option<Vec<Qa>>,
    complex: Option<bool>,
) -> CmdResult<Think> {
    let text: String = text.trim().chars().take(8000).collect();
    if text.is_empty() {
        return Err("先写一写想做什么计划吧".into());
    }
    if !complex.unwrap_or_else(|| goal_ai::looks_complex(&text)) {
        let text: String = text.chars().take(4000).collect();
        return Ok(Think::Plan {
            draft: Box::new(simple_draft(&app, text, fixed).await?),
        });
    }
    let st = app.state::<AppState>();
    let day = today();
    let ask_first = answers.is_none();
    let answers: Vec<Qa> = answers.unwrap_or_default().into_iter().take(6).collect();
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Thinking);
    stage(
        &app,
        if ask_first {
            "看看要不要先问你几句…"
        } else {
            "画路线图…"
        },
    );
    let req = goal_ai::roadmap_request(&text, &answers, ask_first, day, &load(&st, 0), None);
    let reply = ask_within(&st, req, ROADMAP_SECS).await;
    let (roadmap, mut notes, source) = match reply.as_deref().map(goal_ai::parse_roadmap) {
        Ok(Some(Reply::Questions(q))) if ask_first => {
            crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
            return Ok(Think::Questions { questions: q });
        }
        Ok(Some(Reply::Roadmap(r))) => (*r, Vec::new(), "ai"),
        // 回答完了还在问：当成没看懂
        other => {
            let why = match other {
                Err(e) => e.clone(),
                _ => "回答没看懂".into(),
            };
            (
                goal_ai::local_roadmap(&text, &answers, day),
                vec![format!(
                    "这次没让模型帮忙（{why}），先按本地规则做了一份，可以点开每个阶段、每条线自己改。"
                )],
                "local",
            )
        }
    };
    let result = goal_ai::compose(&text, &answers, roadmap, day, None);
    let (mut full, more) = match result {
        Ok(x) => x,
        Err(e) => {
            crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
            return Err(e);
        }
    };
    notes.extend(more);
    if source == "ai" {
        first_expand(&app, &mut full, &mut notes).await;
    } else {
        let soon = goal::draft_soon(&full, day, goal_ai::FIRST_EXPAND_DAYS);
        let segs = goal_ai::segments(&full, &soon, &HashMap::new());
        goal_ai::local_expand(&mut full, &segs, &HashMap::new());
    }
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
    let full = full.sanitize(day)?;
    let viz = goal::draft_viz(&full, day, &HashMap::new());
    if !viz.short.is_empty() {
        notes.push("有的线按现在每天的时间排不完，可以多给点时间、把阶段拉长，或者删掉一些内容（时间线上标了 ⚠ 的）。".into());
    }
    Ok(Think::Goal {
        draft: Box::new(GoalDraft {
            full,
            viz,
            source: source.into(),
            notes,
        }),
    })
}

/// 草稿改了之后重新推算（不保存）
#[tauri::command]
pub fn goal_preview(state: State<'_, AppState>, full: GoalFull) -> CmdResult<Viz> {
    let day = today();
    let full = full.sanitize(day)?;
    let spent = goal::goal_spent(&state.db(), &full);
    Ok(goal::draft_viz(&full, day, &spent))
}

/// 保存整个计划书（新建或修改）。`with_items` = false：只改了阶段、节点、参数，不动内容。保存后重排今天。
#[tauri::command]
pub fn goal_apply(app: AppHandle, full: GoalFull, with_items: bool) -> CmdResult<i64> {
    let st = app.state::<AppState>();
    let day = today();
    let full = full.sanitize(day)?;
    let created = full.goal.id <= 0;
    let id = {
        let db = st.db();
        if created {
            let n = goal::list_goals(&db)
                .map_err(err)?
                .iter()
                .filter(|g| g.status != "archived")
                .count();
            if n >= goal::MAX_GOALS {
                return Err(format!(
                    "计划书太多啦（最多 {} 个），先归档或删掉一些",
                    goal::MAX_GOALS
                ));
            }
        }
        let tx = db.unchecked_transaction().map_err(err)?;
        let id = goal::apply_full(&tx, &full, with_items || created, now()).map_err(err)?;
        if with_items {
            goal::reset_expanded(&tx, id).map_err(err)?;
        }
        tx.commit().map_err(err)?;
        goal::rearrange_today(&db, id, day, now()).map_err(err)?;
        id
    };
    if created {
        let title = full.goal.title.clone();
        let first = {
            let db = st.db();
            let ids: Vec<i64> = goal::goal_tracks(&db, id).iter().map(|p| p.id).collect();
            day_tasks(&db, &day_str(day))
                .into_iter()
                .find(|t| t.plan_id.is_some_and(|p| ids.contains(&p)))
                .map(|t| t.title)
        };
        let say = match &first {
            Some(f) => st
                .line(
                    "plan.goal_created",
                    &[("计划", title.clone()), ("事项", f.clone())],
                )
                .unwrap_or_else(|| format!("「{title}」的计划书定下来啦，今天先从「{f}」开始。")),
            None => st
                .line("plan.created_later", &[("计划", title.clone())])
                .unwrap_or_else(|| format!("「{title}」的计划书记下了，到时候我陪你一起。")),
        };
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
pub fn goal_get(state: State<'_, AppState>, id: i64) -> CmdResult<GoalView> {
    let db = state.db();
    ensure_day(&db, today(), now());
    goal::goal_view(&db, id, today()).ok_or_else(|| "这个计划书已经不在了".into())
}

#[tauri::command]
pub fn goal_list(state: State<'_, AppState>) -> Vec<GoalSummary> {
    goal::summaries(&state.db(), today())
}

#[tauri::command]
pub fn goal_set_status(app: AppHandle, id: i64, status: String) -> CmdResult<()> {
    if !matches!(status.as_str(), "active" | "paused" | "done" | "archived") {
        return Err("不认识的状态".into());
    }
    let st = app.state::<AppState>();
    {
        let db = st.db();
        if !goal::set_goal_status(&db, id, &status, now()).map_err(err)? {
            return Err("这个计划书已经不在了".into());
        }
        goal::rearrange_today(&db, id, today(), now()).map_err(err)?;
    }
    supervise::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn goal_delete(app: AppHandle, id: i64) -> CmdResult<()> {
    goal::delete_goal(&app.state::<AppState>().db(), id).map_err(err)?;
    supervise::changed(&app);
    Ok(())
}

/// 节点达标了没有（passed / missed / skipped / todo）
#[tauri::command]
pub fn goal_milestone_check(
    app: AppHandle,
    id: i64,
    status: String,
    result: Option<String>,
) -> CmdResult<Milestone> {
    let m = goal::check_milestone(
        &app.state::<AppState>().db(),
        id,
        &status,
        result.as_deref(),
        now(),
    )?;
    if m.status == "passed" {
        supervise::react(&app, "plan.milestone_passed", &[("节点", m.title.clone())]);
    }
    supervise::changed(&app);
    Ok(m)
}

/// 调整下周：按这周的情况，让她把接下来两周要做的细化 / 重新分（调一次模型）。结果是草稿，确认后用 `goal_apply` 保存。
#[tauri::command]
pub async fn goal_expand(app: AppHandle, id: i64, text: String) -> CmdResult<GoalDraft> {
    let st = app.state::<AppState>();
    let day = today();
    let (full, spent, segs, week) = {
        let db = st.db();
        ensure_day(&db, day, now());
        let full = goal::load_full(&db, id).ok_or("这个计划书已经不在了")?;
        if full.goal.status != "active" {
            return Err("计划书暂停着，先恢复再调整吧".into());
        }
        let spent = goal::goal_spent(&db, &full);
        let soon = goal::soon_items(
            &db,
            &full.goal,
            day,
            day + Duration::days(goal::EXPAND_DAYS),
            false,
        );
        let segs = goal_ai::segments(&full, &soon, &spent);
        // 这周到今天为止（周一自查的话看上一周）
        let (mon, _) =
            goal::week_of(day - Duration::days(if weekday_index(day) == 0 { 1 } else { 0 }));
        let week = goal::week_stat(&db, id, mon, day);
        (full, spent, segs, week)
    };
    if segs.is_empty() {
        return Err(
            "接下来两周没有要排的内容了。可以在时间线上点开阶段加一些，或者改原文让她重新理解。"
                .into(),
        );
    }
    let text: String = text.trim().chars().take(1000).collect();
    let req = goal_ai::expand_request(&full, &segs, Some(&week), &text, day);
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Thinking);
    let reply = ask_within(&st, req, EXPAND_SECS).await;
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
    let mut full = full;
    let mut notes = Vec::new();
    let source = match reply.map(|r| goal_ai::parse_expand(&r)) {
        Ok(Some((got, note))) => {
            let n = goal_ai::apply_expand(&mut full, &segs, &got, &spent);
            if !note.is_empty() {
                full.goal.strategy = note.clone();
            }
            notes.push(format!("接下来两周的 {n} 段内容重新排了，看看行不行。"));
            "ai"
        }
        other => {
            let why = match other {
                Err(e) => e,
                _ => "回答没看懂".into(),
            };
            let n = goal_ai::local_expand(&mut full, &segs, &spent);
            notes.push(if n > 0 {
                format!("这次没让模型帮忙（{why}），先按要点在本地拆了 {n} 段。")
            } else {
                format!("这次没让模型帮忙（{why}），内容先保持原样，可以直接改。")
            });
            "local"
        }
    };
    let full = full.sanitize(day)?;
    let viz = goal::draft_viz(&full, day, &spent);
    Ok(GoalDraft {
        full,
        viz,
        source: source.into(),
        notes,
    })
}

/// 原文整段改了之后：让她按原文重新理解（调一次模型），做过的留着。结果是草稿。
#[tauri::command]
pub async fn goal_rethink(app: AppHandle, id: i64, doc: String) -> CmdResult<GoalDraft> {
    let st = app.state::<AppState>();
    let day = today();
    let doc: String = doc.trim().chars().take(20000).collect();
    if doc.is_empty() {
        return Err("原文是空的".into());
    }
    let (old, spent) = {
        let db = st.db();
        let old = goal::load_full(&db, id).ok_or("这个计划书已经不在了")?;
        let spent = goal::goal_spent(&db, &old);
        (old, spent)
    };
    let progress = goal_ai::progress_text(&old, &spent);
    let req = goal_ai::roadmap_request(
        &doc,
        &old.goal.answers,
        false,
        day,
        &load(&st, 0),
        Some(&progress),
    );
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Thinking);
    let reply = ask_within(&st, req, ROADMAP_SECS).await;
    crate::chat::emit_pet_state(&app, crate::chat::PetState::Idle);
    let r = match reply.map(|r| goal_ai::parse_roadmap(&r)) {
        Ok(Some(Reply::Roadmap(r))) => *r,
        Ok(_) => {
            return Err("她没看懂改过的原文，结构先没动；可以只保存文字，或者再改改试试。".into());
        }
        Err(e) => {
            return Err(format!(
                "这次没让模型帮忙（{e}），结构先没动；可以只保存文字。"
            ));
        }
    };
    let keep = parse_day(&old.goal.start_day);
    let (mut new, mut notes) = goal_ai::compose(&doc, &old.goal.answers, r, day, keep)?;
    new.goal.doc = doc;
    let (merged, more) = goal_ai::merge_rethink(&old, &spent, new);
    notes.extend(more);
    let full = merged.sanitize(day)?;
    let viz = goal::draft_viz(&full, day, &spent);
    Ok(GoalDraft {
        full,
        viz,
        source: "ai".into(),
        notes,
    })
}

/// 按现在的计划书重新生成一份原文（本地，不调模型）
#[tauri::command]
pub fn goal_render_doc(full: GoalFull) -> String {
    goal_ai::render_doc(&full)
}
