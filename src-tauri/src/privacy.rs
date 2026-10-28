//! 看屏幕时的隐私保护（v2.1 P9，DECISIONS #47）。三层，全部在本机完成，不联网：
//!
//! 1. **看之前**（`block_reason`）：聊天软件、邮件、支付 / 证券、远程桌面、系统凭据窗口，
//!    以及标题里有登录、密码、邮箱、支付、订单、转账、验证码、身份证……的窗口，一律不看；
//!    焦点在密码框里时不看（`password_focused`，UI Automation）。
//! 2. **截什么**：只截前台那一个窗口（`screen.rs`）。
//! 3. **发之前**（`scan`）：Windows 自带的文字识别（`ocr`）读出画面上的字，把邮箱、手机号、证件号、卡号、
//!    金额、长数字、像密钥的字符串、带「密码 / 验证码 / 账号」的那一行涂黑；她主动看时遇到高风险内容直接不看。
//!
//! 窗口标题只在这里本地判断，不保存、不写日志、不发给模型。

/// 不看的原因（决定她说哪句台词）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// 隐私名单（密码管理器、隐身窗口、网银、用户设成「回避」的程序）
    Private,
    /// 聊天软件、邮件、支付 / 证券、远程桌面、系统设置和凭据窗口
    SensitiveApp,
    /// 标题看起来是登录、邮箱、支付、个人信息页面
    SensitiveTitle,
    /// 焦点在密码框里
    Password,
    /// 前台只有桌面 / 任务栏，没什么可看的
    Nothing,
}

impl Block {
    pub fn id(self) -> &'static str {
        match self {
            Block::Private => "private",
            Block::SensitiveApp => "sensitive_app",
            Block::SensitiveTitle => "sensitive_title",
            Block::Password => "password",
            Block::Nothing => "nothing",
        }
    }
}

/// 截屏时一律不看的程序（进程名小写）
const SENSITIVE_EXES: &[&str] = &[
    // 聊天
    "wechat.exe",
    "weixin.exe",
    "wechatappex.exe",
    "qq.exe",
    "qqnt.exe",
    "tim.exe",
    "wxwork.exe",
    "dingtalk.exe",
    "feishu.exe",
    "lark.exe",
    "telegram.exe",
    "discord.exe",
    "whatsapp.exe",
    "signal.exe",
    "line.exe",
    "slack.exe",
    "skype.exe",
    "teams.exe",
    "ms-teams.exe",
    "kakaotalk.exe",
    "viber.exe",
    "element.exe",
    // 邮件
    "outlook.exe",
    "olk.exe",
    "hxoutlook.exe",
    "thunderbird.exe",
    "foxmail.exe",
    "mailmaster.exe",
    "mailbird.exe",
    // 支付、证券、银行
    "alipay.exe",
    "hexin.exe",
    "tdxw.exe",
    "mainfree.exe",
    "eastmoney.exe",
    "futunn.exe",
    "ftnn.exe",
    "tigertrade.exe",
    "xiadan.exe",
    // 远程桌面
    "mstsc.exe",
    "todesk.exe",
    "sunloginclient.exe",
    "anydesk.exe",
    "teamviewer.exe",
    "rustdesk.exe",
    "parsecd.exe",
    // 系统：凭据、锁屏、设置（设置里有账户信息）
    "credentialuibroker.exe",
    "consent.exe",
    "lockapp.exe",
    "logonui.exe",
    "systemsettings.exe",
    // 密码管理器（隐私名单里也有）
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "dashlane.exe",
    "enpass.exe",
    "roboform.exe",
];

/// 标题里出现这些就不看（小写比较）。宁可多拦：误判只是她少看一眼。
const SENSITIVE_TITLES: &[&str] = &[
    // 登录、验证
    "登录",
    "登陆",
    "注册",
    "认证",
    "验证",
    "sign in",
    "sign-in",
    "signin",
    "log in",
    "login",
    "logon",
    "sign up",
    "two-factor",
    "2fa",
    "verify",
    "verification",
    // 密码
    "密码",
    "口令",
    "password",
    "passcode",
    // 邮件
    "邮箱",
    "邮件",
    "mail",
    "inbox",
    "收件箱",
    // 聊天（网页版、浏览器里的）
    "微信",
    "wechat",
    "telegram",
    "whatsapp",
    "messenger",
    "discord",
    "私信",
    "聊天",
    "chatgpt",
    "claude",
    // 支付、钱、交易
    "支付",
    "付款",
    "收银台",
    "checkout",
    "payment",
    "paypal",
    "alipay",
    "钱包",
    "wallet",
    "转账",
    "transfer",
    "银行",
    "bank",
    "账单",
    "账户",
    "帐户",
    "account",
    "订单",
    "交易",
    "证券",
    "股票",
    "基金",
    "理财",
    "信用卡",
    "credit card",
    "余额",
    "发票",
    "invoice",
    // 个人信息、政务、教务
    "身份证",
    "实名",
    "个人信息",
    "个人中心",
    "社保",
    "公积金",
    "个税",
    "税务",
    "病历",
    "体检",
    "教务",
    "学信网",
    "一网通办",
    "政务",
    // 隐身窗口
    "inprivate",
    "incognito",
    "隐身",
    "无痕",
];

/// 不看的原因（纯函数）。`private` = 活动感知已经判定为隐私窗口（隐私名单、用户设的「回避」）。
pub fn block_reason(exe: &str, title: &str, private: bool) -> Option<Block> {
    if private {
        return Some(Block::Private);
    }
    let exe = exe.to_lowercase();
    if SENSITIVE_EXES.contains(&exe.as_str()) {
        return Some(Block::SensitiveApp);
    }
    let t = title.to_lowercase();
    if SENSITIVE_TITLES.iter().any(|k| t.contains(k)) {
        return Some(Block::SensitiveTitle);
    }
    None
}

// ---------------- 第三层：识别出的文字里找敏感内容 ----------------

/// 一个识别出的词和它在图上的位置（像素）
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 要涂黑的一块（像素，相对识别用的那张图）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Findings {
    pub areas: Vec<Area>,
    /// 有密码、验证码、支付这类高风险内容（主动看时直接不看）
    pub high_risk: bool,
}

/// 整行涂黑、并且算高风险的关键词（去掉空格后小写比较）
const RISK_WORDS: &[&str] = &[
    "密码",
    "口令",
    "验证码",
    "校验码",
    "动态码",
    "支付",
    "付款",
    "转账",
    "银行卡",
    "卡号",
    "身份证",
    "证件号",
    "余额",
    "password",
    "passcode",
    "pincode",
    "cvv",
    "verificationcode",
];

/// 整行涂黑、但不算高风险的关键词（个人信息的标签）
const LABEL_WORDS: &[&str] = &[
    "账号",
    "帐号",
    "账户",
    "帐户",
    "用户名",
    "手机号",
    "电话",
    "邮箱",
    "地址",
    "学号",
    "工号",
    "姓名",
    "订单号",
    "account",
    "username",
    "e-mail",
    "email",
    "phone",
    "address",
];

fn is_digitish(c: char) -> bool {
    c.is_ascii_digit()
}

/// 词里连续的数字（允许中间夹空格、短横线、点）最长有多少位——跨词拼起来看，返回每个词所在的数字串长度
fn digit_runs(words: &[Word]) -> Vec<usize> {
    // 把一行拼成字符流，记下每个字符属于哪个词
    let mut chars: Vec<(char, usize)> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            chars.push((' ', usize::MAX));
        }
        for c in w.text.chars() {
            chars.push((c, i));
        }
    }
    let mut out = vec![0usize; words.len()];
    let mut i = 0;
    while i < chars.len() {
        if !is_digitish(chars[i].0) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut digits = 0;
        let mut last_digit = i;
        while j < chars.len() {
            let c = chars[j].0;
            if is_digitish(c) {
                digits += 1;
                last_digit = j;
            } else if !(matches!(c, ' ' | '-' | '.' | ',')
                && j + 1 < chars.len()
                && is_digitish(chars[j + 1].0))
            {
                break;
            }
            j += 1;
        }
        for &(_, wi) in &chars[i..=last_digit] {
            if wi != usize::MAX {
                out[wi] = out[wi].max(digits);
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// 像不像密钥 / 令牌：20 个字符以上、字母和数字都有、没有空格（sk-…、ghp_…、一长串十六进制）
fn secret_like(t: &str) -> bool {
    let n = t.chars().count();
    n >= 20
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '+' | '='))
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

fn email_like(t: &str) -> bool {
    let Some(at) = t.find('@') else { return false };
    at > 0 && t[at + 1..].contains('.')
}

/// 密码框里的圆点
fn dots_like(t: &str) -> bool {
    t.chars()
        .filter(|c| matches!(c, '•' | '●' | '*' | '∙' | '·'))
        .count()
        >= 4
}

fn union(words: &[Word]) -> Option<Area> {
    let mut it = words.iter();
    let f = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (f.x, f.y, f.x + f.w, f.y + f.h);
    for w in it {
        x0 = x0.min(w.x);
        y0 = y0.min(w.y);
        x1 = x1.max(w.x + w.w);
        y1 = y1.max(w.y + w.h);
    }
    Some(Area {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

fn area(w: &Word) -> Area {
    Area {
        x: w.x,
        y: w.y,
        w: w.w,
        h: w.h,
    }
}

/// 在识别出的各行文字里找要涂黑的地方（纯函数）。
/// `strict`：不是游戏画面时更严——任何 6 位以上的数字都涂（学号、订单号、账号）；游戏里只涂像手机号、证件号、卡号的长数字（分数、金币不涂）。
pub fn scan(lines: &[Vec<Word>], strict: bool) -> Findings {
    let mut f = Findings::default();
    for words in lines {
        if words.is_empty() {
            continue;
        }
        let joined: String = words
            .iter()
            .flat_map(|w| w.text.chars())
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_lowercase();
        let risk = RISK_WORDS.iter().any(|k| joined.contains(k));
        let label = LABEL_WORDS.iter().any(|k| joined.contains(k));
        if risk || label {
            f.high_risk |= risk;
            if let Some(a) = union(words) {
                f.areas.push(a);
            }
            continue;
        }
        let runs = digit_runs(words);
        // 货币符号 / 「元」：这一行的数字都涂掉
        let money = joined.contains('¥')
            || joined.contains('￥')
            || joined.contains('$')
            || joined.contains('€')
            || joined.contains("元")
            || joined.contains("rmb")
            || joined.contains("cny")
            || joined.contains("usd");
        for (i, w) in words.iter().enumerate() {
            let t = w.text.trim();
            let long_digits = if strict { runs[i] >= 6 } else { runs[i] >= 11 };
            if email_like(t)
                || secret_like(t)
                || dots_like(t)
                || long_digits
                || (money && t.chars().any(|c| c.is_ascii_digit()))
            {
                f.areas.push(area(w));
            }
        }
    }
    f
}

// ---------------- Windows：密码框、文字识别 ----------------

#[cfg(windows)]
fn com_init() {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    // 同一个线程重复初始化会返回 S_FALSE，不影响
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// 焦点是不是在密码框里（UI Automation；最多等 1 秒，拿不到就当不是）
#[cfg(windows)]
pub fn password_focused() -> bool {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomation2};
    use windows::core::Interface;
    com_init();
    unsafe {
        let Ok(uia) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        else {
            return false;
        };
        if let Ok(u2) = uia.cast::<IUIAutomation2>() {
            let _ = u2.SetConnectionTimeout(1000);
            let _ = u2.SetTransactionTimeout(1000);
        }
        uia.GetFocusedElement()
            .and_then(|e| e.CurrentIsPassword())
            .map(|b| b.as_bool())
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
pub fn password_focused() -> bool {
    false
}

/// 用 Windows 自带的文字识别读出图上的字（BGRA）。系统没有可用的识别语言时返回 None。
#[cfg(windows)]
pub fn ocr(bgra: &[u8], w: u32, h: u32) -> Option<Vec<Vec<Word>>> {
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::core::HSTRING;
    com_init();
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .ok()
        .or_else(|| {
            ["zh-Hans-CN", "zh-Hans", "en-US"].iter().find_map(|l| {
                Language::CreateLanguage(&HSTRING::from(*l))
                    .and_then(|lang| OcrEngine::TryCreateFromLanguage(&lang))
                    .ok()
            })
        })?;
    let max = OcrEngine::MaxImageDimension().unwrap_or(4096);
    if w > max || h > max {
        return None;
    }
    let buf = CryptographicBuffer::CreateFromByteArray(bgra).ok()?;
    let bmp =
        SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Bgra8, w as i32, h as i32)
            .ok()?;
    let result = engine.RecognizeAsync(&bmp).ok()?.join().ok()?;
    let mut lines = Vec::new();
    for line in result.Lines().ok()? {
        let mut words = Vec::new();
        if let Ok(ws) = line.Words() {
            for word in ws {
                let (Ok(t), Ok(r)) = (word.Text(), word.BoundingRect()) else {
                    continue;
                };
                words.push(Word {
                    text: t.to_string_lossy(),
                    x: r.X,
                    y: r.Y,
                    w: r.Width,
                    h: r.Height,
                });
            }
        }
        lines.push(words);
    }
    Some(lines)
}

#[cfg(not(windows))]
pub fn ocr(_bgra: &[u8], _w: u32, _h: u32) -> Option<Vec<Vec<Word>>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(texts: &[&str]) -> Vec<Word> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| Word {
                text: (*t).into(),
                x: i as f32 * 50.0,
                y: 0.0,
                w: 40.0,
                h: 12.0,
            })
            .collect()
    }

    #[test]
    fn sensitive_apps_and_titles_are_never_looked_at() {
        assert_eq!(
            block_reason("WeChat.exe", "", false),
            Some(Block::SensitiveApp)
        );
        assert_eq!(block_reason("QQ.exe", "", false), Some(Block::SensitiveApp));
        assert_eq!(
            block_reason("OUTLOOK.EXE", "", false),
            Some(Block::SensitiveApp)
        );
        assert_eq!(
            block_reason("mstsc.exe", "", false),
            Some(Block::SensitiveApp)
        );
        assert_eq!(
            block_reason("chrome.exe", "统一身份认证 - 某某大学", false),
            Some(Block::SensitiveTitle)
        );
        assert_eq!(
            block_reason("msedge.exe", "收件箱 - Gmail", false),
            Some(Block::SensitiveTitle)
        );
        assert_eq!(
            block_reason("chrome.exe", "我的订单 - 京东", false),
            Some(Block::SensitiveTitle)
        );
        assert_eq!(
            block_reason("chrome.exe", "Sign in to GitHub", false),
            Some(Block::SensitiveTitle)
        );
        assert_eq!(
            block_reason("chrome.exe", "微信网页版", false),
            Some(Block::SensitiveTitle)
        );
        assert_eq!(
            block_reason("game.exe", "Hades", true),
            Some(Block::Private)
        );
        assert_eq!(block_reason("Hades.exe", "Hades", false), None);
        assert_eq!(block_reason("chrome.exe", "Rust 文档", false), None);
    }

    #[test]
    fn keyword_lines_are_masked_and_risky() {
        let f = scan(&[line(&["密", "码", "：", "••••••••"])], true);
        assert!(f.high_risk);
        assert_eq!(f.areas.len(), 1, "整行涂掉");
        assert_eq!(f.areas[0].w, 190.0);
        let f = scan(&[line(&["账号", "zhang3"])], true);
        assert!(!f.high_risk, "账号只是标签，不算高风险");
        assert_eq!(f.areas.len(), 1);
        let f = scan(&[line(&["Enter", "your", "Password"])], false);
        assert!(f.high_risk);
    }

    #[test]
    fn personal_numbers_emails_secrets_are_masked() {
        // 手机号、邮箱、密钥
        let f = scan(
            &[
                line(&["联系", "13812345678"]),
                line(&["me@example.com"]),
                line(&["sk-abcdEFGH1234ijklMNOP5678"]), // 假的测试数据 gitleaks:allow
            ],
            false,
        );
        assert_eq!(f.areas.len(), 3);
        assert!(!f.high_risk);
        // 分开写的卡号（跨词）
        let f = scan(&[line(&["6222", "0210", "0011", "2233"])], false);
        assert_eq!(f.areas.len(), 4);
        // 金额
        let f = scan(&[line(&["合计", "¥", "128.50"])], false);
        assert_eq!(f.areas.len(), 1);
    }

    #[test]
    fn game_scores_are_kept_but_strict_mode_masks_ids() {
        let score = [line(&["SCORE", "1234567"])];
        assert!(scan(&score, false).areas.is_empty(), "游戏里的分数不涂");
        assert_eq!(scan(&score, true).areas.len(), 1, "不是游戏时 6 位以上都涂");
        assert!(scan(&[line(&["HP", "120/300"])], true).areas.is_empty());
        assert!(scan(&[line(&["第", "3", "关"])], true).areas.is_empty());
    }

    #[test]
    fn digit_runs_cross_words() {
        let ws = line(&["ID", "4401", "0619", "9001", "011234"]);
        let r = digit_runs(&ws);
        assert_eq!(r[0], 0);
        assert_eq!(r[1], 18);
        assert_eq!(r[4], 18);
    }

    /// 本机冒烟（默认忽略）：系统有没有可用的文字识别、焦点检测会不会卡住。只打印行数，不打印识别出的文字。
    #[test]
    #[ignore]
    fn real_ocr_smoke() {
        // 一张 200×60 的白图
        let (w, h) = (200u32, 60u32);
        let img = vec![255u8; (w * h * 4) as usize];
        let t = std::time::Instant::now();
        let r = ocr(&img, w, h);
        println!("OCR 可用：{}，{:?}", r.is_some(), t.elapsed());
        let t = std::time::Instant::now();
        println!("焦点是密码框：{}，{:?}", password_focused(), t.elapsed());
    }
}
