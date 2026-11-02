//! 语音识别结果里的称呼纠正：把听错的角色名改回来（「红利期」→「红莉栖」）。
//!
//! 识别模型不认识「红莉栖」这类生僻名字，只会写成读音相近的常见字，而且是桌宠里被叫得最多的词，
//! 所以单独处理：按读音（无声调拼音，多音字的每个读音都算）逐字比较，读音一样或者只差
//! 前后鼻音、平翘舌这类常见混淆的才改。名字越短越保守，免得把别的词改坏。

use pinyin::ToPinyinMulti;

/// 一个字的所有读音（无声调拼音）
fn readings(c: char) -> Vec<&'static str> {
    c.to_pinyin_multi()
        .map(|m| m.into_iter().map(|p| p.plain()).collect())
        .unwrap_or_default()
}

fn is_han(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

/// 声母和韵母（zh/ch/sh 算双字母声母）
fn split(py: &str) -> (&str, &str) {
    for ini in ["zh", "ch", "sh"] {
        if let Some(rest) = py.strip_prefix(ini) {
            return (ini, rest);
        }
    }
    let n = py
        .chars()
        .next()
        .filter(|c| !"aoeiuv".contains(*c))
        .map_or(0, |c| c.len_utf8());
    py.split_at(n)
}

/// 声母混淆：平翘舌、n/l
fn initial_alike(a: &str, b: &str) -> bool {
    a == b
        || matches!(
            (a, b),
            ("z", "zh")
                | ("zh", "z")
                | ("c", "ch")
                | ("ch", "c")
                | ("s", "sh")
                | ("sh", "s")
                | ("n", "l")
                | ("l", "n")
        )
}

/// 韵母混淆：前后鼻音
fn final_alike(a: &str, b: &str) -> bool {
    a == b
        || matches!(
            (a, b),
            ("an", "ang")
                | ("ang", "an")
                | ("en", "eng")
                | ("eng", "en")
                | ("in", "ing")
                | ("ing", "in")
                | ("ian", "iang")
                | ("iang", "ian")
                | ("uan", "uang")
                | ("uang", "uan")
        )
}

/// 两个读音只差一类常见混淆（声母或韵母，不能两样都不同）
fn fuzzy(a: &str, b: &str) -> bool {
    let ((ai, af), (bi, bf)) = (split(a), split(b));
    (initial_alike(ai, bi) && af == bf) || (ai == bi && final_alike(af, bf))
}

/// 听到的字 `heard` 和名字里的字 `want` 差多少：0 = 同一个字，0.25 = 同音，1 = 读音相近，None = 不像
fn char_cost(heard: char, want: char, want_readings: &[&'static str]) -> Option<f32> {
    if heard == want {
        return Some(0.0);
    }
    let hr = readings(heard);
    if hr.iter().any(|h| want_readings.contains(h)) {
        return Some(0.25);
    }
    hr.iter()
        .any(|h| want_readings.iter().any(|w| fuzzy(h, w)))
        .then_some(1.0)
}

struct Name {
    chars: Vec<char>,
    readings: Vec<Vec<&'static str>>,
    /// 总差距超过这个就不改：2 个字只收同音，3 个字最多一个读音相近，更长的放宽一点
    budget: f32,
}

/// 角色的名字和叫法，用来纠正识别结果
#[derive(Default)]
pub struct NameFixer {
    names: Vec<Name>,
}

impl NameFixer {
    /// `names`：全名、简称、外号。只收 2~8 个汉字的，长的排前面（先试「牧濑红莉栖」再试「红莉栖」）
    pub fn new<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut seen = std::collections::HashSet::new();
        let mut list: Vec<Name> = names
            .into_iter()
            .filter_map(|n| {
                let chars: Vec<char> = n.as_ref().trim().chars().collect();
                if !(2..=8).contains(&chars.len()) || !chars.iter().all(|c| is_han(*c)) {
                    return None;
                }
                if !seen.insert(chars.clone()) {
                    return None;
                }
                let readings = chars.iter().map(|c| readings(*c)).collect();
                let budget = match chars.len() {
                    2 => 0.5,
                    3 => 1.25,
                    _ => 2.0,
                };
                Some(Name {
                    chars,
                    readings,
                    budget,
                })
            })
            .collect();
        list.sort_by_key(|n| std::cmp::Reverse(n.chars.len()));
        Self { names: list }
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// 把 `text` 里听错的名字改回来；没有要改的就原样返回
    pub fn fix(&self, text: &str) -> String {
        if self.names.is_empty() {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        'outer: while i < chars.len() {
            if is_han(chars[i]) {
                for name in &self.names {
                    let n = name.chars.len();
                    if i + n > chars.len() || !chars[i..i + n].iter().all(|c| is_han(*c)) {
                        continue;
                    }
                    let mut cost = 0.0;
                    let mut ok = true;
                    for k in 0..n {
                        match char_cost(chars[i + k], name.chars[k], &name.readings[k]) {
                            Some(c) => cost += c,
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok && cost <= name.budget {
                        out.extend(name.chars.iter());
                        i += n;
                        continue 'outer;
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kurisu() -> NameFixer {
        NameFixer::new(["牧濑红莉栖", "红莉栖", "牧濑", "克里斯蒂娜"])
    }

    #[test]
    fn mis_heard_names_are_restored() {
        let f = kurisu();
        // 评测里模型真实写出来的几种
        for heard in ["红利期", "红历七", "红利七", "洪莉栖", "红丽西"] {
            assert_eq!(
                f.fix(&format!("你好呀{heard}今天过得怎么样")),
                "你好呀红莉栖今天过得怎么样",
                "{heard}"
            );
        }
        assert_eq!(
            f.fix("红利期，你相信时间旅行吗？"),
            "红莉栖，你相信时间旅行吗？"
        );
    }

    #[test]
    fn correct_text_and_other_words_are_untouched() {
        let f = kurisu();
        for t in [
            "红莉栖你好",
            "今天天气不错，红彤彤的太阳",
            "你相信时间旅行吗",
            "我买的基金今年红利很高",
            "",
            "hello 红莉栖",
        ] {
            assert_eq!(f.fix(t), t);
        }
    }

    #[test]
    fn short_names_only_accept_homophones() {
        let f = NameFixer::new(["千秋", "七海"]);
        // 同音（前秋 / 迁秋）改，读音只是相近的（前后鼻音）不改
        assert_eq!(f.fix("前秋你好"), "千秋你好");
        assert_eq!(f.fix("强秋你好"), "强秋你好");
        assert_eq!(f.fix("七海千秋"), "七海千秋");
        // 单字不当名字
        assert!(NameFixer::new(["秋"]).is_empty());
        // 带英文 / 标点的不收
        assert!(NameFixer::new(["Kurisu", "红a栖", "红 莉栖"]).is_empty());
    }

    #[test]
    fn longer_name_goes_first_and_overlaps_are_consumed_once() {
        let f = kurisu();
        // 全名里的「红莉栖」不会被先改一遍再被「牧濑」重复处理
        assert_eq!(f.fix("牧濑红莉栖"), "牧濑红莉栖");
        assert_eq!(f.fix("木濑红利期是谁"), "牧濑红莉栖是谁");
        // 三个字的名字允许一个字只是读音相近（n / l 混淆），两个就不行
        let f3 = NameFixer::new(["红莉栖"]);
        assert_eq!(f3.fix("红你栖"), "红莉栖");
        assert_eq!(f3.fix("洪你西"), "洪你西");
    }

    #[test]
    fn readings_cover_polyphonic_characters() {
        // 「栖」读 qi，也有读 xi 的说法；两种听起来像的都能认
        assert!(readings('栖').contains(&"qi"));
        let f = NameFixer::new(["红莉栖"]);
        assert_eq!(f.fix("红莉西"), "红莉栖");
        assert_eq!(f.fix("红莉七"), "红莉栖");
    }
}
