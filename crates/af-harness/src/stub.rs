//! 本 crate 的 HTTP/1.1 stub。只听 `127.0.0.1`，按队列回放响应。
//!
//! 测试用它代替供应商。每连接一线程（stall 响应按住一条连接时不得堵住其他连接）；
//! `Drop` 先置关闭标志，再连接自己把 `accept` 唤醒。

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// 一条预设响应。
#[derive(Debug, Clone)]
pub struct StubResponse {
    pub status: u16,
    pub content_type: String,
    pub body: String,
    /// 限流测试用：响应携带 `Retry-After` 头（秒）。
    pub retry_after: Option<u64>,
    /// 读超时测试用：先按住连接不回任何字节（秒），模拟服务端读挂起。
    pub stall_secs: Option<u64>,
}

impl StubResponse {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "application/json".into(),
            body: body.into(),
            retry_after: None,
            stall_secs: None,
        }
    }

    pub fn sse(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream".into(),
            body: body.into(),
            retry_after: None,
            stall_secs: None,
        }
    }

    /// 一条按住连接不回字节的响应（读超时重试测试用）。
    pub fn stalled(status: u16) -> Self {
        Self {
            status,
            content_type: "application/json".into(),
            body: String::new(),
            retry_after: None,
            stall_secs: Some(1),
        }
    }

    pub fn with_retry_after(mut self, secs: u64) -> Self {
        self.retry_after = Some(secs);
        self
    }

    pub fn with_stall(mut self, secs: u64) -> Self {
        self.stall_secs = Some(secs);
        self
    }
}

/// stub 收到的请求。测试用来核对路径、头和正文。头名已转成小写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// 本地回放服务。
pub struct StubServer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    script: Arc<Mutex<VecDeque<StubResponse>>>,
    join: Option<JoinHandle<()>>,
}

impl StubServer {
    pub fn spawn() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(VecDeque::new()));
        let flag = Arc::clone(&shutdown);
        let recorded = Arc::clone(&requests);
        let queued = Arc::clone(&script);
        let join = thread::spawn(move || serve(listener, flag, recorded, queued));
        Ok(Self {
            addr,
            shutdown,
            requests,
            script,
            join: Some(join),
        })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn push(&self, response: StubResponse) {
        self.script
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push_back(response);
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }
}

impl Drop for StubServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn serve(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    script: Arc<Mutex<VecDeque<StubResponse>>>,
) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok((stream, _)) = listener.accept() else {
            break;
        };
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let flag = Arc::clone(&shutdown);
        let recorded = Arc::clone(&requests);
        let queued = Arc::clone(&script);
        // 每连接一线程：stall 响应按住一条连接时不得堵住后续连接的回放
        // （读超时重试测试的第二次请求要能立即被服务）。
        thread::spawn(move || handle_conn(stream, flag, recorded, queued));
    }
}

fn handle_conn(
    mut stream: TcpStream,
    shutdown: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    script: Arc<Mutex<VecDeque<StubResponse>>>,
) {
    if shutdown.load(Ordering::SeqCst) {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let Ok(Some(raw)) = read_request(&mut stream) else {
        return;
    };
    requests
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .push(parse_request(&raw));
    let response = script
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .pop_front()
        .unwrap_or_else(|| StubResponse::json(500, r#"{"error":"stub script exhausted"}"#));
    if let Some(secs) = response.stall_secs {
        // 按住连接不吐字节：客户端（配置了整体超时）会在这里撞上读超时。
        thread::sleep(Duration::from_secs(secs));
    }
    let _ = write_response(&mut stream, &response);
}

struct RawRequest {
    head: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<RawRequest>> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "header truncated",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_separator(&buf) {
            break pos;
        }
        if buf.len() > 1_048_576 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "header too large",
            ));
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut body = buf[header_end + 4..].to_vec();
    let need = content_length(&head).unwrap_or(0);
    while body.len() < need {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(need);
    Ok(Some(RawRequest { head, body }))
}

fn find_separator(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(head: &str) -> Option<usize> {
    for line in head.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn parse_request(raw: &RawRequest) -> RecordedRequest {
    let mut lines = raw.head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    RecordedRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&raw.body).into_owned(),
    }
}

fn write_response(stream: &mut TcpStream, response: &StubResponse) -> std::io::Result<()> {
    let bytes = response.body.as_bytes();
    let retry_after = response
        .retry_after
        .map(|secs| format!("Retry-After: {secs}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        retry_after,
        bytes.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_length_and_lowercases_headers() {
        let raw = RawRequest {
            head: "POST /v1/chat HTTP/1.1\r\nAuthorization: Bearer secret\r\nContent-Length: 2\r\n"
                .into(),
            body: b"{}".to_vec(),
        };
        let recorded = parse_request(&raw);
        assert_eq!(recorded.method, "POST");
        assert_eq!(recorded.path, "/v1/chat");
        assert_eq!(recorded.body, "{}");
        assert_eq!(content_length(&raw.head), Some(2));
        assert!(recorded
            .headers
            .iter()
            .any(|(name, _)| name == "authorization"));
    }
}
