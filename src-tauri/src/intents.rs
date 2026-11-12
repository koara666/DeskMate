//! 聊天里说一句就能做的事（v2.1 P11，DECISIONS #49）：「来一局井字棋」「别打扰我」「你先躲一下」「看看我的屏幕」
//! 「我有哪些提醒」「取消买菜的提醒」……
//!
//! 本地规则优先（`detect`，纯函数有测试；离线、mock 时也能用，不花钱）；本地认不出的，模型可以在回复里写
//! `[[玩：游戏名]]` / `[[提醒：一句吩咐]]` 动作标记（`markers.rs`）兜底。
//! 这些都是用户自己要求的、可以撤回的小事（打开游戏、开关勿扰、藏起来），所以直接做；删提醒只删内容对得上的那一个。

use crate::backends::FinishReason;
use crate::chat::{self, ChatEvent, PetState};
use crate::state::AppState;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

/// 游戏的说法 → 游戏 id（和前端 `src/games/registry.ts` 一致）
pub const GAME_ALIASES: &[(&str, &str)] = &[
    ("井字棋", "ttt"),
    ("三子棋", "ttt"),
    ("圈圈叉叉", "ttt"),
    ("圈叉棋", "ttt"),
    ("石头剪刀布", "rps"),
    ("剪刀石头布", "rps"),
    ("猜拳", "rps"),
    ("二十一点", "blackjack"),
    ("21点", "blackjack"),
    ("黑杰克", "blackjack"),
    ("抽签", "fortune"),
    ("抽个签", "fortune"),
    ("抽一签", "fortune"),
    ("抽一支签", "fortune"),
    ("求签", "fortune"),
    ("运势", "fortune"),
    ("成语接龙", "idiom"),
    ("接龙", "idiom"),
    ("海龟汤", "turtle"),
    ("二十问", "twenty"),
    ("20问", "twenty"),
    ("方块对战", "blocks"),
    ("俄罗斯方块", "blocks"),
    ("消方块", "blocks"),
    ("方块", "blocks"),
];

/// 游戏 id → 名字（台词里用）
pub fn game_name(id: &str) -> &'static str {
    match id {
        "ttt" => "井字棋",
        "rps" => "猜拳",
        "blackjack" => "21 点",
        "fortune" => "今日运势",
        "idiom" => "成语接龙",
        "turtle" => "海龟汤",
        "twenty" => "二十问",
        "blocks" => "方块对战",
        _ => "小游戏",
    }
}

/// 游戏名（用户或模型说的）→ id
pub fn game_id(name: &str) -> Option<&'static str> {
    let n: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    GAME_ALIASES
        .iter()
        .find(|(k, _)| n.contains(k))
        .map(|(_, id)| *id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// 打开某个游戏（None = 打开游戏列表）
    Game(Option<&'static str>),
    Dnd(bool),
    Hide,
    Look,
    /// 打开设置（某一页）
    Settings(Option<&'static str>),
    ListReminders,
    /// 取消内容里有这些字的提醒（空 = 刚才那个）
    CancelReminder(String),
    /// 今天的计划还有什么（`explicit`：说了「计划」两个字；没说的话今天没有安排就交给普通聊天）
    PlansToday {
        explicit: bool,
    },
    /// 打开计划窗口
    OpenPlans,
    /// 想做个新计划（原话，打开计划窗口时填进去）
    NewPlan(String),
}

/// 「帮我做个计划」这类说法
const NEW_PLAN_WORDS: &[&str] = &[
    "做个计划",
    "做一个计划",
    "定个计划",
    "定一个计划",
    "制定计划",
    "制定一个计划",
    "订个计划",
    "排个计划",
    "安排一个计划",
    "帮我规划",
    "帮我做计划",
    "做个学习计划",
    "做一个学习计划",
];

fn has_any(t: &str, ks: &[&str]) -> bool {
    ks.iter().any(|k| t.contains(k))
}

/// 「做个计划」前面可以是什么：没有、称呼加逗号（「千秋，做个计划」），或者以请求的说法结尾（帮我 / 我想 / 能不能……）
const NEW_PLAN_ASK: &[&str] = &[
    "帮我",
    "给我",
    "替我",
    "帮忙",
    "我想",
    "想",
    "我要",
    "想要",
    "请",
    "来",
    "一起",
    "咱们",
    "我们",
    "能不能",
    "可以",
    "可不可以",
];

/// 是在请她做个新计划吗（`t` 已去掉空白、转小写）。
/// 「昨天我做个计划结果没完成」「公司要做一个计划书」这种是在聊天；「帮我规划一下去京都的路线」是问路线，都交给普通聊天。
fn asks_new_plan(t: &str) -> bool {
    if has_any(t, &["不用", "不想", "不要"]) {
        return false;
    }
    if t.contains("帮我规划") && has_any(t, &["路线", "行程", "怎么走", "导航", "旅游", "旅行"])
    {
        return false;
    }
    NEW_PLAN_WORDS.iter().any(|w| {
        t.match_indices(w).any(|(i, _)| {
            let before = &t[..i];
            if before.chars().count() > 8 {
                return false;
            }
            let vocative = before.ends_with(['，', ',', '、', '！', '!', '~', '～']);
            before.is_empty() || vocative || NEW_PLAN_ASK.iter().any(|k| before.ends_with(k))
        })
    })
}

/// 认一认这句话是不是要她做点什么（纯函数）。拿不准就返回 None，交给普通聊天。
pub fn detect(text: &str) -> Option<Intent> {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let n = t.chars().count();
    // 「帮我做个计划：……」后面可能写得很长，在长度限制之前认
    if asks_new_plan(&t) {
        return Some(Intent::NewPlan(text.trim().chars().take(500).collect()));
    }
    if n == 0 || n > 40 {
        // 长句多半是在聊天，不是吩咐
        return None;
    }
    // —— 计划 ——
    if has_any(
        &t,
        &[
            "打开计划",
            "计划窗口",
            "看看我的计划",
            "看看计划",
            "打开我的计划",
        ],
    ) {
        return Some(Intent::OpenPlans);
    }
    let asking = n <= 10
        || has_any(
            &t,
            &[
                "看看",
                "哪些",
                "什么",
                "吗",
                "呢",
                "?",
                "？",
                "说说",
                "告诉我",
                "多少",
            ],
        );
    if asking
        && has_any(
            &t,
            &[
                "今天的计划",
                "今天计划",
                "今日计划",
                "我的计划",
                "今天的安排",
            ],
        )
    {
        return Some(Intent::PlansToday { explicit: true });
    }
    if has_any(
        &t,
        &[
            "今天要做什么",
            "今天还要做什么",
            "今天要干什么",
            "今天还要干什么",
            "今天有什么任务",
            "今天还有什么任务",
            "今天的任务",
            "今天还有什么要做",
            "还有什么没做",
            "还剩什么没做",
            "还有哪些没做",
        ],
    ) {
        return Some(Intent::PlansToday { explicit: false });
    }
    // 否定、回忆、问能力的，不算
    let negative = has_any(
        &t,
        &[
            "不想",
            "不要",
            "不玩",
            "别玩",
            "没玩",
            "玩过",
            "玩了",
            "不用打开",
            "不用了吗",
        ],
    );

    // —— 提醒列表 / 取消 ——
    if has_any(
        &t,
        &[
            "哪些提醒",
            "什么提醒",
            "几个提醒",
            "提醒列表",
            "看看提醒",
            "有提醒吗",
            "我的提醒",
        ],
    ) {
        return Some(Intent::ListReminders);
    }
    if t.contains("提醒") && has_any(&t, &["取消", "删掉", "删除", "不用提醒", "别提醒"])
    {
        let mut k = t.clone();
        for w in [
            "帮我",
            "把",
            "取消",
            "删掉",
            "删除",
            "不用提醒我",
            "不用提醒",
            "别提醒我",
            "别提醒",
            "那个",
            "提醒",
            "刚才",
            "刚刚",
            "的",
            "吧",
            "了",
            "掉",
            "一下",
            "。",
            "，",
            "！",
            "～",
        ] {
            k = k.replace(w, "");
        }
        return Some(Intent::CancelReminder(k.chars().take(20).collect()));
    }

    // —— 玩游戏 ——
    let play_words = [
        "玩",
        "来一局",
        "来一把",
        "来一盘",
        "来局",
        "来把",
        "来盘",
        "下一盘",
        "下盘",
        "下棋",
        "陪我",
        "开一局",
        "走起",
        "抽个签",
        "抽一签",
    ];
    if !negative && let Some(id) = game_id(&t) {
        let bare = n <= 6; // 只说了游戏名（「井字棋！」「猜拳吧」）
        if bare || has_any(&t, &play_words) || (id == "fortune" && has_any(&t, &["抽", "求", "算"]))
        {
            return Some(Intent::Game(Some(id)));
        }
    }
    if !negative
        && has_any(
            &t,
            &[
                "玩游戏",
                "玩个游戏",
                "玩会游戏",
                "玩点什么",
                "一起玩",
                "陪我玩",
                "玩小游戏",
                "来玩吧",
                "玩一会",
            ],
        )
    {
        return Some(Intent::Game(None));
    }

    // —— 勿扰 ——
    if has_any(
        &t,
        &[
            "取消勿扰",
            "关掉勿扰",
            "关闭勿扰",
            "可以说话了",
            "可以说话啦",
            "不用安静了",
            "勿扰关",
        ],
    ) {
        return Some(Intent::Dnd(false));
    }
    if has_any(
        &t,
        &[
            "别打扰我",
            "不要打扰我",
            "勿扰",
            "安静一会",
            "安静一下",
            "我要专心",
            "先别说话",
            "别吵",
            "让我专心",
            "别说话了",
        ],
    ) {
        return Some(Intent::Dnd(true));
    }

    // —— 藏起来 ——
    if has_any(
        &t,
        &[
            "躲一下",
            "躲起来",
            "藏起来",
            "藏一下",
            "隐藏一下",
            "隐藏起来",
            "先下去吧",
            "你先隐藏",
        ],
    ) {
        return Some(Intent::Hide);
    }

    // —— 看屏幕 ——
    if has_any(
        &t,
        &[
            "看看我的屏幕",
            "看一眼屏幕",
            "看看屏幕",
            "看一下屏幕",
            "看我的屏幕",
            "看我屏幕",
            "看看我在干嘛",
            "看看我在干什么",
            "看看这个画面",
            "看看这局",
            "看我玩",
        ],
    ) {
        return Some(Intent::Look);
    }

    // —— 设置、日记 ——
    if has_any(&t, &["打开设置", "打开你的设置", "去设置"]) {
        return Some(Intent::Settings(None));
    }
    if has_any(&t, &["你的日记", "看看日记", "看日记", "日记给我看"]) {
        return Some(Intent::Settings(Some("diary")));
    }
    None
}

/// 说一句（先找台词池，没有就用 `fallback`）
fn line(st: &AppState, scene: &str, vars: &[(&str, String)], fallback: String) -> String {
    st.line(scene, vars).unwrap_or(fallback)
}

/// 处理聊天里的一句吩咐。返回 false = 不是吩咐，交给普通聊天。
pub async fn handle(
    app: &AppHandle,
    text: &str,
    _request_id: &str,
    channel: &Channel<ChatEvent>,
) -> bool {
    let Some(intent) = detect(text) else {
        return false;
    };
    let st = app.state::<AppState>();
    let mut open_game: Option<String> = None;
    let mut after: Option<Box<dyn FnOnce(AppHandle) + Send>> = None;
    let say = match intent {
        Intent::Game(id) => {
            open_game = Some(id.unwrap_or("").to_string());
            match id {
                Some(id) => line(
                    &st,
                    "intent.game",
                    &[("游戏", game_name(id).into())],
                    format!("好呀，来玩{}！", game_name(id)),
                ),
                None => line(&st, "intent.game_list", &[], "好呀，想玩什么？".into()),
            }
        }
        Intent::Dnd(on) => {
            st.update_settings(|s| s.dnd = on);
            crate::desktop::broadcast_settings_changed(app);
            if on {
                crate::companion::emit_pose(app, "intent.dnd_on");
                line(
                    &st,
                    "intent.dnd_on",
                    &[],
                    "好，我先安静待着，有事叫我。".into(),
                )
            } else {
                line(&st, "intent.dnd_off", &[], "好～那我可以说话啦。".into())
            }
        }
        Intent::Hide => {
            crate::companion::emit_pose(app, "intent.hide");
            after = Some(Box::new(|app: AppHandle| {
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
                    if let Some(p) = crate::desktop::pet(&app)
                        && p.is_visible().unwrap_or(true)
                    {
                        let _ = crate::desktop::toggle_pet_visible(&app);
                        crate::desktop::refresh_tray_menu(&app);
                    }
                });
            }));
            line(
                &st,
                "intent.hide",
                &[],
                "好，我先躲起来。想我了点一下右下角托盘里的图标就行。".into(),
            )
        }
        Intent::Look => {
            after = Some(Box::new(|app: AppHandle| {
                crate::vision::look_after_ack(&app, "用户在聊天里让你看一眼他正在看的窗口");
            }));
            if st.settings().vision.enabled {
                st.line("vision.ack", &[])
                    .unwrap_or_else(|| "嗯？我看看……".into())
            } else {
                "我还不能看屏幕：到「设置 → 活动感知」最下面打开「允许她看屏幕」就可以了。".into()
            }
        }
        Intent::Settings(tab) => {
            let t = tab.map(str::to_string);
            after = Some(Box::new(move |app: AppHandle| {
                crate::desktop::open_settings_tab_async(&app, t);
            }));
            line(&st, "intent.settings", &[], "好，帮你打开了。".into())
        }
        Intent::ListReminders => list_reminders(&st),
        Intent::CancelReminder(k) => cancel_reminder(app, &st, &k),
        Intent::PlansToday { explicit } => match crate::plans::supervise::today_answer(&st) {
            Some(a) => a,
            None if explicit => line(
                &st,
                "plan.today_none",
                &[],
                "今天还没有安排哦。想做计划的话跟我说「帮我做个计划」，或者右键我点「计划」。"
                    .into(),
            ),
            None => return false,
        },
        Intent::OpenPlans => {
            after = Some(Box::new(|app: AppHandle| {
                crate::desktop::open_plans_async(&app, Some("today".into()));
            }));
            line(&st, "plan.open", &[], "好，帮你打开了。".into())
        }
        Intent::NewPlan(text) => {
            let bare = NEW_PLAN_WORDS.iter().any(|w| {
                text.trim_end_matches(['吧', '。', '！', '!', '~', '～'])
                    .ends_with(w)
            });
            let view = if bare {
                "new".to_string()
            } else {
                format!("new:{text}")
            };
            after = Some(Box::new(move |app: AppHandle| {
                crate::desktop::open_plans_async(&app, Some(view));
            }));
            line(
                &st,
                "plan.new_open",
                &[],
                "好呀！计划窗口打开了，把想做的写进去，我来帮你排到每天。".into(),
            )
        }
    };
    // 记进聊天记录：之后聊天时她知道刚才做了什么
    chat::record_exchange(&st, Some(text), &say);
    crate::companion::interact(app, crate::companion::Interaction::Chat);
    chat::emit_pet_state(app, PetState::Happy);
    crate::voice::on_line(app, &say, crate::voice::Scene::Chat);
    let _ = channel.send(ChatEvent::Delta { text: say.clone() });
    let _ = channel.send(ChatEvent::Done {
        text: say,
        finish: FinishReason::Stop,
        task: None,
        actions: Vec::new(),
        reminder: None,
        reminder_form: false,
        open_game,
    });
    if let Some(f) = after {
        f(app.clone());
    }
    true
}

fn list_reminders(st: &AppState) -> String {
    let now = crate::reminders::now_local();
    let all = crate::reminders::list(&st.db()).unwrap_or_default();
    let active: Vec<_> = all
        .into_iter()
        .filter(|r| r.status == "active")
        .take(5)
        .map(|r| r.view(now))
        .collect();
    if active.is_empty() {
        return line(
            st,
            "intent.reminders_none",
            &[],
            "现在没有要提醒你的事哦。想让我记什么，直接说「X 点提醒我……」就行。".into(),
        );
    }
    let items: Vec<String> = active
        .iter()
        .map(|v| format!("{}：{}", v.when, v.reminder.content))
        .collect();
    let head = line(
        st,
        "intent.reminders",
        &[("数量", active.len().to_string())],
        format!("现在有 {} 件事要提醒你：", active.len()),
    );
    format!("{head}\n{}", items.join("\n"))
}

fn cancel_reminder(app: &AppHandle, st: &AppState, keyword: &str) -> String {
    let all = crate::reminders::list(&st.db()).unwrap_or_default();
    let active: Vec<_> = all.into_iter().filter(|r| r.status != "done").collect();
    let matched: Vec<_> = if keyword.is_empty() {
        // 「取消刚才的提醒」：最近建的那个
        active.iter().max_by_key(|r| r.id).into_iter().collect()
    } else {
        active
            .iter()
            .filter(|r| r.content.contains(keyword))
            .collect()
    };
    match matched.as_slice() {
        [r] => {
            let _ = crate::reminders::delete(&st.db(), r.id);
            crate::reminders::notify_changed(app);
            line(
                st,
                "intent.reminder_cancelled",
                &[("内容", r.content.clone())],
                format!("好，「{}」的提醒取消了。", r.content),
            )
        }
        [] => line(
            st,
            "intent.reminder_not_found",
            &[],
            "嗯……我没找到对得上的提醒。可以到「设置 → 提醒」里看看全部的。".into(),
        ),
        many => format!(
            "有 {} 个提醒都对得上：{}。说得具体一点，或者到「设置 → 提醒」里删吧。",
            many.len(),
            many.iter()
                .take(3)
                .map(|r| format!("「{}」", r.content))
                .collect::<Vec<_>>()
                .join("、")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_intents() {
        assert_eq!(detect("打开计划"), Some(Intent::OpenPlans));
        assert_eq!(
            detect("今天的计划呢"),
            Some(Intent::PlansToday { explicit: true })
        );
        assert_eq!(
            detect("今天还要做什么？"),
            Some(Intent::PlansToday { explicit: false })
        );
        assert_eq!(
            detect("帮我做个计划：一个月学完 Python 基础，每天一小时，周末休息，最后两天做个小项目练手"),
            Some(Intent::NewPlan(
                "帮我做个计划：一个月学完 Python 基础，每天一小时，周末休息，最后两天做个小项目练手".into()
            ))
        );
        assert_eq!(
            detect("帮我做个计划吧"),
            Some(Intent::NewPlan("帮我做个计划吧".into()))
        );
        // 聊天里顺口提到的不算
        assert_eq!(
            detect("我的计划是周末去爬山，你觉得怎么样我觉得会很好玩"),
            None
        );
        assert_eq!(detect("我不想做计划了，好累"), None);
        // 请求的各种说法都认
        for ask in [
            "千秋，帮我做个计划",
            "红莉栖，做个计划吧",
            "能帮我定个计划吗",
            "我想做一个学习计划",
            "做个计划",
            "我们来排个计划",
            "帮我规划一下期末复习",
        ] {
            assert!(
                matches!(detect(ask), Some(Intent::NewPlan(_))),
                "{ask} 应该打开新建计划"
            );
        }
        // 在讲自己的事、在问别的，不弹窗口
        for chat in [
            "昨天我做个计划结果没完成",
            "公司要做一个计划书",
            "帮我规划一下去京都的路线",
            "你会做个计划吗",
        ] {
            assert!(
                !matches!(detect(chat), Some(Intent::NewPlan(_))),
                "{chat} 不该打开新建计划"
            );
        }
    }

    #[test]
    fn games() {
        assert_eq!(detect("来一局井字棋吧"), Some(Intent::Game(Some("ttt"))));
        assert_eq!(detect("我们玩石头剪刀布"), Some(Intent::Game(Some("rps"))));
        assert_eq!(detect("井字棋！"), Some(Intent::Game(Some("ttt"))));
        assert_eq!(detect("陪我玩21点"), Some(Intent::Game(Some("blackjack"))));
        assert_eq!(detect("帮我抽个签"), Some(Intent::Game(Some("fortune"))));
        assert_eq!(detect("来玩海龟汤"), Some(Intent::Game(Some("turtle"))));
        assert_eq!(detect("想玩俄罗斯方块"), Some(Intent::Game(Some("blocks"))));
        assert_eq!(detect("一起玩点什么吧"), Some(Intent::Game(None)));
        assert_eq!(detect("我不想玩井字棋了"), None);
        assert_eq!(detect("昨天玩了好久的猜拳"), None);
        assert_eq!(detect("今天天气不错"), None);
        assert_eq!(
            detect("你知道井字棋这个游戏最早是从哪里来的吗？我一直很好奇它的历史和起源"),
            None,
            "长句交给聊天"
        );
        assert_eq!(game_id("井字棋"), Some("ttt"));
        assert_eq!(game_id(" 21 点 "), Some("blackjack"));
        assert_eq!(game_name("ttt"), "井字棋");
    }

    #[test]
    fn dnd_hide_look_settings() {
        assert_eq!(detect("我要专心写作业，别打扰我"), Some(Intent::Dnd(true)));
        assert_eq!(detect("安静一会儿"), Some(Intent::Dnd(true)));
        assert_eq!(detect("可以说话了"), Some(Intent::Dnd(false)));
        assert_eq!(detect("取消勿扰"), Some(Intent::Dnd(false)));
        assert_eq!(detect("你先躲一下"), Some(Intent::Hide));
        assert_eq!(detect("看看我的屏幕"), Some(Intent::Look));
        assert_eq!(detect("看我玩"), Some(Intent::Look));
        assert_eq!(detect("打开设置"), Some(Intent::Settings(None)));
        assert_eq!(
            detect("给我看看你的日记"),
            Some(Intent::Settings(Some("diary")))
        );
    }

    #[test]
    fn reminders() {
        assert_eq!(detect("我有哪些提醒？"), Some(Intent::ListReminders));
        assert_eq!(
            detect("取消买菜的提醒"),
            Some(Intent::CancelReminder("买菜".into()))
        );
        assert_eq!(
            detect("刚才那个提醒取消吧"),
            Some(Intent::CancelReminder(String::new()))
        );
        // 设提醒交给提醒模块
        assert_eq!(detect("半小时后提醒我收衣服"), None);
    }

    #[test]
    fn markers_map_to_games() {
        assert_eq!(game_id("井字棋（困难）"), Some("ttt"));
        assert_eq!(game_id("扫雷"), None);
    }
}
