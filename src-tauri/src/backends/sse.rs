//! Server-Sent Events 增量解析器（三家接口的流式输出都是 SSE）。
//! 网络分片可能在任意字节处切开（包括 UTF-8 多字节字符中间），所以按字节缓冲、按行解析。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// `event:` 字段（没有时为 None）
    pub event: Option<String>,
    /// 多行 `data:` 用换行拼接
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

/// 单个事件上限，防止异常数据把内存撑爆
const MAX_LINE: usize = 4 * 1024 * 1024;

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一段字节，返回其中完整的事件
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop(); // \n
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            if let Some(ev) = self.line(&line) {
                out.push(ev);
            }
        }
        if self.buf.len() > MAX_LINE {
            return Err("流式数据中出现超长的一行".into());
        }
        Ok(out)
    }

    /// 流结束时把最后一个没有空行结尾的事件也交出来
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buf.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned();
            let rest = rest.trim_end_matches('\r').to_string();
            if let Some(ev) = self.line(&rest) {
                return Some(ev);
            }
        }
        self.dispatch()
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None; // 注释 / 心跳
        }
        let (field, value) = match line.find(':') {
            Some(i) => {
                let v = &line[i + 1..];
                (&line[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {} // id / retry 用不到
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() && self.event.is_none() {
            return None;
        }
        let ev = SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        };
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut p = SseParser::new();
        let mut v = Vec::new();
        for c in chunks {
            v.extend(p.push(c).unwrap());
        }
        v.extend(p.finish());
        v
    }

    #[test]
    fn basic_events() {
        let v = all(&[b"event: a\ndata: {\"x\":1}\n\ndata: hello\n\n"]);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].event.as_deref(), Some("a"));
        assert_eq!(v[0].data, "{\"x\":1}");
        assert_eq!(v[1].event, None);
        assert_eq!(v[1].data, "hello");
    }

    #[test]
    fn split_anywhere_including_inside_utf8() {
        let raw = "data: 你好世界\r\n\r\ndata: [DONE]\n\n".as_bytes();
        for cut in 1..raw.len() {
            let v = all(&[&raw[..cut], &raw[cut..]]);
            assert_eq!(v.len(), 2, "切在 {cut}");
            assert_eq!(v[0].data, "你好世界");
            assert_eq!(v[1].data, "[DONE]");
        }
    }

    #[test]
    fn comments_and_multiline_data() {
        let v = all(&[b": ping\n\ndata: a\ndata: b\n\n"]);
        assert_eq!(
            v,
            vec![SseEvent {
                event: None,
                data: "a\nb".into()
            }]
        );
    }

    #[test]
    fn last_event_without_blank_line() {
        let v = all(&[b"data: tail"]);
        assert_eq!(v[0].data, "tail");
    }
}
