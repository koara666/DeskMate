//! 各后端共用的 HTTP 工具：发请求、把错误归类并脱敏、可取消地读取 SSE 流。
//! 规则：请求头（含 Key）和请求体永远不写日志。

use super::BackendError;
use super::sse::{SseEvent, SseParser};
use crate::redact::redact_secrets;
use reqwest::header::HeaderMap;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// 共用的 HTTP 客户端（连接池复用）。会读取系统代理设置。
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // 流式输出时两段数据之间最多等这么久
        .read_timeout(Duration::from_secs(90))
        .user_agent(concat!("DeskMate/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn map_reqwest_error(e: reqwest::Error) -> BackendError {
    if e.is_timeout() {
        BackendError::Timeout
    } else {
        BackendError::Network(redact_secrets(&e.to_string()))
    }
}

/// 从错误响应体里取出可读的信息（各家都是 `{"error": {"message": ...}}` 的形式），截断并脱敏
pub fn error_message(body: &str) -> String {
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            let e = v.get("error")?;
            e.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .or_else(|| e.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.trim().to_string());
    let short: String = msg.chars().take(300).collect();
    redact_secrets(&short)
}

/// 发送 POST 请求；非 2xx 时读取错误信息并归类。`classify` 让各后端细化归类。
pub async fn post_stream(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    body: &serde_json::Value,
    cancel: &CancellationToken,
    classify: fn(u16, String) -> BackendError,
) -> Result<Option<reqwest::Response>, BackendError> {
    let send = client.post(url).headers(headers).json(body).send();
    let resp = tokio::select! {
        _ = cancel.cancelled() => return Ok(None),
        r = send => r.map_err(map_reqwest_error)?,
    };
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let text = tokio::select! {
            _ = cancel.cancelled() => return Ok(None),
            t = resp.text() => t.unwrap_or_default(),
        };
        let msg = error_message(&text);
        log::warn!("模型接口返回 {status}");
        return Err(classify(status, format!("{status} {msg}")));
    }
    Ok(Some(resp))
}

/// 可取消地逐个读取 SSE 事件。`on_event` 返回 `Ok(false)` 表示可以结束了。
/// 返回值：是否被取消。
pub async fn read_sse(
    mut resp: reqwest::Response,
    cancel: &CancellationToken,
    mut on_event: impl FnMut(SseEvent) -> Result<bool, BackendError>,
) -> Result<bool, BackendError> {
    let mut parser = SseParser::new();
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Ok(true),
            c = resp.chunk() => c.map_err(map_reqwest_error)?,
        };
        let Some(bytes) = chunk else { break };
        for ev in parser.push(&bytes).map_err(BackendError::Protocol)? {
            if !on_event(ev)? {
                return Ok(false);
            }
        }
    }
    if let Some(ev) = parser.finish() {
        on_event(ev)?;
    }
    Ok(false)
}

/// 测试用：起一个本地假服务器，按顺序返回预设的响应（不联网、不花钱）
#[cfg(test)]
pub mod fake_server {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub struct Captured {
        pub head: String,
        pub body: String,
    }

    /// 返回 (base_url, 收到的请求)。`chunks` 会分多次写出，模拟流式。
    pub async fn serve(
        status: u16,
        content_type: &'static str,
        chunks: Vec<String>,
    ) -> (String, tokio::sync::oneshot::Receiver<Captured>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            // 读完请求头和 Content-Length 指定的请求体
            let (head, body) = loop {
                let n = sock.read(&mut tmp).await.unwrap();
                buf.extend_from_slice(&tmp[..n]);
                let s = String::from_utf8_lossy(&buf).to_string();
                if let Some(i) = s.find("\r\n\r\n") {
                    let head = s[..i].to_string();
                    let len = head
                        .lines()
                        .find_map(|l| {
                            let l = l.to_ascii_lowercase();
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if buf.len() >= i + 4 + len {
                        break (
                            head,
                            String::from_utf8_lossy(&buf[i + 4..i + 4 + len]).to_string(),
                        );
                    }
                }
                if n == 0 {
                    break (s, String::new());
                }
            };
            let _ = tx.send(Captured { head, body });
            let reason = if status == 200 { "OK" } else { "ERR" };
            let header = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            );
            sock.write_all(header.as_bytes()).await.unwrap();
            for c in chunks {
                let part = format!("{:X}\r\n{}\r\n", c.len(), c);
                sock.write_all(part.as_bytes()).await.unwrap();
                sock.flush().await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            sock.write_all(b"0\r\n\r\n").await.unwrap();
        });
        (format!("http://127.0.0.1:{}", addr.port()), rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_message_extracts_and_redacts() {
        let k = ["sk-", "abcdefghij0123456789XYZ"].concat();
        let body =
            format!(r#"{{"error":{{"message":"Incorrect API key provided: {k}","type":"auth"}}}}"#);
        let m = error_message(&body);
        assert!(m.starts_with("Incorrect API key"));
        assert!(!m.contains("abcdefghij0123456789"));
        assert_eq!(error_message("plain text"), "plain text");
        assert_eq!(error_message(&"x".repeat(1000)).len(), 300);
    }
}
