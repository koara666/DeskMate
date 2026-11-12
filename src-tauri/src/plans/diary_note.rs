//! 日记里的计划情况：她写某一天的日记时，知道你那天的计划做得怎么样。
//!
//! 只拼一两句给模型看的事实（做完几项、用了多久、没做的、计划书在哪个阶段、当天节点有没有达标）。
//! 没有安排的日子返回 None。日记写的是前一天，所以这里按「计划日」查那一天落库的事项。

use super::{day_str, get_plan, goal, minutes_text};
use chrono::NaiveDate;
use rusqlite::Connection;

/// 每份名单最多列几项
const MAX_LISTED: usize = 5;
/// 事项名最多留几个字（再长截短）
const MAX_TITLE: usize = 14;
/// 最多说几本计划书
const MAX_GOALS: usize = 2;

/// 那天的计划情况
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaySummary {
    /// 写给模型看的事实
    pub text: String,
    /// 那天一共几项（跳过的不算；挪到别的日子的算没做）
    pub total: usize,
    /// 做完几项
    pub done: usize,
}

/// 事项名太长就截短
fn short(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > MAX_TITLE {
        format!("{}…", s.chars().take(MAX_TITLE).collect::<String>())
    } else {
        s.to_string()
    }
}

/// 「A、B、C」；超过 5 项只列前 5 项，后面写「等」
fn list(names: &[String]) -> String {
    let shown = names
        .iter()
        .take(MAX_LISTED)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if names.len() > MAX_LISTED {
        format!("{shown} 等")
    } else {
        shown
    }
}

/// 「计划：做完 3/4 项（共 1 小时 50 分钟）——A、B、C；没做的：D。」+ 每本计划书一句
fn render(done: &[String], undone: &[String], minutes: u32, goals: &[String]) -> String {
    let total = done.len() + undone.len();
    let mins = if minutes > 0 {
        format!("（共 {}）", minutes_text(minutes))
    } else {
        String::new()
    };
    let mut s = if undone.is_empty() {
        format!("计划：做完了 {total} 项{mins}——{}。", list(done))
    } else if done.is_empty() {
        format!(
            "计划：安排了 {total} 项，一项都没做；没做的：{}。",
            list(undone)
        )
    } else {
        format!(
            "计划：做完 {}/{total} 项{mins}——{}；没做的：{}。",
            done.len(),
            list(done),
            list(undone)
        )
    };
    for g in goals {
        s.push_str(g);
    }
    s
}

/// 「计划书「A」在「B」阶段；节点「C」达标了。」没有阶段也没有节点结果就不说
fn goal_note(title: &str, phase: Option<&str>, milestones: &[String]) -> Option<String> {
    let head = format!("计划书「{title}」");
    let mut s = match phase {
        Some(p) => std::iter::once(format!("{head}在「{p}」阶段"))
            .chain(milestones.iter().cloned())
            .collect::<Vec<_>>()
            .join("；"),
        None if milestones.is_empty() => return None,
        None => format!("{head}：{}", milestones.join("；")),
    };
    s.push('。');
    Some(s)
}

/// 那天从别的日子挪走（推到明天、没做完被挪到后来）的事项名：那天算没做
fn moved_away(db: &Connection, day: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(
        "SELECT title FROM plan_tasks WHERE orig_day = ?1 AND day <> ?1 AND status <> 'skipped' ORDER BY sort, id",
    ) else {
        return Vec::new();
    };
    stmt.query_map([day], |r| r.get::<_, String>(0))
        .map(|it| it.flatten().map(|t| short(&t)).collect())
        .unwrap_or_default()
}

/// 那天的计划情况。那天没有安排（或全跳过了）返回 None。
pub fn day_summary(db: &Connection, day: NaiveDate) -> Option<DaySummary> {
    let d = day_str(day);
    let tasks = super::day_tasks(db, &d);
    let mut done = Vec::new();
    let mut undone = Vec::new();
    let mut minutes = 0u32;
    let mut plan_ids: Vec<i64> = Vec::new();
    for t in tasks.iter().filter(|t| t.status != "skipped") {
        if t.status == "done" {
            done.push(short(&t.title));
            minutes += t.spent;
        } else {
            undone.push(short(&t.title));
        }
        if let Some(p) = t.plan_id
            && !plan_ids.contains(&p)
        {
            plan_ids.push(p);
        }
    }
    undone.extend(moved_away(db, &d));
    let total = done.len() + undone.len();
    if total == 0 {
        return None;
    }
    // 做了哪几本计划书里的事
    let mut goal_ids: Vec<i64> = Vec::new();
    for p in plan_ids {
        if let Ok(Some(plan)) = get_plan(db, p)
            && let Some(g) = plan.goal_id
            && !goal_ids.contains(&g)
        {
            goal_ids.push(g);
        }
    }
    let mut notes = Vec::new();
    for gid in goal_ids.into_iter().take(MAX_GOALS) {
        let Ok(Some(g)) = goal::get_goal(db, gid) else {
            continue;
        };
        let phase = goal::list_phases(db, gid)
            .into_iter()
            .find(|p| p.start_day <= d && d <= p.end_day);
        let ms: Vec<String> = goal::list_milestones(db, gid)
            .into_iter()
            .filter(|m| m.day == d)
            .filter_map(|m| match m.status.as_str() {
                "passed" => Some(format!("节点「{}」达标了", short(&m.title))),
                "missed" => Some(format!("节点「{}」没达标", short(&m.title))),
                _ => None,
            })
            .collect();
        let phase_title = phase.map(|p| short(&p.title));
        if let Some(n) = goal_note(&short(&g.title), phase_title.as_deref(), &ms) {
            notes.push(n);
        }
    }
    Some(DaySummary {
        text: render(&done, &undone, minutes, &notes),
        total,
        done: done.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Plan, Task, save_plan, save_task};
    use super::*;
    use crate::plans::goal::{Goal, Milestone, Phase, save_goal, save_milestone, save_phase};

    const DAY: &str = "2026-11-10";

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn add(db: &Connection, plan: Option<i64>, title: &str, status: &str, spent: u32) {
        save_task(
            db,
            &Task {
                plan_id: plan,
                day: DAY.into(),
                title: title.into(),
                status: status.into(),
                spent,
                ..Default::default()
            },
            0,
        )
        .unwrap();
    }

    #[test]
    fn renders_the_three_shapes() {
        let part = render(
            &names(&["线代第3章", "背单词", "交实验报告"]),
            &names(&["跑步"]),
            110,
            &[],
        );
        assert_eq!(
            part,
            "计划：做完 3/4 项（共 1 小时 50 分钟）——线代第3章、背单词、交实验报告；没做的：跑步。"
        );
        let all = render(&names(&["背单词", "跑步"]), &[], 45, &[]);
        assert_eq!(all, "计划：做完了 2 项（共 45 分钟）——背单词、跑步。");
        let none = render(&[], &names(&["背单词", "跑步"]), 0, &[]);
        assert_eq!(
            none,
            "计划：安排了 2 项，一项都没做；没做的：背单词、跑步。"
        );
        // 做完了但没记时间：不写「共」
        assert!(!render(&names(&["a"]), &[], 0, &[]).contains('共'));
    }

    #[test]
    fn long_lists_and_titles_are_cut() {
        let many: Vec<String> = (1..=8).map(|i| format!("事项{i}")).collect();
        let s = render(&many, &[], 0, &[]);
        assert!(s.contains("事项5 等") && !s.contains("事项6"), "{s}");
        assert_eq!(short("  交实验报告  "), "交实验报告");
        let cut = short("把第三章到第五章的全部课后习题都做完并且整理错题");
        assert_eq!(cut.chars().count(), MAX_TITLE + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn goal_note_shapes() {
        let ms = vec!["节点「模拟面试」达标了".to_string()];
        assert_eq!(
            goal_note("就活准备", Some("基础"), &ms).unwrap(),
            "计划书「就活准备」在「基础」阶段；节点「模拟面试」达标了。"
        );
        assert_eq!(
            goal_note("就活准备", Some("基础"), &[]).unwrap(),
            "计划书「就活准备」在「基础」阶段。"
        );
        assert_eq!(
            goal_note("就活准备", None, &ms).unwrap(),
            "计划书「就活准备」：节点「模拟面试」达标了。"
        );
        assert!(goal_note("就活准备", None, &[]).is_none());
    }

    #[test]
    fn nothing_planned_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        assert!(day_summary(&st.db(), day()).is_none(), "没有安排");
        add(&st.db(), None, "不想做了", "skipped", 0);
        assert!(day_summary(&st.db(), day()).is_none(), "全跳过了也不算");
    }

    #[test]
    fn all_done_and_partly_done() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        let db = st.db();
        add(&db, None, "线代第3章", "done", 60);
        add(&db, None, "背单词", "done", 50);
        add(&db, None, "跳过的不算", "skipped", 0);
        let s = day_summary(&db, day()).unwrap();
        assert_eq!((s.total, s.done), (2, 2));
        assert_eq!(
            s.text,
            "计划：做完了 2 项（共 1 小时 50 分钟）——线代第3章、背单词。"
        );
        add(&db, None, "跑步", "todo", 0);
        let s = day_summary(&db, day()).unwrap();
        assert_eq!((s.total, s.done), (3, 2));
        assert_eq!(
            s.text,
            "计划：做完 2/3 项（共 1 小时 50 分钟）——线代第3章、背单词；没做的：跑步。"
        );
    }

    #[test]
    fn moved_to_later_counts_as_not_done() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        let db = st.db();
        add(&db, None, "背单词", "done", 30);
        // 单独的事没做完，第二天被挪走了：那天算没做
        save_task(
            &db,
            &Task {
                day: "2026-11-11".into(),
                orig_day: DAY.into(),
                title: "交实验报告".into(),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        // 挪过来的、原本是别的日子的事不算在这天
        save_task(
            &db,
            &Task {
                day: DAY.into(),
                orig_day: "2026-11-09".into(),
                title: "前天挪来的".into(),
                status: "done".into(),
                spent: 15,
                ..Default::default()
            },
            0,
        )
        .unwrap();
        let s = day_summary(&db, day()).unwrap();
        assert_eq!((s.total, s.done), (3, 2));
        assert!(s.text.contains("没做的：交实验报告"), "{}", s.text);
        // 第二天自己的汇总里它是没做完的一项
        let next = day_summary(&db, NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()).unwrap();
        assert_eq!((next.total, next.done), (1, 0));
    }

    #[test]
    fn goal_phase_and_milestone_of_that_day() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        let db = st.db();
        let gid = save_goal(
            &db,
            &Goal {
                title: "就活准备".into(),
                start_day: "2026-11-01".into(),
                end_day: "2027-02-28".into(),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        let pid = save_plan(
            &db,
            &Plan {
                title: "面试练习".into(),
                goal_id: Some(gid),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        for (title, start, end) in [
            ("入门", "2026-10-01", "2026-10-31"),
            ("基础", "2026-11-01", "2026-11-30"),
        ] {
            save_phase(
                &db,
                &Phase {
                    goal_id: gid,
                    title: title.into(),
                    start_day: start.into(),
                    end_day: end.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        for (title, d, status) in [
            ("模拟面试", DAY, "passed"),
            ("写简历", DAY, "missed"),
            ("还没验收的", DAY, "todo"),
            ("别的日子的", "2026-11-20", "passed"),
        ] {
            save_milestone(
                &db,
                &Milestone {
                    goal_id: gid,
                    title: title.into(),
                    day: d.into(),
                    status: status.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        add(&db, Some(pid), "模拟面试一次", "done", 90);
        add(&db, Some(pid), "整理自我介绍", "done", 20);
        let s = day_summary(&db, day()).unwrap();
        assert_eq!((s.total, s.done), (2, 2));
        assert!(
            s.text.ends_with(
                "计划书「就活准备」在「基础」阶段；节点「模拟面试」达标了；节点「写简历」没达标。"
            ),
            "{}",
            s.text
        );
        assert!(!s.text.contains("别的日子的") && !s.text.contains("还没验收"));
        // 同一天没有这本计划书的事项时，不提它
        let other = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        save_task(
            &db,
            &Task {
                day: "2026-11-15".into(),
                title: "散步".into(),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        let s = day_summary(&db, other).unwrap();
        assert!(!s.text.contains("计划书"), "{}", s.text);
    }
}
