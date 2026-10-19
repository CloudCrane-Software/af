//! 测试用的 ClickHouse HTTP 替身。校验 RowBinary 批与失败重试，不连真实库。
//!
//! 只讲官方客户端实际用的那一小段 HTTP/1.1：
//! 插入是 `POST /?query=INSERT … FORMAT RowBinary&decompress=1`，
//! body 是 lz4 帧里的 RowBinary；DDL / SELECT 是 body 里的 SQL 文本。
//! 不连真实 ClickHouse。

use crate::event::MesEvent;
use crate::rowbin::{self, decode_events, decompress_ch_lz4};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// 一次插入请求解出来的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedInsert {
    pub query: String,
    pub decompress_flag: bool,
    pub lz4_framed: bool,
    pub accepted: bool,
    pub rows: Vec<MesEvent>,
    pub raw_rowbinary: Vec<u8>,
}

struct State {
    fail_inserts: usize,
    inserts: Vec<CapturedInsert>,
    queries: Vec<String>,
    scripted_bodies: VecDeque<Vec<u8>>,
    parse_error: Option<String>,
}

/// 绑定在 `127.0.0.1` 的临时端口上。
pub struct MockClickHouse {
    url: String,
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    state: Arc<Mutex<State>>,
}

impl MockClickHouse {
    pub fn start() -> Self {
        let listener =
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("bind mock clickhouse");
        let addr = listener.local_addr().expect("mock local addr");
        let shutdown = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(State {
            fail_inserts: 0,
            inserts: Vec::new(),
            queries: Vec::new(),
            scripted_bodies: VecDeque::new(),
            parse_error: None,
        }));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_state = Arc::clone(&state);
        let join = thread::Builder::new()
            .name("af-mes-mock".into())
            .spawn(move || serve(listener, thread_shutdown, thread_state))
            .expect("spawn mock clickhouse");
        Self {
            url: format!("http://{addr}"),
            addr,
            shutdown,
            join: Some(join),
            state,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// 接下来 n 次插入返回 500。字节仍会解码并记下来。
    pub fn fail_next_inserts(&self, n: usize) {
        self.state.lock().expect("mock state").fail_inserts = n;
    }

    /// 下一次非插入请求的响应体。不设则空 body（0 行 / DDL 成功）。
    pub fn enqueue_query_body(&self, body: Vec<u8>) {
        self.state
            .lock()
            .expect("mock state")
            .scripted_bodies
            .push_back(body);
    }

    pub fn inserts(&self) -> Vec<CapturedInsert> {
        let state = self.state.lock().expect("mock state");
        if let Some(err) = &state.parse_error {
            panic!("mock 没能解析插入: {err}");
        }
        state.inserts.clone()
    }

    pub fn queries(&self) -> Vec<String> {
        self.state.lock().expect("mock state").queries.clone()
    }
}

impl Drop for MockClickHouse {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(join) = self.join.take() {
            if thread::panicking() {
                return;
            }
            let _ = join.join();
        }
    }
}

fn serve(listener: TcpListener, shutdown: Arc<AtomicBool>, state: Arc<Mutex<State>>) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(err) = handle_one(stream, &state) {
                    let mut guard = state.lock().expect("mock state");
                    guard.parse_error.get_or_insert(err);
                }
            }
            Err(_) => break,
        }
    }
}

fn handle_one(stream: TcpStream, state: &Mutex<State>) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream);
    let Some((request_line, headers)) = read_headers(&mut reader)? else {
        return Ok(());
    };
    let body = read_body(&mut reader, &headers)?;
    let response = dispatch(&request_line, &body, state);
    let stream = reader.get_mut();
    stream.write_all(&response).map_err(|err| err.to_string())?;
    stream.flush().map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch(request_line: &str, body: &[u8], state: &Mutex<State>) -> Vec<u8> {
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let params = query_params(target);
    let query = params.get("query").cloned().unwrap_or_default();
    let is_insert = query
        .trim_start()
        .get(..6)
        .is_some_and(|head| head.eq_ignore_ascii_case("INSERT"));
    if is_insert {
        return dispatch_insert(
            &query,
            params.get("decompress").map(String::as_str),
            body,
            state,
        );
    }
    let sql = String::from_utf8_lossy(body).into_owned();
    let mut guard = state.lock().expect("mock state");
    guard.queries.push(sql);
    let scripted = guard.scripted_bodies.pop_front().unwrap_or_default();
    drop(guard);
    http_response(200, scripted)
}

fn dispatch_insert(
    query: &str,
    decompress: Option<&str>,
    body: &[u8],
    state: &Mutex<State>,
) -> Vec<u8> {
    let decompress_flag = decompress == Some("1");
    let lz4_framed = rowbin::frame_is_lz4(body);
    let decoded = if decompress_flag {
        decompress_ch_lz4(body).and_then(|raw| decode_events(&raw).map(|rows| (raw, rows)))
    } else {
        decode_events(body).map(|rows| (body.to_vec(), rows))
    };
    let (raw_rowbinary, rows) = match decoded {
        Ok(pair) => pair,
        Err(err) => {
            state.lock().expect("mock state").parse_error = Some(err.clone());
            return http_response(500, err.into_bytes());
        }
    };
    let mut guard = state.lock().expect("mock state");
    let accepted = if guard.fail_inserts > 0 {
        guard.fail_inserts -= 1;
        false
    } else {
        true
    };
    guard.inserts.push(CapturedInsert {
        query: query.to_string(),
        decompress_flag,
        lz4_framed,
        accepted,
        rows,
        raw_rowbinary,
    });
    drop(guard);
    if accepted {
        http_response(200, Vec::new())
    } else {
        http_response(500, b"mock insert failure".to_vec())
    }
}

fn http_response(status: u16, body: Vec<u8>) -> Vec<u8> {
    let reason = if status == 200 { "OK" } else { "Error" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(&body);
    out
}

type HeaderMap = HashMap<String, String>;

fn read_headers(reader: &mut BufReader<TcpStream>) -> Result<Option<(String, HeaderMap)>, String> {
    let mut request_line = String::new();
    let n = reader
        .read_line(&mut request_line)
        .map_err(|err| err.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    let mut headers = HeaderMap::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|err| err.to_string())?;
        if n == 0 {
            return Err("请求头被截断".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(Some((request_line, headers)))
}

fn read_body(reader: &mut BufReader<TcpStream>, headers: &HeaderMap) -> Result<Vec<u8>, String> {
    const MAX_BODY: usize = 64 * 1024 * 1024;
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    {
        return read_chunked(reader, MAX_BODY);
    }
    let length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|err| format!("Content-Length: {err}"))?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err(format!("body {length} 超过 {MAX_BODY}"));
    }
    let mut buf = vec![0u8; length];
    reader
        .read_exact(&mut buf)
        .map_err(|err| format!("读 body: {err}"))?;
    Ok(buf)
}

fn read_chunked(reader: &mut BufReader<TcpStream>, max_body: usize) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|err| format!("读 chunk 长度: {err}"))?;
        let size_hex = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|err| format!("chunk 长度 {size_hex:?}: {err}"))?;
        if size == 0 {
            loop {
                let mut trailer = String::new();
                reader
                    .read_line(&mut trailer)
                    .map_err(|err| format!("读 chunk trailer: {err}"))?;
                if trailer == "\r\n" || trailer == "\n" || trailer.is_empty() {
                    break;
                }
            }
            break;
        }
        if body.len().saturating_add(size) > max_body {
            return Err("chunked body 超过上限".into());
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|err| format!("读 chunk: {err}"))?;
        body.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader
            .read_exact(&mut crlf)
            .map_err(|err| format!("读 chunk CRLF: {err}"))?;
        if &crlf != b"\r\n" {
            return Err("chunk 结尾不是 CRLF".into());
        }
    }
    Ok(body)
}

fn query_params(target: &str) -> HashMap<String, String> {
    let Some(query) = target.split_once('?').map(|(_, query)| query) else {
        return HashMap::new();
    };
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(name), percent_decode(value));
    }
    params
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &input[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(b'%');
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
