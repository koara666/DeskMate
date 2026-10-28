//! 预写台词池（DECISIONS #35）：角色包的 `lines/*.md`，按场景分节。主动说话尽量用它，不调用模型。
//!
//! 格式：
//! ```text
//! ## reminder.fire
//! - {内容}的时间到了哦。
//! - [深夜] 这么晚了……{内容}，别忘了。
//! - [亲密度3,心情好] ……
//! ```
//! - `## 场景名` 开一节；`- ` 开头的一行是一句台词；其余行忽略（可以写说明）。
//! - 行首 `[条件,条件]` 可选，全部满足才会被选中：`亲密度N`（N 级及以上）、`心情好`、`心情差`、
//!   `早上`(5~11 点) `中午`(11~14) `下午`(14~18) `晚上`(18~23) `深夜`(23~5)。不认识的条件 = 这句永远不选（宁可不说也不说错）。
//! - `{占位符}` 由程序填入（每个场景能用哪些见 HOW_TO_ADD）；填不上的台词不会被选中。
//! - 角色没写的场景，用程序内置的通用台词（`characters/_common/lines/`）。
//!
//! 只读：角色包在运行时只读，台词只作为要显示的文字或提示词里的参考。

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

/// 程序用到的场景和每个场景能用的占位符（HOW_TO_ADD 里有同样的表；新增场景时两边一起改）
pub const SCENES: &[(&str, &[&str])] = &[
    ("reminder.confirm", &["内容", "时间"]),
    ("reminder.saved", &["内容", "时间"]),
    ("reminder.fire", &["内容"]),
    ("reminder.missed", &["内容", "时间"]),
    ("reminder.unclear", &[]),
    ("reminder.cancelled", &[]),
    ("briefing.open", &[]),
    ("briefing.bedtime", &[]),
    ("briefing.close", &[]),
    ("briefing.umbrella", &[]),
    ("briefing.snow", &[]),
    ("briefing.cold", &[]),
    ("briefing.hot", &[]),
    ("briefing.temp_swing", &[]),
    ("briefing.nice", &[]),
    ("briefing.holiday", &["节日"]),
    ("briefing.weekend", &[]),
    ("briefing.workday", &[]),
    ("briefing.makeup_workday", &["节日"]),
    ("briefing.upcoming", &["节日", "天数"]),
    ("briefing.leisure", &["活动"]),
    ("briefing.meal", &["饭", "菜"]),
    ("briefing.reminders", &["数量", "提醒"]),
    ("briefing.news", &[]),
    ("briefing.birthday_self", &[]),
    ("briefing.special_user", &["日子"]),
    ("briefing.festival", &["节日"]),
];

const MAX_FILES: usize = 32;
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_LINE_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tag {
    Affinity(u8),
    MoodGood,
    MoodLow,
    /// 一天中的时段：[起始小时, 结束小时)，可以跨午夜
    Hours(u32, u32),
    /// 不认识的条件
    Unknown,
}

fn parse_tag(t: &str) -> Tag {
    let t = t.trim();
    if let Some(n) = t.strip_prefix("亲密度") {
        return n.trim().parse().map(Tag::Affinity).unwrap_or(Tag::Unknown);
    }
    match t {
        "心情好" => Tag::MoodGood,
        "心情差" => Tag::MoodLow,
        "早上" => Tag::Hours(5, 11),
        "中午" => Tag::Hours(11, 14),
        "下午" => Tag::Hours(14, 18),
        "晚上" => Tag::Hours(18, 23),
        "深夜" => Tag::Hours(23, 5),
        _ => Tag::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    tags: Vec<Tag>,
}

/// 一个角色（或通用）的全部台词，按场景分
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    scenes: HashMap<String, Vec<Line>>,
}

/// 挑台词时的情境
#[derive(Debug, Clone, Default)]
pub struct LineCtx<'a> {
    /// 亲密度等级（0 起）
    pub affinity: u8,
    /// 心情：正数开心、负数低落（-100 ~ 100）
    pub mood: i32,
    /// 本地时间的小时（0~23）
    pub hour: u32,
    /// 占位符的值
    pub vars: &'a [(&'a str, String)],
}

impl Tag {
    fn ok(&self, ctx: &LineCtx<'_>) -> bool {
        match self {
            Tag::Affinity(n) => ctx.affinity >= *n,
            Tag::MoodGood => ctx.mood >= 30,
            Tag::MoodLow => ctx.mood <= -30,
            Tag::Hours(a, b) if a < b => (*a..*b).contains(&ctx.hour),
            Tag::Hours(a, b) => ctx.hour >= *a || ctx.hour < *b,
            Tag::Unknown => false,
        }
    }
}

/// 把 `{名字}` 换成值；有填不上的占位符时返回 None
pub fn fill(text: &str, vars: &[(&str, String)]) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let j = after.find('}')?;
        let key = &after[..j];
        let (_, v) = vars.iter().find(|(k, _)| *k == key)?;
        out.push_str(v);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

impl Lines {
    /// 解析一个文件的内容，合并进来（同名场景追加）
    pub fn parse_into(&mut self, src: &str) {
        let mut scene: Option<String> = None;
        for raw in src.lines() {
            let line = raw.trim();
            if let Some(h) = line.strip_prefix("## ") {
                let name = h.trim();
                scene = (!name.is_empty() && name.len() <= 64).then(|| name.to_string());
                continue;
            }
            let (Some(s), Some(item)) = (&scene, line.strip_prefix("- ")) else {
                continue;
            };
            let item = item.trim();
            let (tags, text) = match item.strip_prefix('[').and_then(|r| r.split_once(']')) {
                Some((t, rest)) => (
                    t.split([',', '，', '、'])
                        .filter(|x| !x.trim().is_empty())
                        .map(parse_tag)
                        .collect(),
                    rest.trim(),
                ),
                None => (Vec::new(), item),
            };
            if text.is_empty() {
                continue;
            }
            self.scenes.entry(s.clone()).or_default().push(Line {
                text: text.chars().take(MAX_LINE_CHARS).collect(),
                tags,
            });
        }
    }

    pub fn parse(src: &str) -> Self {
        let mut l = Self::default();
        l.parse_into(src);
        l
    }

    /// 读取一个 `lines/` 文件夹里的全部 `.md`（按文件名排序；不跟随链接，有数量和大小上限）
    pub fn load_dir(dir: &Path) -> Self {
        let mut l = Self::default();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return l;
        };
        let mut files: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
            .collect();
        files.sort();
        for p in files.into_iter().take(MAX_FILES) {
            match crate::character::read_limited(&p, MAX_FILE_BYTES) {
                Ok(raw) => l.parse_into(&String::from_utf8_lossy(&raw)),
                Err(e) => log::warn!("台词文件 {} 无效，已忽略：{e}", p.display()),
            }
        }
        l
    }

    /// 用 `over` 里有的场景整节替换（用户角色包覆盖内置的）
    pub fn overlay(&mut self, over: Lines) {
        for (k, v) in over.scenes {
            self.scenes.insert(k, v);
        }
    }

    pub fn has(&self, scene: &str) -> bool {
        self.scenes.get(scene).is_some_and(|v| !v.is_empty())
    }

    /// 检查台词：不认识的场景、场景里用了不该用的占位符、不认识的条件（体检测试用）
    pub fn problems(&self) -> Vec<String> {
        let mut v = Vec::new();
        for (scene, lines) in &self.scenes {
            let Some((_, allowed)) = SCENES.iter().find(|(s, _)| s == scene) else {
                v.push(format!("不认识的场景 {scene}"));
                continue;
            };
            for l in lines {
                if l.tags.contains(&Tag::Unknown) {
                    v.push(format!("{scene}：不认识的条件「{}」", l.text));
                }
                let mut rest = l.text.as_str();
                while let Some(i) = rest.find('{') {
                    let after = &rest[i + 1..];
                    let Some(j) = after.find('}') else {
                        v.push(format!("{scene}：括号没闭合「{}」", l.text));
                        break;
                    };
                    if !allowed.contains(&&after[..j]) {
                        v.push(format!("{scene}：不能用 {{{}}}「{}」", &after[..j], l.text));
                    }
                    rest = &after[j + 1..];
                }
            }
        }
        v.sort();
        v
    }

    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    pub fn line_count(&self) -> usize {
        self.scenes.values().map(Vec::len).sum()
    }

    /// 这个场景里此刻能用的台词（已填好占位符），保持文件里的顺序
    pub fn eligible(&self, scene: &str, ctx: &LineCtx<'_>) -> Vec<String> {
        self.scenes
            .get(scene)
            .map(|v| {
                v.iter()
                    .filter(|l| l.tags.iter().all(|t| t.ok(ctx)))
                    .filter_map(|l| fill(&l.text, ctx.vars))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// 每个场景最近说过的几句，挑的时候尽量避开（不重复）
#[derive(Default)]
pub struct Recent {
    inner: Mutex<HashMap<String, VecDeque<String>>>,
}

const RECENT_KEEP: usize = 3;

impl Recent {
    /// 从候选里挑一句：避开最近说过的（全都说过时就不管了），用 `seed` 随机
    pub fn choose(&self, scene: &str, candidates: Vec<String>, seed: u64) -> Option<String> {
        if candidates.is_empty() {
            return None;
        }
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let recent = g.entry(scene.to_string()).or_default();
        let fresh: Vec<&String> = candidates.iter().filter(|c| !recent.contains(c)).collect();
        let pool: Vec<&String> = if fresh.is_empty() {
            candidates.iter().collect()
        } else {
            fresh
        };
        let pick = pool[(seed % pool.len() as u64) as usize].clone();
        recent.push_back(pick.clone());
        while recent.len() > RECENT_KEEP.min(candidates.len().saturating_sub(1)) {
            recent.pop_front();
        }
        Some(pick)
    }
}

/// 先在角色自己的台词里找，没有（或此刻都不合适）再用通用台词
pub fn pick(
    own: &Lines,
    common: &Lines,
    scene: &str,
    ctx: &LineCtx<'_>,
    recent: &Recent,
    seed: u64,
) -> Option<String> {
    let mut c = own.eligible(scene, ctx);
    if c.is_empty() {
        c = common.eligible(scene, ctx);
    }
    recent.choose(scene, c, seed)
}

/// 随机种子（纳秒部分）
pub fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs().rotate_left(17))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "说明文字，程序忽略\n\n## greet\n- 你好呀\n- [早上] 早上好，{称呼}\n- [深夜] 这么晚还不睡？\n- [亲密度3] 又见面啦\n- [火星] 不认识的条件\n这一行不是台词\n\n## empty\n";

    fn ctx(hour: u32, vars: &[(&'static str, String)]) -> LineCtx<'static> {
        LineCtx {
            hour,
            vars: Box::leak(vars.to_vec().into_boxed_slice()),
            ..Default::default()
        }
    }

    #[test]
    fn parses_scenes_tags_and_ignores_other_lines() {
        let l = Lines::parse(SRC);
        assert_eq!(l.scene_count(), 1, "空场景不算");
        assert_eq!(l.line_count(), 5);
        assert!(!l.has("empty"));
        // 下午：只有不带条件的那句
        assert_eq!(l.eligible("greet", &ctx(15, &[])), vec!["你好呀"]);
        // 早上 + 有称呼
        assert_eq!(
            l.eligible("greet", &ctx(8, &[("称呼", "小林".into())])),
            vec!["你好呀", "早上好，小林"]
        );
        // 早上但没有称呼：那句填不上，不选
        assert_eq!(l.eligible("greet", &ctx(8, &[])), vec!["你好呀"]);
        // 深夜跨午夜
        assert!(
            l.eligible("greet", &ctx(2, &[]))
                .contains(&"这么晚还不睡？".to_string())
        );
        assert!(
            l.eligible("greet", &ctx(23, &[]))
                .contains(&"这么晚还不睡？".to_string())
        );
        // 亲密度
        let mut c = ctx(15, &[]);
        c.affinity = 3;
        assert!(l.eligible("greet", &c).contains(&"又见面啦".to_string()));
        // 不认识的条件永远不选
        c.affinity = 9;
        assert!(!l.eligible("greet", &c).iter().any(|x| x.contains("不认识")));
    }

    #[test]
    fn fill_placeholders() {
        let v = [
            ("内容", "收衣服".to_string()),
            ("时间", "15:00".to_string()),
        ];
        assert_eq!(
            fill("{时间}记得{内容}", &v).as_deref(),
            Some("15:00记得收衣服")
        );
        assert_eq!(fill("没有占位符", &v).as_deref(), Some("没有占位符"));
        assert_eq!(fill("{城市}下雨", &v), None);
        assert_eq!(fill("坏的 {内容", &v), None);
    }

    #[test]
    fn falls_back_to_common_and_avoids_repeats() {
        let own = Lines::parse("## a\n- 角色的话\n");
        let common = Lines::parse("## a\n- 通用1\n## b\n- 通用2\n- 通用3\n- 通用4\n");
        let r = Recent::default();
        assert_eq!(
            pick(&own, &common, "a", &ctx(12, &[]), &r, 7).as_deref(),
            Some("角色的话")
        );
        assert_eq!(
            pick(&own, &common, "b", &ctx(12, &[]), &r, 0).as_deref(),
            Some("通用2")
        );
        // 同一个种子，第二次会避开刚说过的
        let second = pick(&own, &common, "b", &ctx(12, &[]), &r, 0).unwrap();
        assert_ne!(second, "通用2");
        let third = pick(&own, &common, "b", &ctx(12, &[]), &r, 0).unwrap();
        assert!(third != "通用2" && third != second);
        assert_eq!(pick(&own, &common, "none", &ctx(12, &[]), &r, 0), None);
    }

    #[test]
    fn overlay_replaces_whole_scene() {
        let mut base = Lines::parse("## a\n- 内置1\n- 内置2\n## b\n- 内置b\n");
        base.overlay(Lines::parse("## a\n- 用户的\n"));
        assert_eq!(base.eligible("a", &ctx(12, &[])), vec!["用户的"]);
        assert_eq!(base.eligible("b", &ctx(12, &[])), vec!["内置b"]);
    }

    #[test]
    fn builtin_lines_are_valid_and_common_covers_every_scene() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters");
        let common = Lines::load_dir(&root.join("_common/lines"));
        assert!(common.problems().is_empty(), "{:?}", common.problems());
        for (scene, _) in SCENES {
            assert!(common.has(scene), "通用台词缺少场景 {scene}");
        }
        for e in std::fs::read_dir(&root).unwrap().flatten() {
            let dir = e.path().join("lines");
            if dir.is_dir() {
                let l = Lines::load_dir(&dir);
                assert!(
                    l.problems().is_empty(),
                    "{}：{:?}",
                    dir.display(),
                    l.problems()
                );
            }
        }
    }

    #[test]
    fn mood_tags() {
        let l = Lines::parse("## m\n- [心情好] 开心\n- [心情差] 难过\n- 平常\n");
        let mut c = ctx(12, &[]);
        c.mood = 50;
        assert_eq!(l.eligible("m", &c), vec!["开心", "平常"]);
        c.mood = -50;
        assert_eq!(l.eligible("m", &c), vec!["难过", "平常"]);
        c.mood = 0;
        assert_eq!(l.eligible("m", &c), vec!["平常"]);
    }
}
