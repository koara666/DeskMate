//! 识别聊天回复里的动作标记，并在流式输出中把它们藏起来。
//!
//! - `[[打开：路径或网址]]`：内置能力层，气泡里出现「打开」按钮，用户点了才打开（`abilities.rs`）。
//! - `[[读取：文件路径]]`：内置能力层，用户点了才读（只能读设置里允许的文件夹）。
//! - `[[任务：要做的事]]`：委托外部 agent（高级选项，默认关闭，DECISIONS #33），弹出任务确认卡片。
//! - `[[玩：游戏名]]`：用户想玩游戏时，直接打开那个游戏（v2.1，DECISIONS #49）。
//! - `[[提醒：一句吩咐]]`：用户要她记着某个时间提醒，本地解析后在气泡里确认（v2.1）。
//! - `[[日语：……]]`：开着日语语音时，回复开头一两句的日语版本，只用来念（DECISIONS #58）。
//! - `[[搜索：关键词]]`：她觉得要上网查才答得上来时，整条回复只写这个；程序搜完再让她回答（DECISIONS #65）。
//!
//! 标记本身不显示给用户，也不存进聊天记录。模型只能「提议」，真正动手必须用户点确认。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Task,
    Open,
    Read,
    Game,
    Remind,
    Ja,
    Search,
}

const TAGS: [(&str, MarkerKind); 7] = [
    ("[[任务", MarkerKind::Task),
    ("[[打开", MarkerKind::Open),
    ("[[读取", MarkerKind::Read),
    ("[[玩", MarkerKind::Game),
    ("[[提醒", MarkerKind::Remind),
    ("[[日语", MarkerKind::Ja),
    ("[[搜索", MarkerKind::Search),
];
const CLOSE: &str = "]]";
const MAX_MARKER_CHARS: usize = 500;
/// 一条回复里最多认几个标记（多出来的丢掉）
const MAX_MARKERS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Marker {
    pub kind: MarkerKind,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct MarkerFilter {
    buf: String,
    inside: Option<MarkerKind>,
    cur: String,
    pub found: Vec<Marker>,
}

fn clean(raw: &str) -> String {
    raw.trim_start_matches(['：', ':', ' '])
        .trim()
        .chars()
        .take(MAX_MARKER_CHARS)
        .collect()
}

/// `s` 的结尾是不是某个标记开头的前缀（还没说完），返回要先扣住的字节数
fn partial_open_suffix(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut best = 0;
    for (open, _) in TAGS {
        let open: Vec<char> = open.chars().collect();
        for n in (1..open.len()).rev() {
            if chars.len() >= n && chars[chars.len() - n..] == open[..n] {
                let bytes = chars[chars.len() - n..].iter().map(|c| c.len_utf8()).sum();
                best = best.max(bytes);
                break;
            }
        }
    }
    best
}

/// 最早出现的标记开头：（位置，标记种类，开头长度）
fn find_open(s: &str) -> Option<(usize, MarkerKind, usize)> {
    TAGS.iter()
        .filter_map(|(open, kind)| s.find(open).map(|i| (i, *kind, open.len())))
        .min_by_key(|x| x.0)
}

impl MarkerFilter {
    fn close_marker(&mut self, kind: MarkerKind) {
        let t = clean(&std::mem::take(&mut self.cur));
        // [[玩：]]（没说玩什么）也算：打开游戏列表
        if (!t.is_empty() || kind == MarkerKind::Game) && self.found.len() < MAX_MARKERS {
            self.found.push(Marker { kind, text: t });
        }
    }

    /// 输入一段增量，返回可以显示的文本
    pub fn push(&mut self, chunk: &str) -> String {
        self.buf.push_str(chunk);
        let mut out = String::new();
        loop {
            if let Some(kind) = self.inside {
                if let Some(i) = self.buf.find(CLOSE) {
                    self.cur.push_str(&self.buf[..i]);
                    self.buf.drain(..i + CLOSE.len());
                    self.inside = None;
                    self.close_marker(kind);
                    continue;
                }
                // 还没闭合：保留最后一个字符（可能是 "]" 的一半）
                let keep = if self.buf.ends_with(']') { 1 } else { 0 };
                let take = self.buf.len() - keep;
                self.cur.push_str(&self.buf[..take]);
                self.buf.drain(..take);
                if self.cur.len() > MAX_MARKER_CHARS * 4 {
                    // 异常长：不再当成标记，放弃
                    self.inside = None;
                    self.cur.clear();
                }
                return out;
            }
            if let Some((i, kind, len)) = find_open(&self.buf) {
                out.push_str(&self.buf[..i]);
                self.buf.drain(..i + len);
                self.inside = Some(kind);
                continue;
            }
            let hold = partial_open_suffix(&self.buf);
            let emit = self.buf.len() - hold;
            out.push_str(&self.buf[..emit]);
            self.buf.drain(..emit);
            return out;
        }
    }

    /// 结束：返回剩余可显示文本
    pub fn finish(&mut self) -> String {
        if let Some(kind) = self.inside.take() {
            // 模型忘了写 "]]"：把剩下的都当成标记内容
            self.cur.push_str(&self.buf);
            self.buf.clear();
            let raw = std::mem::take(&mut self.cur);
            self.cur = raw.trim_end_matches(']').to_string();
            self.close_marker(kind);
            return String::new();
        }
        std::mem::take(&mut self.buf)
    }
}

/// 从完整文本里去掉标记，返回（显示文本，标记列表）
pub fn strip(text: &str) -> (String, Vec<Marker>) {
    let mut f = MarkerFilter::default();
    let mut out = f.push(text);
    out.push_str(&f.finish());
    // 日语标记写在回复最前面（DECISIONS #65），它后面的换行不留
    (out.trim().to_string(), f.found)
}

/// 标记列表里的日语版本（念给用户听的）
pub fn ja_text(markers: &[Marker]) -> Option<&str> {
    markers
        .iter()
        .find(|m| m.kind == MarkerKind::Ja && !m.text.is_empty())
        .map(|m| m.text.as_str())
}

/// 标记列表里的联网搜索请求（关键词）
pub fn search_query(markers: &[Marker]) -> Option<&str> {
    markers
        .iter()
        .find(|m| m.kind == MarkerKind::Search && !m.text.is_empty())
        .map(|m| m.text.as_str())
}

/// 标记列表里的第一个任务提议
pub fn first_task(markers: &[Marker]) -> Option<String> {
    markers
        .iter()
        .find(|m| m.kind == MarkerKind::Task)
        .map(|m| m.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(chunks: &[&str]) -> (String, Vec<Marker>) {
        let mut f = MarkerFilter::default();
        let mut out = String::new();
        for c in chunks {
            out.push_str(&f.push(c));
        }
        out.push_str(&f.finish());
        (out, f.found)
    }

    fn task(t: &str) -> Marker {
        Marker {
            kind: MarkerKind::Task,
            text: t.into(),
        }
    }

    #[test]
    fn plain_text_passes() {
        assert_eq!(stream(&["你好", "呀[笑]"]), ("你好呀[笑]".into(), vec![]));
        assert_eq!(stream(&["[[", "好]]"]), ("[[好]]".into(), vec![]));
    }

    #[test]
    fn marker_is_hidden_and_extracted() {
        let (out, m) = stream(&[
            "好的，我这就交给执行工具～\n",
            "[[任务：把下载文件夹里的截图移到 截图 文件夹]]",
        ]);
        assert_eq!(out, "好的，我这就交给执行工具～\n");
        assert_eq!(m, vec![task("把下载文件夹里的截图移到 截图 文件夹")]);
    }

    #[test]
    fn marker_split_across_chunks() {
        let (out, m) = stream(&["收到！[", "[任", "务：整理", "桌面]", "]", " 稍等"]);
        assert_eq!(out, "收到！ 稍等");
        assert_eq!(m, vec![task("整理桌面")]);
    }

    #[test]
    fn open_and_read_markers() {
        let (out, m) = stream(&[
            "好，给你打开～[[打",
            "开：https://example.com]]\n还有这个[[读取：D:\\笔记\\a.txt]]",
        ]);
        assert_eq!(out, "好，给你打开～\n还有这个");
        assert_eq!(
            m,
            vec![
                Marker {
                    kind: MarkerKind::Open,
                    text: "https://example.com".into()
                },
                Marker {
                    kind: MarkerKind::Read,
                    text: "D:\\笔记\\a.txt".into()
                },
            ]
        );
        assert_eq!(first_task(&m), None);
    }

    #[test]
    fn game_and_reminder_markers() {
        let (out, m) = stream(&["好呀！[[玩", "：井字棋]]"]);
        assert_eq!(out, "好呀！");
        assert_eq!(m[0].kind, MarkerKind::Game);
        assert_eq!(m[0].text, "井字棋");
        let (out, m) = strip("记下啦\n[[提醒：明天下午3点提醒我开会]]");
        assert_eq!(out, "记下啦");
        assert_eq!(m[0].kind, MarkerKind::Remind);
        let (_, m) = strip("来吧[[玩：]]");
        assert_eq!((m[0].kind, m[0].text.as_str()), (MarkerKind::Game, ""));
        assert_eq!(strip("[[好]]").1, vec![]);
    }

    #[test]
    fn search_marker_is_hidden_and_readable() {
        let (out, m) = stream(&["[[搜索：抚顺 ", "天气]]"]);
        assert_eq!(out, "");
        assert_eq!(search_query(&m), Some("抚顺 天气"));
        let (out, m) = stream(&["我查查～[[搜索："]);
        assert_eq!(out, "我查查～");
        assert_eq!(search_query(&m), None, "没写完的不算");
    }

    #[test]
    fn japanese_marker_is_hidden() {
        let (out, m) = stream(&["好呀，一起玩吧。\n[[日", "语：うん、一緒に遊ぼう。]]"]);
        assert_eq!(out.trim_end(), "好呀，一起玩吧。");
        assert_eq!(ja_text(&m), Some("うん、一緒に遊ぼう。"));
        assert_eq!(ja_text(&strip("没有日语").1), None);
    }

    #[test]
    fn at_most_three_markers() {
        let (_, m) = strip("[[打开：a]][[打开：b]][[打开：c]][[打开：d]]");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn unclosed_marker_at_end() {
        let (out, m) = stream(&["好的\n[[任务：写一个脚本"]);
        assert_eq!(out, "好的\n");
        assert_eq!(m, vec![task("写一个脚本")]);
    }

    #[test]
    fn strip_whole_text() {
        let (out, m) = strip("没问题\n[[任务: 统计字数]]");
        assert_eq!(out, "没问题");
        assert_eq!(first_task(&m).as_deref(), Some("统计字数"));
        assert_eq!(strip("[[x]] 不是任务"), ("[[x]] 不是任务".into(), vec![]));
    }
}
