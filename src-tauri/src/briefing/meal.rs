//! 吃饭建议：按时段、天气、季节挑两三样菜式。**只说菜式和类型，不编造店名和地址**（DECISIONS #39）。
//! 用户的口味（记忆里「喜欢吃 / 不吃……」）交给模型参考；没有模型时只用这里的列表。

use chrono::{Datelike, NaiveDateTime, Timelike};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meal {
    Breakfast,
    Lunch,
    Tea,
    Dinner,
    Snack,
}

impl Meal {
    pub fn at(t: NaiveDateTime) -> Meal {
        match t.hour() {
            5..=9 => Meal::Breakfast,
            10..=13 => Meal::Lunch,
            14..=16 => Meal::Tea,
            17..=20 => Meal::Dinner,
            _ => Meal::Snack,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Meal::Breakfast => "早饭",
            Meal::Lunch => "午饭",
            Meal::Tea => "下午茶",
            Meal::Dinner => "晚饭",
            Meal::Snack => "夜宵",
        }
    }
}

/// 天气对吃什么的影响
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feel {
    Cold,
    Hot,
    Rainy,
    Normal,
}

fn pool(meal: Meal, feel: Feel, japan: bool) -> &'static [&'static str] {
    use Feel::*;
    use Meal::*;
    if japan {
        return match (meal, feel) {
            (Breakfast, _) => &["饭团", "味噌汤配米饭", "吐司", "玉子烧"],
            (Lunch | Dinner, Cold | Rainy) => &["拉面", "关东煮", "寿喜烧", "乌冬面", "咖喱饭"],
            (Lunch | Dinner, Hot) => &["冷荞麦面", "寿司", "冷乌冬", "海鲜盖饭"],
            (Lunch | Dinner, Normal) => &["定食", "炸猪排饭", "咖喱饭", "回转寿司", "亲子丼"],
            (Tea, _) => &["铜锣烧", "抹茶甜点", "可丽饼"],
            (Snack, _) => &["便利店的关东煮", "烤串", "茶泡饭"],
        };
    }
    match (meal, feel) {
        (Breakfast, Cold | Rainy) => &["热豆浆配包子", "小米粥", "热干面", "馄饨"],
        (Breakfast, _) => &["包子豆浆", "煎饼果子", "鸡蛋饼", "粥配小菜", "牛奶燕麦"],
        (Lunch | Dinner, Cold) => &["火锅", "羊肉汤", "砂锅粥", "麻辣烫", "炖菜"],
        (Lunch | Dinner, Rainy) => &["一碗热汤面", "麻辣烫", "砂锅", "炖菜配米饭"],
        (Lunch | Dinner, Hot) => &["凉面", "凉皮", "清淡的粥", "冷面", "沙拉", "绿豆汤配小菜"],
        (Lunch | Dinner, Normal) => &[
            "盖浇饭",
            "牛肉面",
            "家常小炒",
            "饺子",
            "黄焖鸡",
            "米线",
            "烤肉",
        ],
        (Tea, Hot) => &["冰奶茶", "水果", "冰淇淋"],
        (Tea, _) => &["奶茶配小蛋糕", "一杯热可可", "水果"],
        (Snack, Hot) => &["烧烤配冰饮", "水果"],
        (Snack, _) => &["一碗热汤面", "煮个泡面加蛋", "烤红薯"],
    }
}

/// 季节性的一样（锦上添花，没有就不加）
fn seasonal(t: NaiveDateTime, japan: bool) -> Option<&'static str> {
    if japan {
        return match t.month() {
            12..=12 | 1..=2 => Some("关东煮"),
            6..=8 => Some("冷荞麦面"),
            9..=11 => Some("秋刀鱼定食"),
            _ => None,
        };
    }
    match t.month() {
        12 | 1 | 2 => Some("羊肉火锅"),
        6..=8 => Some("西瓜"),
        9..=11 => Some("糖炒栗子"),
        _ => None,
    }
}

/// 挑两三样（`seed` 随机）
pub fn suggest(t: NaiveDateTime, feel: Feel, japan: bool, seed: u64) -> (Meal, Vec<&'static str>) {
    let meal = Meal::at(t);
    let p = pool(meal, feel, japan);
    let mut picks: Vec<&'static str> = Vec::new();
    let mut s = seed;
    while picks.len() < 2.min(p.len()) {
        let x = p[(s % p.len() as u64) as usize];
        if !picks.contains(&x) {
            picks.push(x);
        }
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
            >> 1;
    }
    if matches!(meal, Meal::Lunch | Meal::Dinner)
        && let Some(x) = seasonal(t, japan)
        && !picks.contains(&x)
        && seed % 3 == 0
    {
        picks.push(x);
    }
    (meal, picks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(m: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, m, 10)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    #[test]
    fn picks_by_time_and_weather() {
        let (meal, v) = suggest(at(1, 12), Feel::Cold, false, 7);
        assert_eq!(meal, Meal::Lunch);
        assert!(v.len() >= 2);
        assert!(
            v.iter()
                .all(|x| pool(Meal::Lunch, Feel::Cold, false).contains(x) || *x == "羊肉火锅")
        );
        let (meal, v) = suggest(at(7, 8), Feel::Hot, false, 3);
        assert_eq!(meal, Meal::Breakfast);
        assert_eq!(v.len(), 2);
        assert_ne!(v[0], v[1]);
        let (_, v) = suggest(at(10, 19), Feel::Normal, true, 1);
        assert!(
            v.iter().all(|x| !x.contains("煎饼")),
            "在日本推荐日本的吃的"
        );
        assert_eq!(Meal::at(at(1, 23)), Meal::Snack);
    }
}
