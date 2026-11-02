//! 说话时的表情：按这句话的内容挑一张合适的图。
//!
//! 以前只要在出声，桌宠就一直是「说话」那一张（嘴一张一合），她说「好困」是这样，说「晕了」也是这样，很违和。
//! 现在后端看一眼要念的这句中文（气泡里显示的那句），挑出最贴切的几张图（按顺序试，角色有哪张用哪张）；
//! 没挑出来就保持原来的「说话」图。只是规则匹配，不调用模型、不联网。
//! 角色没有对应的图就自动退回「说话」图，所以千秋和红莉栖共用同一套规则。

/// 一条规则：这些词出现（且前面不是否定）→ 按顺序试这几张图
struct Rule {
    poses: &'static [&'static str],
    keys: &'static [&'static str],
}

/// 越靠前越优先
const RULES: &[Rule] = &[
    Rule {
        poses: &["dizzy"],
        keys: &[
            "晕了",
            "头晕",
            "好晕",
            "要晕",
            "眼花",
            "天旋地转",
            "转圈",
            "晕乎",
        ],
    },
    Rule {
        poses: &["scared", "surprised"],
        keys: &[
            "糟了",
            "完了",
            "不妙",
            "害怕",
            "吓死",
            "好可怕",
            "恐怖",
            "慌了",
            "危险",
        ],
    },
    Rule {
        poses: &["angry", "pout"],
        keys: &[
            "生气", "气死", "讨厌", "烦死", "吵死", "不许", "够了", "哼", "混蛋", "变态", "别戳",
            "别晃", "别乱",
        ],
    },
    Rule {
        poses: &["idle_night", "sleep"],
        keys: &[
            "好困",
            "很困",
            "困了",
            "犯困",
            "困得",
            "想睡",
            "瞌睡",
            "哈欠",
            "打盹",
            "睡着",
            "没睡醒",
            "睡眼",
            "眼皮",
        ],
    },
    Rule {
        poses: &["sad", "idle_low"],
        keys: &[
            "难过",
            "伤心",
            "寂寞",
            "孤单",
            "失落",
            "想哭",
            "哭了",
            "唉",
            "沮丧",
            "没精神",
        ],
    },
    Rule {
        poses: &["surprised"],
        keys: &[
            "吓",
            "惊",
            "咦",
            "诶",
            "欸",
            "哇",
            "突然",
            "竟然",
            "居然",
            "没想到",
            "真的假的",
        ],
    },
    Rule {
        poses: &["lose"],
        keys: &["输了", "你赢了", "败给", "认输", "被打败", "被你读到"],
    },
    Rule {
        poses: &["win"],
        keys: &["我赢了", "这局是我的", "这手是我的", "胜利", "大吉"],
    },
    Rule {
        poses: &["wink"],
        keys: &["秘密", "悄悄", "偷偷", "嘘", "不要告诉", "不许告诉"],
    },
    Rule {
        poses: &["hood"],
        keys: &["躲起来", "退到后台", "退到托盘"],
    },
    Rule {
        poses: &["shy", "pat"],
        keys: &[
            "害羞",
            "脸红",
            "不好意思",
            "脸都",
            "羞",
            "才不是",
            "别误会",
            "不是在关心",
            "不是关心",
        ],
    },
    Rule {
        poses: &["confused"],
        keys: &[
            "不明白",
            "不懂",
            "搞不懂",
            "什么意思",
            "奇怪",
            "不对劲",
            "困惑",
        ],
    },
    Rule {
        poses: &["thinking"],
        keys: &[
            "我想想",
            "让我想",
            "想一想",
            "想想看",
            "考虑一下",
            "让我整理",
            "我整理一下",
            "稍等",
        ],
    },
    Rule {
        poses: &["happy", "idle_good"],
        keys: &[
            "开心",
            "高兴",
            "太好了",
            "好耶",
            "嘿嘿",
            "耶",
            "太棒",
            "满意",
            "谢谢",
            "不错",
        ],
    },
];

/// 前面是这些字就是否定（「不困」「别生气」「没吓到」），这次出现不算
const NEGATIONS: [char; 4] = ['不', '没', '别', '未'];

/// 这句话该用的表情候选（按顺序试）；空 = 保持「说话」图
pub fn poses_for_text(text: &str) -> &'static [&'static str] {
    let chars: Vec<char> = text.chars().collect();
    // 结巴（「戳、戳太多次了」「才、才不是」）= 慌乱或害羞：「别、别」「才、才」这类放在最前面单独判断
    for w in chars.windows(3) {
        if w[1] == '、' && w[0] == w[2] && w[0].is_alphanumeric() && !w[0].is_ascii() {
            // 带「别戳」之类的生气词时仍然按生气走
            if !contains_unnegated(&chars, &["生气", "别戳", "别晃", "够了", "哼"]) {
                return &["shy", "pat"];
            }
            break;
        }
    }
    for rule in RULES {
        if contains_unnegated(&chars, rule.keys) {
            return rule.poses;
        }
    }
    &[]
}

fn contains_unnegated(chars: &[char], keys: &[&str]) -> bool {
    for key in keys {
        let k: Vec<char> = key.chars().collect();
        if k.is_empty() || chars.len() < k.len() {
            continue;
        }
        for i in 0..=chars.len() - k.len() {
            if chars[i..i + k.len()] != k[..] {
                continue;
            }
            let negated = i > 0 && NEGATIONS.contains(&chars[i - 1]);
            // 「不许」「不错」「不好意思」本身就以否定字开头，这里 i 指向的是关键词开头，所以不受影响
            if !negated {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(t: &str) -> Option<&'static str> {
        poses_for_text(t).first().copied()
    }

    #[test]
    fn sleepy_and_dizzy_lines_get_their_own_faces() {
        // 用户的例子：说「困了」应该是困的表情，说「晕了」应该是晕的表情
        assert_eq!(
            first("……嗯？我睡着了吗……刚才梦到在打 Boss。"),
            Some("idle_night")
        );
        assert_eq!(
            first("唔……有点困了……再摸的话我会睡着的。"),
            Some("idle_night")
        );
        assert_eq!(first("头好晕……是不是触发了混乱状态……"), Some("dizzy"));
        assert_eq!(first("呜……画面在转……我要晕了……"), Some("dizzy"));
    }

    #[test]
    fn mood_words_pick_matching_faces() {
        assert_eq!(
            first("你赢了。很强哦。……不过下一局我不会让的。"),
            Some("lose")
        );
        assert_eq!(first("我赢了。……你刚才那步其实不差。"), Some("win"));
        assert_eq!(first("真是的，我生气了！"), Some("angry"));
        assert_eq!(first("太好了，今天也能一起玩。"), Some("happy"));
        assert_eq!(first("……难过的时候，说出来会好一点。"), Some("sad"));
        assert_eq!(
            first("好，我先躲起来。想找我的话点托盘图标。"),
            Some("hood")
        );
        assert_eq!(first("不、不是在关心你！"), Some("shy"));
    }

    #[test]
    fn stutter_means_shy_unless_angry() {
        assert_eq!(first("才、才不是呢。"), Some("shy"));
        assert_eq!(first("戳、戳太多次了……判定会出问题的。"), Some("shy"));
        assert_eq!(first("戳、戳太多次了！再戳我就要生气了！"), Some("angry"));
    }

    #[test]
    fn negation_and_lookalike_words_are_ignored() {
        assert!(poses_for_text("我现在一点也不困。").is_empty());
        assert!(poses_for_text("这个问题有点困难，别晕。").is_empty());
        assert!(poses_for_text("今天的天气很好，出门记得带伞。").is_empty());
        assert!(poses_for_text("").is_empty());
    }

    #[test]
    fn every_pose_name_is_a_real_image_name() {
        // 候选名必须是角色包里真的会有的图名（状态、反应、待机变体），写错了就永远用不上
        let known = |p: &str| {
            crate::character::STATES.contains(&p)
                || crate::character::POSES.contains(&p)
                || crate::character::IDLE_VARIANTS.contains(&p)
        };
        for rule in RULES {
            for p in rule.poses {
                assert!(known(p), "不认识的图名 {p}");
            }
        }
    }
}
