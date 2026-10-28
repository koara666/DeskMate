//! 活动会话的状态机（纯逻辑，时间由调用方传入，测试用假时钟）。
//! 新的分类要连续出现 20 秒以上才算数（来回切窗口不算）；产生「开始 / 结束 / 很久了 / 深夜还在」等事件。

use super::classify::{Category, Sample};

/// 一段持续的活动
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub category: Category,
    pub label: Option<String>,
    pub start: i64,
    long_said: bool,
    late_said: bool,
    /// 活动日志里对应的那一行（定期更新时长）
    pub row: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Start {
        category: Category,
        label: Option<String>,
    },
    End {
        category: Category,
        label: Option<String>,
        secs: i64,
    },
    /// 同一件事做了很久（每段只说一次）
    Long {
        category: Category,
        label: Option<String>,
        secs: i64,
    },
    /// 深夜还在玩游戏 / 看视频（每段只说一次）
    LateNight { category: Category },
    Switch {
        from: Category,
        from_secs: i64,
        to: Category,
    },
}

/// 多久算「很久」
pub fn long_after(c: Category) -> Option<i64> {
    match c {
        Category::Game | Category::Video | Category::Coding => Some(2 * 3600),
        Category::Writing => Some(90 * 60),
        _ => None,
    }
}

const CONFIRM_SECS: i64 = 20;

#[derive(Debug, Default)]
pub struct Tracker {
    pub cur: Option<Session>,
    pending: Option<(Category, Option<String>, i64)>,
}

/// 深夜：0:30 ~ 5:00（`minute_of_day` 是本地时间）
pub fn is_late(minute_of_day: u32) -> bool {
    (30..300).contains(&minute_of_day)
}

impl Tracker {
    /// 喂一次采样，返回发生的事件
    pub fn observe(&mut self, s: &Sample, now: i64, minute_of_day: u32) -> Vec<Event> {
        let mut ev = Vec::new();
        if s.private {
            // 隐私窗口：结束当前这段，什么都不开始、不记
            if let Some(c) = self.cur.take() {
                ev.push(Event::End {
                    category: c.category,
                    label: c.label,
                    secs: now - c.start,
                });
            }
            self.pending = None;
            return ev;
        }
        let same = self
            .cur
            .as_ref()
            .is_some_and(|c| c.category == s.category && c.label == s.label);
        if same {
            self.pending = None;
            if let Some(c) = self.cur.as_mut() {
                let secs = now - c.start;
                if !c.long_said && long_after(c.category).is_some_and(|t| secs >= t) {
                    c.long_said = true;
                    ev.push(Event::Long {
                        category: c.category,
                        label: c.label.clone(),
                        secs,
                    });
                }
                if !c.late_said
                    && matches!(c.category, Category::Game | Category::Video)
                    && is_late(minute_of_day)
                {
                    c.late_said = true;
                    ev.push(Event::LateNight {
                        category: c.category,
                    });
                }
            }
            return ev;
        }
        match &self.pending {
            Some((cat, label, since))
                if *cat == s.category && *label == s.label && now - since >= CONFIRM_SECS =>
            {
                let since = *since;
                if let Some(old) = self.cur.take() {
                    let secs = (since - old.start).max(0);
                    ev.push(Event::End {
                        category: old.category,
                        label: old.label,
                        secs,
                    });
                    ev.push(Event::Switch {
                        from: old.category,
                        from_secs: secs,
                        to: s.category,
                    });
                }
                ev.push(Event::Start {
                    category: s.category,
                    label: s.label.clone(),
                });
                self.cur = Some(Session {
                    category: s.category,
                    label: s.label.clone(),
                    start: since,
                    long_said: false,
                    // 一开始就是深夜的话，过一会儿（采样时）再说
                    late_said: false,
                    row: None,
                });
                self.pending = None;
            }
            Some((cat, label, _)) if *cat == s.category && *label == s.label => {}
            _ => self.pending = Some((s.category, s.label.clone(), now)),
        }
        ev
    }

    pub fn current(&self) -> Option<&Session> {
        self.cur.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(c: Category, label: &str) -> Sample {
        Sample {
            category: c,
            label: (!label.is_empty()).then(|| label.to_string()),
            exe: "x.exe".into(),
            private: false,
        }
    }

    const NOON: u32 = 12 * 60;

    #[test]
    fn needs_twenty_seconds_to_count() {
        let mut t = Tracker::default();
        assert!(
            t.observe(&s(Category::Coding, "VS Code"), 0, NOON)
                .is_empty()
        );
        assert!(
            t.observe(&s(Category::Coding, "VS Code"), 10, NOON)
                .is_empty()
        );
        let ev = t.observe(&s(Category::Coding, "VS Code"), 30, NOON);
        assert_eq!(
            ev,
            vec![Event::Start {
                category: Category::Coding,
                label: Some("VS Code".into())
            }]
        );
        assert_eq!(t.current().unwrap().start, 0, "从第一次看到算起");
        // 切去聊天 5 秒又回来：不算
        assert!(t.observe(&s(Category::Chat, "微信"), 100, NOON).is_empty());
        assert!(
            t.observe(&s(Category::Coding, "VS Code"), 105, NOON)
                .is_empty()
        );
        assert_eq!(t.current().unwrap().category, Category::Coding);
    }

    #[test]
    fn switch_end_and_long() {
        let mut t = Tracker::default();
        t.observe(&s(Category::Writing, "Word"), 0, NOON);
        t.observe(&s(Category::Writing, "Word"), 30, NOON);
        let ev = t.observe(&s(Category::Writing, "Word"), 90 * 60, NOON);
        assert!(matches!(
            ev[0],
            Event::Long {
                category: Category::Writing,
                ..
            }
        ));
        assert!(
            t.observe(&s(Category::Writing, "Word"), 100 * 60, NOON)
                .is_empty(),
            "只说一次"
        );
        t.observe(&s(Category::Video, "Chrome"), 6000, NOON);
        let ev = t.observe(&s(Category::Video, "Chrome"), 6030, NOON);
        assert_eq!(ev.len(), 3);
        assert_eq!(
            ev[0],
            Event::End {
                category: Category::Writing,
                label: Some("Word".into()),
                secs: 6000
            }
        );
        assert!(matches!(
            ev[1],
            Event::Switch {
                from: Category::Writing,
                to: Category::Video,
                ..
            }
        ));
    }

    #[test]
    fn late_night_once_and_private_ends_session() {
        let mut t = Tracker::default();
        let late = 60; // 1:00
        t.observe(&s(Category::Game, "Hades"), 0, late);
        t.observe(&s(Category::Game, "Hades"), 30, late);
        let ev = t.observe(&s(Category::Game, "Hades"), 60, late);
        assert_eq!(
            ev,
            vec![Event::LateNight {
                category: Category::Game
            }]
        );
        assert!(t.observe(&s(Category::Game, "Hades"), 90, late).is_empty());
        assert!(!is_late(10) && is_late(30) && !is_late(300));
        let mut p = s(Category::Other, "");
        p.private = true;
        let ev = t.observe(&p, 100, late);
        assert!(matches!(ev[0], Event::End { secs: 100, .. }));
        assert!(t.current().is_none());
    }
}
