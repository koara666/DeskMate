//! 前台程序分类（纯本地规则，DECISIONS #40）。
//!
//! 输入是进程名、程序路径和窗口标题；**窗口标题只在这里用来判断（视频网站、隐身窗口、网银），不保存、不发给模型**。
//! 输出分类 + 「清洗过的程序名」：只来自下面的已知程序表或 Steam 等游戏库的文件夹名，不会是窗口标题里的任何内容。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Game,
    Coding,
    Writing,
    Video,
    Music,
    Browsing,
    Chat,
    Meeting,
    Other,
    /// 好几分钟没有键盘鼠标操作
    Idle,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Game,
        Category::Coding,
        Category::Writing,
        Category::Video,
        Category::Music,
        Category::Browsing,
        Category::Chat,
        Category::Meeting,
        Category::Other,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Category::Game => "game",
            Category::Coding => "coding",
            Category::Writing => "writing",
            Category::Video => "video",
            Category::Music => "music",
            Category::Browsing => "browsing",
            Category::Chat => "chat",
            Category::Meeting => "meeting",
            Category::Other => "other",
            Category::Idle => "idle",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Category::Game => "玩游戏",
            Category::Coding => "写代码",
            Category::Writing => "写东西",
            Category::Video => "看视频",
            Category::Music => "听音乐",
            Category::Browsing => "上网",
            Category::Chat => "聊天",
            Category::Meeting => "开会",
            Category::Other => "用电脑",
            Category::Idle => "离开电脑",
        }
    }
    pub fn from_id(s: &str) -> Option<Category> {
        Category::ALL
            .iter()
            .copied()
            .chain(std::iter::once(Category::Idle))
            .find(|c| c.id() == s)
    }
}

/// 分类结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub category: Category,
    /// 清洗过的程序名（已知程序或游戏文件夹名），没有就是 None
    pub label: Option<String>,
    /// 进程名（小写，只在本地用：用户规则、最近用过的程序列表）
    pub exe: String,
    /// 隐私名单：前台时完全不记录、不评论
    pub private: bool,
}

/// 已知程序：进程名 → (分类, 显示名)
const KNOWN: &[(&str, Category, &str)] = &[
    // 写代码
    ("code.exe", Category::Coding, "VS Code"),
    ("cursor.exe", Category::Coding, "Cursor"),
    ("windsurf.exe", Category::Coding, "Windsurf"),
    ("zed.exe", Category::Coding, "Zed"),
    ("devenv.exe", Category::Coding, "Visual Studio"),
    ("idea64.exe", Category::Coding, "IntelliJ IDEA"),
    ("pycharm64.exe", Category::Coding, "PyCharm"),
    ("webstorm64.exe", Category::Coding, "WebStorm"),
    ("clion64.exe", Category::Coding, "CLion"),
    ("goland64.exe", Category::Coding, "GoLand"),
    ("rider64.exe", Category::Coding, "Rider"),
    ("rustrover64.exe", Category::Coding, "RustRover"),
    ("studio64.exe", Category::Coding, "Android Studio"),
    ("sublime_text.exe", Category::Coding, "Sublime Text"),
    ("windowsterminal.exe", Category::Coding, "终端"),
    ("wt.exe", Category::Coding, "终端"),
    ("powershell.exe", Category::Coding, "终端"),
    ("pwsh.exe", Category::Coding, "终端"),
    // 写东西 / 作业
    ("winword.exe", Category::Writing, "Word"),
    ("excel.exe", Category::Writing, "Excel"),
    ("powerpnt.exe", Category::Writing, "PowerPoint"),
    ("wps.exe", Category::Writing, "WPS"),
    ("et.exe", Category::Writing, "WPS 表格"),
    ("wpp.exe", Category::Writing, "WPS 演示"),
    ("onenote.exe", Category::Writing, "OneNote"),
    ("notion.exe", Category::Writing, "Notion"),
    ("obsidian.exe", Category::Writing, "Obsidian"),
    ("typora.exe", Category::Writing, "Typora"),
    ("notepad.exe", Category::Writing, "记事本"),
    ("notepad++.exe", Category::Writing, "Notepad++"),
    // 看视频
    ("potplayermini64.exe", Category::Video, "PotPlayer"),
    ("potplayermini.exe", Category::Video, "PotPlayer"),
    ("vlc.exe", Category::Video, "VLC"),
    ("mpc-hc64.exe", Category::Video, "MPC"),
    ("mpc-be64.exe", Category::Video, "MPC"),
    ("mpv.exe", Category::Video, "mpv"),
    ("qqlive.exe", Category::Video, "腾讯视频"),
    ("qiyiclient.exe", Category::Video, "爱奇艺"),
    ("bilibili.exe", Category::Video, "哔哩哔哩"),
    // 听音乐
    ("cloudmusic.exe", Category::Music, "网易云音乐"),
    ("qqmusic.exe", Category::Music, "QQ 音乐"),
    ("spotify.exe", Category::Music, "Spotify"),
    ("kugou.exe", Category::Music, "酷狗"),
    ("foobar2000.exe", Category::Music, "foobar2000"),
    ("aimp.exe", Category::Music, "AIMP"),
    // 聊天
    ("wechat.exe", Category::Chat, "微信"),
    ("weixin.exe", Category::Chat, "微信"),
    ("qq.exe", Category::Chat, "QQ"),
    ("telegram.exe", Category::Chat, "Telegram"),
    ("discord.exe", Category::Chat, "Discord"),
    ("line.exe", Category::Chat, "LINE"),
    ("slack.exe", Category::Chat, "Slack"),
    ("dingtalk.exe", Category::Chat, "钉钉"),
    ("feishu.exe", Category::Chat, "飞书"),
    ("lark.exe", Category::Chat, "飞书"),
    // 开会
    ("zoom.exe", Category::Meeting, "Zoom"),
    ("teams.exe", Category::Meeting, "Teams"),
    ("ms-teams.exe", Category::Meeting, "Teams"),
    ("wemeetapp.exe", Category::Meeting, "腾讯会议"),
    // 浏览器（标题再细分）
    ("chrome.exe", Category::Browsing, "Chrome"),
    ("msedge.exe", Category::Browsing, "Edge"),
    ("firefox.exe", Category::Browsing, "Firefox"),
    ("brave.exe", Category::Browsing, "Brave"),
    ("opera.exe", Category::Browsing, "Opera"),
    ("360se.exe", Category::Browsing, "360 浏览器"),
    ("qqbrowser.exe", Category::Browsing, "QQ 浏览器"),
    ("vivaldi.exe", Category::Browsing, "Vivaldi"),
    // 常见游戏（不在 Steam / Epic 文件夹里的）
    ("yuanshen.exe", Category::Game, "原神"),
    ("genshinimpact.exe", Category::Game, "原神"),
    ("starrail.exe", Category::Game, "崩坏：星穹铁道"),
    ("zenlesszonezero.exe", Category::Game, "绝区零"),
    ("league of legends.exe", Category::Game, "英雄联盟"),
    ("valorant-win64-shipping.exe", Category::Game, "无畏契约"),
    ("minecraft.exe", Category::Game, "Minecraft"),
    ("javaw.exe", Category::Other, "Java"),
];

/// 游戏库文件夹：路径里出现这些就算游戏，下一级文件夹名就是游戏名
const GAME_DIRS: &[&str] = &[
    "\\steamapps\\common\\",
    "\\epic games\\",
    "\\gog galaxy\\games\\",
    "\\riot games\\",
    "\\xboxgames\\",
    "\\ubisoft game launcher\\games\\",
];

/// 平台 / 启动器本身不算在玩游戏
const LAUNCHERS: &[&str] = &[
    "steam.exe",
    "steamwebhelper.exe",
    "epicgameslauncher.exe",
    "galaxyclient.exe",
    "riotclientservices.exe",
    "riotclientux.exe",
];

/// 默认的隐私名单：密码管理器
const PRIVATE_EXES: &[&str] = &[
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "dashlane.exe",
    "enpass.exe",
    "roboform.exe",
];

/// 标题里出现这些就当隐私窗口：隐身 / 无痕浏览、网银、支付
const PRIVATE_TITLES: &[&str] = &[
    "inprivate",
    "incognito",
    "隐身",
    "无痕",
    "隐私浏览",
    "private browsing",
    "网上银行",
    "网银",
    "银行",
    "bank",
    "支付宝",
    "alipay",
    "paypal",
    "password",
    "密码管理",
];

const VIDEO_TITLES: &[&str] = &[
    "bilibili",
    "哔哩哔哩",
    "youtube",
    "爱奇艺",
    "优酷",
    "腾讯视频",
    "netflix",
    "prime video",
    "twitch",
    "抖音",
    "斗鱼",
    "虎牙",
    "niconico",
    "芒果tv",
];

const MUSIC_TITLES: &[&str] = &["网易云音乐", "spotify", "qq音乐", "apple music"];

/// 从路径里取游戏名：游戏库文件夹下一级的文件夹名
fn game_from_path(path: &str) -> Option<String> {
    let lower = path.to_lowercase().replace('/', "\\");
    for d in GAME_DIRS {
        if let Some(i) = lower.find(d) {
            let start = i + d.len();
            // 用原始路径取名字（保留大小写）
            let rest = path.get(start..)?;
            let name = rest.split(['\\', '/']).next()?.trim();
            if name.is_empty() {
                return None;
            }
            return Some(name.chars().take(40).collect());
        }
    }
    None
}

/// 分类（纯函数）。`extra_private` 是用户加进隐私名单的进程名（小写）。
pub fn classify(exe: &str, path: &str, title: &str, extra_private: &[String]) -> Sample {
    let exe = exe.to_lowercase();
    let t = title.to_lowercase();
    let private = PRIVATE_EXES.contains(&exe.as_str())
        || extra_private.contains(&exe)
        || PRIVATE_TITLES.iter().any(|k| t.contains(k));
    let mut s = Sample {
        category: Category::Other,
        label: None,
        exe: exe.clone(),
        private,
    };
    if private {
        return s;
    }
    if !LAUNCHERS.contains(&exe.as_str())
        && let Some(g) = game_from_path(path)
    {
        s.category = Category::Game;
        s.label = Some(g);
        return s;
    }
    if let Some((_, c, name)) = KNOWN.iter().find(|(e, _, _)| *e == exe) {
        s.category = *c;
        s.label = Some((*name).into());
    }
    // Minecraft Java 版是 javaw.exe
    if exe == "javaw.exe" && t.contains("minecraft") {
        s.category = Category::Game;
        s.label = Some("Minecraft".into());
    }
    // 浏览器：按标题看是不是在看视频 / 听歌（标题不保存）
    if s.category == Category::Browsing {
        if VIDEO_TITLES.iter().any(|k| t.contains(k)) {
            s.category = Category::Video;
        } else if MUSIC_TITLES.iter().any(|k| t.contains(k)) {
            s.category = Category::Music;
        }
    }
    // 聊天软件里开着会议窗口
    if s.category == Category::Chat && (t.contains("会议") || t.contains("meeting")) {
        s.category = Category::Meeting;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(exe: &str, path: &str, title: &str) -> Sample {
        classify(exe, path, title, &[])
    }

    #[test]
    fn known_programs() {
        let s = c(
            "Code.exe",
            "C:\\Users\\x\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
            "main.rs - 秘密项目 - Visual Studio Code",
        );
        assert_eq!(s.category, Category::Coding);
        assert_eq!(
            s.label.as_deref(),
            Some("VS Code"),
            "程序名来自已知表，不是窗口标题"
        );
        assert_eq!(
            c("WINWORD.EXE", "", "论文.docx").category,
            Category::Writing
        );
        assert_eq!(c("WeChat.exe", "", "张三").category, Category::Chat);
        assert_eq!(c("WeChat.exe", "", "腾讯会议").category, Category::Meeting);
        assert_eq!(c("unknown.exe", "", "随便").category, Category::Other);
        assert_eq!(c("unknown.exe", "", "随便").label, None);
    }

    #[test]
    fn games_from_library_folders() {
        let s = c(
            "DR2_us.exe",
            "F:\\SteamLibrary\\steamapps\\common\\Danganronpa 2 Goodbye Despair\\DR2_us.exe",
            "",
        );
        assert_eq!(s.category, Category::Game);
        assert_eq!(s.label.as_deref(), Some("Danganronpa 2 Goodbye Despair"));
        let s = c("Game.exe", "D:\\Epic Games\\Hades\\x64\\Game.exe", "");
        assert_eq!(s.label.as_deref(), Some("Hades"));
        assert_eq!(
            c(
                "steam.exe",
                "C:\\Program Files (x86)\\Steam\\steam.exe",
                "Steam"
            )
            .category,
            Category::Other
        );
        assert_eq!(c("YuanShen.exe", "", "原神").label.as_deref(), Some("原神"));
        assert_eq!(
            c("javaw.exe", "", "Minecraft 1.21").category,
            Category::Game
        );
    }

    #[test]
    fn browser_titles_refine_but_are_not_kept() {
        let s = c(
            "chrome.exe",
            "",
            "【4K】某个视频 - 哔哩哔哩 (゜-゜)つロ 干杯~-bilibili",
        );
        assert_eq!(s.category, Category::Video);
        assert_eq!(s.label.as_deref(), Some("Chrome"));
        assert_eq!(
            c("msedge.exe", "", "Rust 文档").category,
            Category::Browsing
        );
        assert_eq!(c("msedge.exe", "", "网易云音乐").category, Category::Music);
    }

    #[test]
    fn private_list() {
        assert!(c("KeePassXC.exe", "", "数据库").private);
        assert!(
            c(
                "msedge.exe",
                "",
                "新建标签页 - [InPrivate] - Microsoft Edge"
            )
            .private
        );
        assert!(c("chrome.exe", "", "招商银行 个人网银").private);
        assert!(c("chrome.exe", "", "新标签页 - 无痕模式").private);
        let s = classify("secret.exe", "", "", &["secret.exe".into()]);
        assert!(s.private);
        assert_eq!(s.label, None, "隐私窗口什么都不记");
        assert!(!c("chrome.exe", "", "天气预报").private);
    }

    #[test]
    fn category_ids_roundtrip() {
        for cat in Category::ALL {
            assert_eq!(Category::from_id(cat.id()), Some(cat));
        }
        assert_eq!(Category::from_id("idle"), Some(Category::Idle));
        assert_eq!(Category::from_id("nope"), None);
    }
}
