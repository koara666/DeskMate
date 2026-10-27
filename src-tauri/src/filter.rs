//! 禁词过滤：模型输出在显示前替换掉禁词（提示词里也会要求模型不说，这里是兜底）。
//!
//! 流式输出时一个禁词可能被拆在两段里，所以 [`StreamFilter`] 会扣住
//! 「可能是某个禁词开头」的尾巴，等下一段来了再判断。
//! 英文字母不区分大小写。

/// 替换后的字符
const MASK: char = '*';

#[derive(Debug, Clone, Default)]
pub struct BannedWords {
    /// 按字符数从长到短排序（优先匹配长词）
    words: Vec<Vec<char>>,
}

fn norm(c: char) -> char {
    c.to_ascii_lowercase()
}

impl BannedWords {
    pub fn new<I, S>(words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut v: Vec<Vec<char>> = words
            .into_iter()
            .map(|w| w.as_ref().trim().chars().map(norm).collect::<Vec<char>>())
            .filter(|w| !w.is_empty() && !w.iter().all(|c| *c == MASK))
            .collect();
        v.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        v.dedup();
        Self { words: v }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    fn max_len(&self) -> usize {
        self.words.first().map_or(0, |w| w.len())
    }

    /// 在 `chars[i..]` 开头匹配到的禁词长度
    fn match_at(&self, chars: &[char], i: usize) -> Option<usize> {
        self.words.iter().find_map(|w| {
            let end = i + w.len();
            (end <= chars.len() && chars[i..end].iter().zip(w).all(|(a, b)| norm(*a) == *b))
                .then_some(w.len())
        })
    }

    /// 一次性过滤完整文本
    pub fn apply(&self, text: &str) -> String {
        if self.is_empty() {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            if let Some(n) = self.match_at(&chars, i) {
                out.extend(std::iter::repeat_n(MASK, n));
                i += n;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    /// `tail` 是否是某个禁词的真前缀（还没说完）
    fn is_partial_prefix(&self, tail: &[char]) -> bool {
        self.words
            .iter()
            .any(|w| w.len() > tail.len() && tail.iter().zip(w).all(|(a, b)| norm(*a) == *b))
    }
}

/// 流式过滤器：`push` 每段增量，返回可以安全显示的文本；结束时 `finish` 取回剩余部分。
#[derive(Debug, Clone)]
pub struct StreamFilter {
    words: BannedWords,
    pending: Vec<char>,
}

impl StreamFilter {
    pub fn new(words: BannedWords) -> Self {
        Self {
            words,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &str) -> String {
        if self.words.is_empty() {
            return chunk.to_string();
        }
        self.pending.extend(chunk.chars());
        let chars = std::mem::take(&mut self.pending);
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            // 剩下的部分可能是某个（更长）禁词的开头：先扣住，等下一段再判断
            let rest = &chars[i..];
            if rest.len() < self.words.max_len() && self.words.is_partial_prefix(rest) {
                self.pending = rest.to_vec();
                return out;
            }
            if let Some(n) = self.words.match_at(&chars, i) {
                out.extend(std::iter::repeat_n(MASK, n));
                i += n;
                continue;
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    pub fn finish(&mut self) -> String {
        let rest: String = std::mem::take(&mut self.pending).into_iter().collect();
        self.words.apply(&rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_stream(words: &[&str], chunks: &[&str]) -> String {
        let mut f = StreamFilter::new(BannedWords::new(words));
        let mut out = String::new();
        for c in chunks {
            out.push_str(&f.push(c));
        }
        out.push_str(&f.finish());
        out
    }

    #[test]
    fn replaces_whole_text() {
        let b = BannedWords::new(["笨蛋", "Stupid"]);
        assert_eq!(b.apply("你这个笨蛋！"), "你这个**！");
        assert_eq!(b.apply("so STUPID."), "so ******.");
        assert_eq!(b.apply("没有禁词"), "没有禁词");
    }

    #[test]
    fn empty_list_is_passthrough() {
        let b = BannedWords::new(["", "  ", "***"]);
        assert!(b.is_empty());
        assert_eq!(run_stream(&[], &["a", "b"]), "ab");
    }

    #[test]
    fn longest_match_wins() {
        let b = BannedWords::new(["笨", "笨蛋蛋"]);
        assert_eq!(b.apply("笨蛋蛋笨"), "****");
    }

    #[test]
    fn word_split_across_chunks() {
        assert_eq!(run_stream(&["笨蛋"], &["你这个笨", "蛋！"]), "你这个**！");
        assert_eq!(run_stream(&["abcd"], &["xa", "b", "c", "dy"]), "x****y");
    }

    #[test]
    fn longest_match_wins_across_chunks() {
        assert_eq!(run_stream(&["笨", "笨蛋蛋"], &["笨蛋", "蛋笨"]), "****");
        assert_eq!(run_stream(&["ab", "abcd"], &["ab", "x"]), "**x");
    }

    #[test]
    fn false_prefix_is_released() {
        // 「笨」后面不是「蛋」，应该原样输出
        assert_eq!(run_stream(&["笨蛋"], &["笨", "鸟先飞"]), "笨鸟先飞");
        // 结尾停在前缀上也要原样输出
        assert_eq!(run_stream(&["笨蛋"], &["我好笨"]), "我好笨");
    }

    #[test]
    fn stream_matches_whole_text_result() {
        let words = ["禁词", "坏话", "XYZ"];
        let text = "这里有禁词，还有坏话和xyz，以及禁和词分开的禁 词。";
        let whole = BannedWords::new(words).apply(text);
        for size in 1..6 {
            let chars: Vec<char> = text.chars().collect();
            let chunks: Vec<String> = chars.chunks(size).map(|c| c.iter().collect()).collect();
            let refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
            assert_eq!(run_stream(&words, &refs), whole, "分段大小 {size}");
        }
    }
}
