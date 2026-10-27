//! 密钥脱敏：日志和展示给用户的错误信息在输出前都要过一遍。
//! 这是兜底措施——正常代码路径本来就不应该把 Key 放进这些字符串里。

/// 看起来像 API Key 的前缀
const KEY_PREFIXES: [&str; 4] = ["sk-", "AIza", "xai-", "gsk_"];
const MIN_KEY_BODY: usize = 16;

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// 把字符串里疑似密钥的片段替换为 `[已隐藏]`。
/// 规则：已知前缀 + 至少 16 个字母数字；或者 Bearer 后面的令牌；或者 `key=` 查询参数。
pub fn redact_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'outer: while !rest.is_empty() {
        // Bearer xxx
        if let Some(r) = strip_prefix_ci(rest, "bearer ") {
            let body_len = r
                .chars()
                .take_while(|c| is_key_char(*c) || *c == '.')
                .count();
            if body_len >= 8 {
                out.push_str(&rest[..rest.len() - r.len()]);
                out.push_str("[已隐藏]");
                rest = &r[body_len..];
                continue;
            }
        }
        // key=xxx / api_key=xxx（URL 查询参数）
        for p in ["key=", "api_key=", "apikey="] {
            if let Some(r) = strip_prefix_ci(rest, p) {
                let body_len = r.chars().take_while(|c| is_key_char(*c)).count();
                if body_len >= 8 {
                    out.push_str(&rest[..p.len()]);
                    out.push_str("[已隐藏]");
                    rest = &r[body_len..];
                    continue 'outer;
                }
            }
        }
        for p in KEY_PREFIXES {
            if let Some(r) = rest.strip_prefix(p) {
                let body_len = r.chars().take_while(|c| is_key_char(*c)).count();
                if body_len >= MIN_KEY_BODY {
                    out.push_str("[已隐藏]");
                    rest = &r[body_len..];
                    continue 'outer;
                }
            }
        }
        let c = rest.chars().next().expect("rest 非空");
        out.push(c);
        rest = &rest[c.len_utf8()..];
        // 跳过当前单词剩余部分，避免在单词中间误匹配（比如 task-xxxx 里的 sk-）
        if is_key_char(c) {
            let skip = rest
                .chars()
                .take_while(|c| is_key_char(*c))
                .map(char::len_utf8)
                .sum::<usize>();
            out.push_str(&rest[..skip]);
            rest = &rest[skip..];
        }
    }
    out
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// 只显示末尾 4 位，比如 `••••a1b2`
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.len() <= 4 {
        return "••••".into();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_key_formats() {
        let k = ["sk-", "a1B2c3D4e5F6g7H8i9J0"].concat();
        let g = ["AIza", "SyA1234567890abcdefghijklmnopqrstu"].concat();
        let s = format!("用了 {k} 和 {g} 两个 key");
        let r = redact_secrets(&s);
        assert!(!r.contains("a1B2c3D4"));
        assert!(!r.contains("SyA1234567890"));
        assert_eq!(r.matches("[已隐藏]").count(), 2);
    }

    #[test]
    fn redacts_bearer_and_query_param() {
        let r = redact_secrets("Authorization: Bearer abcdefgh12345678.xyz");
        assert_eq!(r, "Authorization: Bearer [已隐藏]");
        let r = redact_secrets("GET /v1/models?key=abcdefgh12345678&x=1");
        assert_eq!(r, "GET /v1/models?key=[已隐藏]&x=1");
    }

    #[test]
    fn leaves_normal_text_alone() {
        for s in [
            "普通的错误信息：401 Unauthorized",
            "task-abcdefghijklmnopqrstuvwxyz 不是密钥",
            "sk-short",
            "monkey=banana",
            "",
        ] {
            assert_eq!(redact_secrets(s), s);
        }
    }

    #[test]
    fn mask_shows_last_four() {
        assert_eq!(mask_key("sk-1234567890abcd"), "••••abcd");
        assert_eq!(mask_key("abc"), "••••");
    }
}
