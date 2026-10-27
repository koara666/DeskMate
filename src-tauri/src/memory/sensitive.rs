//! 敏感信息过滤：写入数据库（聊天记录、记忆）之前用规则检查。
//! 绝不保存：API Key、密码、验证码、证件号、银行卡号（需求第 7 节）。
//!
//! - `contains_sensitive`：用于决定一条记忆要不要直接丢弃。
//! - `scrub`：把敏感片段替换成「[已隐藏]」，用于聊天记录和摘要。

use crate::redact::redact_secrets;

const MASK: &str = "[已隐藏]";

/// 「密码 / 验证码」等关键词，后面紧跟的值视为敏感
const SECRET_KEYWORDS: &[&str] = &[
    "密码",
    "口令",
    "密钥",
    "秘钥",
    "验证码",
    "校验码",
    "动态码",
    "取件码",
    "支付码",
    "pin码",
    "password",
    "passwd",
    "pwd",
    "passcode",
    "otp",
    "token",
    "api key",
    "apikey",
    "secret",
];

/// 关键词后允许出现的连接符
const JOINERS: &[&str] = &[
    "是", "为", "：", ":", "=", "＝", " ", "是：", "是:", "改成", "设成", "设为",
];

fn luhn_ok(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = (*d - b'0') as u32;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

/// 身份证号：17 位数字 + 数字或 X，出生年份 19xx/20xx，月份 01~12
fn looks_like_id_card(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 18
        && b[..17].iter().all(u8::is_ascii_digit)
        && (b[17].is_ascii_digit() || b[17] == b'X' || b[17] == b'x')
        && (&s[6..8] == "19" || &s[6..8] == "20")
        && matches!(
            &s[10..12],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

/// 找出所有需要隐藏的字节区间
fn sensitive_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let lower = text.to_lowercase();
    // lower 与 text 的字节位置在 ASCII 部分一致；只对 ASCII 关键词做大小写无关匹配
    let same_len = lower.len() == text.len();

    // 1. 数字串：身份证号、银行卡号（允许中间有空格或短横线）
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut digits: Vec<u8> = Vec::new();
            let mut end = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || ((bytes[i] == b' ' || bytes[i] == b'-')
                        && i + 1 < bytes.len()
                        && bytes[i + 1].is_ascii_digit()))
            {
                if bytes[i].is_ascii_digit() {
                    digits.push(bytes[i]);
                    end = i + 1;
                }
                i += 1;
            }
            // 身份证末位可能是 X
            if digits.len() == 17 && end < bytes.len() && (bytes[end] == b'X' || bytes[end] == b'x')
            {
                let s: String = digits
                    .iter()
                    .map(|d| *d as char)
                    .chain(std::iter::once('X'))
                    .collect();
                if looks_like_id_card(&s) {
                    spans.push((start, end + 1));
                    i = end + 1;
                    continue;
                }
            }
            let s: String = digits.iter().map(|d| *d as char).collect();
            if (digits.len() == 18 && looks_like_id_card(&s))
                || ((15..=19).contains(&digits.len()) && luhn_ok(&digits))
            {
                spans.push((start, end));
            }
        } else {
            i += 1;
        }
    }

    // 2. 关键词后面的值：「密码是 abc123」「验证码：834921」
    if same_len {
        for kw in SECRET_KEYWORDS {
            let mut from = 0;
            while let Some(pos) = lower[from..].find(kw) {
                let kw_end = from + pos + kw.len();
                from = kw_end;
                let mut rest_start = kw_end;
                let rest = &text[kw_end..];
                let mut joiners = JOINERS.to_vec();
                joiners.sort_by_key(|j| std::cmp::Reverse(j.len()));
                for j in joiners {
                    if rest.starts_with(j) {
                        rest_start = kw_end + j.len();
                        break;
                    }
                }
                let rest = &text[rest_start..];
                let rest = rest.trim_start();
                let val_start = text.len() - rest.len();
                let val_len: usize = rest
                    .chars()
                    .take_while(|c| !c.is_whitespace() && !"，。,;；！？）」】".contains(*c))
                    .map(char::len_utf8)
                    .sum();
                let val = &text[val_start..val_start + val_len];
                // 值至少 4 个字符，且含数字或拉丁字母（避免把「密码忘了」当成密码）
                if val.chars().count() >= 4 && val.chars().any(|c| c.is_ascii_alphanumeric()) {
                    spans.push((val_start, val_start + val_len));
                }
            }
        }
    }

    spans.sort();
    // 合并重叠区间
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// 是否包含不能保存的敏感信息
pub fn contains_sensitive(text: &str) -> bool {
    !sensitive_spans(text).is_empty() || redact_secrets(text) != text
}

/// 把敏感片段替换成「[已隐藏]」
pub fn scrub(text: &str) -> String {
    let spans = sensitive_spans(text);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (s, e) in spans {
        out.push_str(&text[last..s]);
        out.push_str(MASK);
        last = e;
    }
    out.push_str(&text[last..]);
    redact_secrets(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_codes() {
        assert_eq!(
            scrub("我的密码是 abc12345，别忘了"),
            "我的密码是 [已隐藏]，别忘了"
        );
        assert_eq!(scrub("验证码：834921"), "验证码：[已隐藏]");
        assert_eq!(scrub("Password=Hunter2!x"), "Password=[已隐藏]");
        assert!(contains_sensitive("wifi 密码 qwerty123"));
        // 只是提到密码这个词，不算
        assert!(!contains_sensitive("我又把密码忘了"));
        assert!(!contains_sensitive("验证码一直收不到"));
    }

    #[test]
    fn id_card_numbers() {
        // 结构合法的虚构号码（出生日期 1990-01-01）
        let id = ["11010519", "900101", "1234"].concat();
        assert!(contains_sensitive(&format!("身份证 {id}")));
        assert_eq!(scrub(&format!("号码{id}吗")), "号码[已隐藏]吗");
        let idx = ["11010519", "900101", "123X"].concat();
        assert_eq!(scrub(&format!("{idx}。")), "[已隐藏]。");
    }

    #[test]
    fn bank_cards_need_luhn() {
        // 通过 Luhn 校验的测试卡号
        assert!(contains_sensitive("卡号 4111 1111 1111 1111"));
        assert_eq!(scrub("卡号 4111-1111-1111-1111 ok"), "卡号 [已隐藏] ok");
        // 普通长数字（订单号之类）不通过 Luhn，不算
        assert!(!contains_sensitive("订单号 1234567890123456"));
        assert!(!contains_sensitive("今年 2026 年，我 25 岁"));
    }

    #[test]
    fn api_keys_reuse_redaction() {
        let k = ["sk-", "abcdefghij0123456789XYZ"].concat();
        assert!(contains_sensitive(&format!("我的 key 是 {k}")));
        assert!(!scrub(&format!("用 {k} 试试")).contains("abcdefghij"));
    }

    #[test]
    fn normal_text_untouched() {
        for s in [
            "我叫小林，喜欢猫",
            "明天下午三点开会",
            "电话联系",
            "token 用完了",
        ] {
            assert_eq!(scrub(s), s);
        }
    }
}
