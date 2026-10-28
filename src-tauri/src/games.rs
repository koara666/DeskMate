//! 小游戏（v2 P5，DECISIONS #42）的后端部分：关键时刻角色说什么、战绩、对心情和亲密度的影响、每日抽签。
//!
//! 游戏规则和胜负判定都在前端本地实现（`src/games/`），这里不关心具体是什么游戏：
//! 新增游戏不用改这里（台词场景按 `game.<游戏>.<事件>` → `game.<事件>` 找）。

use crate::companion::{self, Interaction};
use crate::state::AppState;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 关键时刻（台词场景的最后一段）
pub const EVENTS: [&str; 10] = [
    "start",
    "round_win",
    "round_lose",
    "round_draw",
    "win",
    "lose",
    "draw",
    "lead",
    "behind",
    "special",
];

pub fn valid_game(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 30
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 用户这一局的结果（从用户的角度：win = 用户赢了）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Win,
    Lose,
    Draw,
    /// 没有胜负（抽签、合作类）
    Done,
}

impl Outcome {
    fn id(self) -> &'static str {
        match self {
            Outcome::Win => "win",
            Outcome::Lose => "lose",
            Outcome::Draw => "draw",
            Outcome::Done => "done",
        }
    }
}

pub fn record(
    db: &Connection,
    game: &str,
    character_id: &str,
    o: Outcome,
    now: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO game_records(game, character_id, outcome, ts) VALUES (?1, ?2, ?3, ?4)",
        params![game, character_id, o.id(), now],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub game: String,
    pub win: i64,
    pub lose: i64,
    pub draw: i64,
    pub played: i64,
}

pub fn stats(db: &Connection, character_id: &str) -> Vec<Stats> {
    let Ok(mut stmt) = db.prepare(
        "SELECT game,
                SUM(outcome = 'win'), SUM(outcome = 'lose'), SUM(outcome = 'draw'), COUNT(*)
         FROM game_records WHERE character_id = ?1 GROUP BY game ORDER BY COUNT(*) DESC",
    ) else {
        return Vec::new();
    };
    stmt.query_map([character_id], |r| {
        Ok(Stats {
            game: r.get(0)?,
            win: r.get(1)?,
            lose: r.get(2)?,
            draw: r.get(3)?,
            played: r.get(4)?,
        })
    })
    .map(|it| it.flatten().collect())
    .unwrap_or_default()
}

/// 某天玩过的游戏（日记用）：（游戏，用户赢，用户输，平）
pub fn day_summary(db: &Connection, start: i64, end: i64) -> Vec<(String, i64, i64, i64)> {
    let Ok(mut stmt) = db.prepare(
        "SELECT game, SUM(outcome = 'win'), SUM(outcome = 'lose'), SUM(outcome = 'draw')
         FROM game_records WHERE ts >= ?1 AND ts < ?2 GROUP BY game",
    ) else {
        return Vec::new();
    };
    stmt.query_map([start, end], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })
    .map(|it| it.flatten().collect())
    .unwrap_or_default()
}

/// 关键时刻说一句：先找这个游戏专门的台词，再找通用的
pub fn line(st: &AppState, game: &str, event: &str, vars: &[(&str, String)]) -> Option<String> {
    st.line(&format!("game.{game}.{event}"), vars)
        .or_else(|| st.line(&format!("game.{event}"), vars))
}

/// 一局结束：记战绩，影响心情（角色的视角：用户赢了 = 她输了）和亲密度
pub fn finish(app: &AppHandle, game: &str, o: Outcome) {
    let st = app.state::<AppState>();
    let Some(c) = st.current_character() else {
        return;
    };
    if let Err(e) = record(&st.db(), game, &c.manifest.id, o, crate::memory::now_secs()) {
        log::warn!("记录战绩失败：{e}");
    }
    let i = match o {
        Outcome::Win => Interaction::GameLose,
        Outcome::Lose => Interaction::GameWin,
        Outcome::Draw | Outcome::Done => Interaction::GameDraw,
    };
    companion::interact(app, i);
    crate::chat::emit_pet_state(
        app,
        match i {
            Interaction::GameWin => crate::chat::PetState::Happy,
            Interaction::GameLose if c.manifest.game_style.competitive => {
                crate::chat::PetState::Sad
            }
            _ => crate::chat::PetState::Happy,
        },
    );
}

// ---------------- 每日抽签 ----------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fortune {
    pub day: String,
    /// great / good / small / fair / bad
    pub level: String,
    pub name: String,
    pub color: String,
    pub item: String,
    pub direction: String,
    /// 角色的解读
    pub comment: String,
}

const LEVELS: [(&str, &str, u64); 5] = [
    ("great", "大吉", 15),
    ("good", "中吉", 25),
    ("small", "小吉", 30),
    ("fair", "末吉", 20),
    ("bad", "凶", 10),
];
const COLORS: [&str; 10] = [
    "樱花粉",
    "天空蓝",
    "薄荷绿",
    "柠檬黄",
    "珊瑚橙",
    "雾霾紫",
    "奶白色",
    "深海蓝",
    "抹茶色",
    "暖灰色",
];
const ITEMS: [&str; 12] = [
    "热茶",
    "耳机",
    "一本旧书",
    "手账",
    "游戏手柄",
    "围巾",
    "便签纸",
    "零食",
    "雨伞",
    "小镜子",
    "钢笔",
    "毛绒挂件",
];
const DIRECTIONS: [&str; 8] = ["东", "南", "西", "北", "东南", "东北", "西南", "西北"];

/// 按「日期 + 一个随机数」抽（纯函数）
pub fn draw(day: &str, seed: u64) -> (usize, usize, usize, usize) {
    let mut h = seed
        ^ day.bytes().fold(1469598103934665603u64, |a, b| {
            (a ^ b as u64).wrapping_mul(1099511628211)
        });
    let mut next = |n: u64| {
        h = h
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (h >> 33) % n
    };
    let total: u64 = LEVELS.iter().map(|l| l.2).sum();
    let mut r = next(total);
    let mut level = 0;
    for (i, l) in LEVELS.iter().enumerate() {
        if r < l.2 {
            level = i;
            break;
        }
        r -= l.2;
    }
    (
        level,
        next(COLORS.len() as u64) as usize,
        next(ITEMS.len() as u64) as usize,
        next(DIRECTIONS.len() as u64) as usize,
    )
}

const KEY_FORTUNE: &str = "fortune_today";

/// 今天已经抽过的签
pub fn fortune_today(st: &AppState) -> Option<Fortune> {
    let day = crate::briefing::briefing_day(crate::reminders::now_local()).to_string();
    crate::settings::kv_get(&st.db(), KEY_FORTUNE)
        .and_then(|s| serde_json::from_str::<Fortune>(&s).ok())
        .filter(|f| f.day == day)
}

/// 抽签（每天一次；再抽返回今天那支）
pub fn fortune_draw(app: &AppHandle) -> Fortune {
    let st = app.state::<AppState>();
    if let Some(f) = fortune_today(&st) {
        return f;
    }
    let day = crate::briefing::briefing_day(crate::reminders::now_local()).to_string();
    let (l, c, i, d) = draw(&day, crate::lines::seed());
    let (lid, name, _) = LEVELS[l];
    let vars = [
        ("签", name.to_string()),
        ("颜色", COLORS[c].to_string()),
        ("物品", ITEMS[i].to_string()),
        ("方位", DIRECTIONS[d].to_string()),
    ];
    let comment = line(&st, "fortune", lid, &vars).unwrap_or_else(|| format!("今天是{name}。"));
    let f = Fortune {
        day,
        level: lid.into(),
        name: name.into(),
        color: COLORS[c].into(),
        item: ITEMS[i].into(),
        direction: DIRECTIONS[d].into(),
        comment,
    };
    if let Ok(s) = serde_json::to_string(&f) {
        let _ = crate::settings::kv_set(&st.db(), KEY_FORTUNE, &s);
    }
    finish(app, "fortune", Outcome::Done);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_stats() {
        let db = crate::db::open_in_memory().unwrap();
        for o in [Outcome::Win, Outcome::Win, Outcome::Lose, Outcome::Draw] {
            record(&db, "rps", "nanami-chiaki", o, 100).unwrap();
        }
        record(&db, "ttt", "nanami-chiaki", Outcome::Lose, 200).unwrap();
        record(&db, "rps", "nailong", Outcome::Win, 300).unwrap();
        let s = stats(&db, "nanami-chiaki");
        assert_eq!(
            s[0],
            Stats {
                game: "rps".into(),
                win: 2,
                lose: 1,
                draw: 1,
                played: 4
            }
        );
        assert_eq!(s.len(), 2);
        assert_eq!(day_summary(&db, 0, 250).len(), 2);
    }

    #[test]
    fn game_ids() {
        assert!(valid_game("rps") && valid_game("blackjack") && valid_game("idiom_chain"));
        assert!(!valid_game("") && !valid_game("../x") && !valid_game("RPS"));
    }

    #[test]
    fn fortune_distribution_and_stability() {
        assert_eq!(draw("2026-10-28", 42), draw("2026-10-28", 42));
        let mut counts = [0; 5];
        for seed in 0..2000u64 {
            counts[draw("2026-10-28", seed).0] += 1;
        }
        // 大吉约 15%、凶约 10%，每一种都会出现
        assert!(counts.iter().all(|&c| c > 100), "{counts:?}");
        assert!(counts[2] > counts[4]);
    }

    #[tokio::test]
    async fn line_falls_back_to_generic_scene() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        st.update_settings(|s| s.character_id = "nanami-chiaki".into());
        assert!(line(&st, "rps", "win", &[]).is_some());
        assert!(
            line(&st, "some_new_game", "start", &[]).is_some(),
            "新游戏没写台词也有通用的"
        );
        let v = [
            ("签", "大吉".to_string()),
            ("颜色", "樱花粉".to_string()),
            ("物品", "热茶".to_string()),
            ("方位", "东".to_string()),
        ];
        assert!(line(&st, "fortune", "great", &v).unwrap().len() > 3);
    }
}
