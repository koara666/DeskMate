//! 识别聊天回复里的任务提议标记 `[[任务：……]]`，并在流式输出中把它藏起来。
//!
//! 角色在用户同意后会在回复末尾写一行 `[[任务：要做的事]]`，程序据此弹出确认卡片。
//! 标记本身不显示给用户，也不存进聊天记录。模型只能「提议」，真正执行必须用户在卡片上点确认。

const OPEN: &str = "[[任务";
const CLOSE: &str = "]]";
const MAX_TASK_CHARS: usize = 500;

#[derive(Debug, Default)]
pub struct MarkerFilter {
    buf: String,
    inside: bool,
    task: String,
    pub found: Option<String>,
}

fn clean_task(raw: &str) -> String {
    raw.trim_start_matches(['：', ':', ' '])
        .trim()
        .chars()
        .take(MAX_TASK_CHARS)
        .collect()
}

/// `s` 的结尾是不是 OPEN 的一个前缀（还没说完）
fn partial_open_suffix(s: &str) -> usize {
    let open: Vec<char> = OPEN.chars().collect();
    let chars: Vec<char> = s.chars().collect();
    for n in (1..open.len()).rev() {
        if chars.len() >= n && chars[chars.len() - n..] == open[..n] {
            return chars[chars.len() - n..].iter().map(|c| c.len_utf8()).sum();
        }
    }
    0
}

impl MarkerFilter {
    /// 输入一段增量，返回可以显示的文本
    pub fn push(&mut self, chunk: &str) -> String {
        self.buf.push_str(chunk);
        let mut out = String::new();
        loop {
            if self.inside {
                if let Some(i) = self.buf.find(CLOSE) {
                    self.task.push_str(&self.buf[..i]);
                    self.buf.drain(..i + CLOSE.len());
                    self.inside = false;
                    let t = clean_task(&std::mem::take(&mut self.task));
                    if !t.is_empty() {
                        self.found = Some(t);
                    }
                    continue;
                }
                // 还没闭合：保留最后一个字符（可能是 "]" 的一半）
                let keep = if self.buf.ends_with(']') { 1 } else { 0 };
                let take = self.buf.len() - keep;
                self.task.push_str(&self.buf[..take]);
                self.buf.drain(..take);
                if self.task.len() > MAX_TASK_CHARS * 4 {
                    // 异常长：不再当成标记，放弃
                    self.inside = false;
                    self.task.clear();
                }
                return out;
            }
            if let Some(i) = self.buf.find(OPEN) {
                out.push_str(&self.buf[..i]);
                self.buf.drain(..i + OPEN.len());
                self.inside = true;
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
        if self.inside {
            // 模型忘了写 "]]"：把剩下的都当成任务描述
            self.task.push_str(&self.buf);
            self.buf.clear();
            self.inside = false;
            let t = clean_task(self.task.trim_end_matches(']'));
            if !t.is_empty() {
                self.found = Some(t);
            }
            return String::new();
        }
        std::mem::take(&mut self.buf)
    }
}

/// 从完整文本里去掉标记，返回（显示文本，任务）
pub fn strip(text: &str) -> (String, Option<String>) {
    let mut f = MarkerFilter::default();
    let mut out = f.push(text);
    out.push_str(&f.finish());
    (out.trim_end().to_string(), f.found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(chunks: &[&str]) -> (String, Option<String>) {
        let mut f = MarkerFilter::default();
        let mut out = String::new();
        for c in chunks {
            out.push_str(&f.push(c));
        }
        out.push_str(&f.finish());
        (out, f.found)
    }

    #[test]
    fn plain_text_passes() {
        assert_eq!(stream(&["你好", "呀[笑]"]), ("你好呀[笑]".into(), None));
    }

    #[test]
    fn marker_is_hidden_and_extracted() {
        let (out, t) = stream(&[
            "好的，我这就交给执行工具～\n",
            "[[任务：把下载文件夹里的截图移到 截图 文件夹]]",
        ]);
        assert_eq!(out, "好的，我这就交给执行工具～\n");
        assert_eq!(t.as_deref(), Some("把下载文件夹里的截图移到 截图 文件夹"));
    }

    #[test]
    fn marker_split_across_chunks() {
        let (out, t) = stream(&["收到！[", "[任", "务：整理", "桌面]", "]", " 稍等"]);
        assert_eq!(out, "收到！ 稍等");
        assert_eq!(t.as_deref(), Some("整理桌面"));
    }

    #[test]
    fn unclosed_marker_at_end() {
        let (out, t) = stream(&["好的\n[[任务：写一个脚本"]);
        assert_eq!(out, "好的\n");
        assert_eq!(t.as_deref(), Some("写一个脚本"));
    }

    #[test]
    fn strip_whole_text() {
        assert_eq!(
            strip("没问题\n[[任务: 统计字数]]"),
            ("没问题".into(), Some("统计字数".into()))
        );
        assert_eq!(strip("[[x]] 不是任务"), ("[[x]] 不是任务".into(), None));
    }
}
