//! 最小 HTTP/1.1 客户端。只发 JSON POST，供 rekor-tiles 提交使用。
//!
//! 只接受 `http://`。连接、读、写都有调用方给出的超时，避免不可达端点把锚线程挂死。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const MAX_BODY: usize = 1024 * 1024;

pub(crate) struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub(crate) fn post_json(url: &str, body: &[u8], timeout: Duration) -> Result<HttpResponse, String> {
    let target = parse_http_url(url)?;
    let addr = resolve(&target)?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|err| format!("连接 {} 失败: {err}", target.host_port()))?;
    stream
        .set_nodelay(true)
        .map_err(|err| format!("设置 TCP_NODELAY 失败: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("设置读超时失败: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("设置写超时失败: {err}"))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        path = target.path,
        host = target.host_header(),
        len = body.len(),
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("写入请求失败: {err}"))?;
    stream
        .write_all(body)
        .map_err(|err| format!("写入请求体失败: {err}"))?;
    stream
        .flush()
        .map_err(|err| format!("刷新请求失败: {err}"))?;

    let raw = read_limited(&mut stream, MAX_BODY)?;
    parse_response(&raw)
}

struct HttpUrl {
    host: String,
    port: u16,
    path: String,
}

impl HttpUrl {
    fn host_port(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            self.host_port()
        }
    }
}

fn parse_http_url(url: &str) -> Result<HttpUrl, String> {
    let rest = url
        .trim()
        .strip_prefix("http://")
        .ok_or_else(|| format!("只支持 http:// 端点，实际 {url}"))?;
    if rest.is_empty() {
        return Err("端点缺少主机".into());
    }
    let (hostport, path) = match rest.split_once('/') {
        Some((hostport, path)) => (hostport, format!("/{path}")),
        None => (rest, "/api/v2/log/entries".to_string()),
    };
    let path = if path == "/" {
        "/api/v2/log/entries".to_string()
    } else {
        path
    };
    let (host, port) = split_host_port(hostport)?;
    if host.is_empty() {
        return Err("端点主机为空".into());
    }
    Ok(HttpUrl { host, port, path })
}

fn split_host_port(hostport: &str) -> Result<(String, u16), String> {
    if let Some(host) = hostport.strip_prefix('[') {
        let (host, rest) = host
            .split_once(']')
            .ok_or_else(|| format!("IPv6 主机缺少括号: {hostport}"))?;
        let port = match rest.strip_prefix(':') {
            Some(port) => parse_port(port)?,
            None if rest.is_empty() => 80,
            _ => return Err(format!("IPv6 主机格式不对: {hostport}")),
        };
        return Ok((host.to_string(), port));
    }
    match hostport.rsplit_once(':') {
        Some((host, port)) => Ok((host.to_string(), parse_port(port)?)),
        None => Ok((hostport.to_string(), 80)),
    }
}

fn parse_port(text: &str) -> Result<u16, String> {
    text.parse::<u16>()
        .map_err(|_| format!("端口不是 u16: {text}"))
}

fn resolve(target: &HttpUrl) -> Result<SocketAddr, String> {
    let mut addrs = target
        .host_port()
        .to_socket_addrs()
        .map_err(|err| format!("解析 {} 失败: {err}", target.host_port()))?;
    addrs
        .next()
        .ok_or_else(|| format!("解析 {} 没有得到地址", target.host_port()))
}

fn read_limited(stream: &mut TcpStream, limit: usize) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        if response_complete(&buf).is_some() {
            break;
        }
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len().saturating_add(n) > limit {
                    return Err("响应超过 1MiB".into());
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => {
                if buf.is_empty() {
                    return Err(format!("读取响应失败: {err}"));
                }
                return Err(format!("响应未完成: {err}"));
            }
        }
    }
    Ok(buf)
}

fn response_complete(buf: &[u8]) -> Option<usize> {
    let header_end = find_header_end(buf)?;
    let headers = std::str::from_utf8(&buf[..header_end]).ok()?;
    let length = content_length(headers)?;
    let total = header_end.checked_add(4)?.checked_add(length)?;
    if buf.len() >= total {
        Some(total)
    } else {
        None
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn parse_response(raw: &[u8]) -> Result<HttpResponse, String> {
    let header_end = find_header_end(raw).ok_or("响应没有头部结束")?;
    let head = std::str::from_utf8(&raw[..header_end])
        .map_err(|err| format!("响应头不是 UTF-8: {err}"))?;
    let status_line = head.lines().next().ok_or("响应没有状态行")?;
    let mut parts = status_line.split_whitespace();
    let version = parts.next().ok_or("状态行缺少版本")?;
    if !version.starts_with("HTTP/1.") {
        return Err(format!("不是 HTTP/1 响应: {status_line}"));
    }
    let status = parts
        .next()
        .ok_or("状态行缺少状态码")?
        .parse::<u16>()
        .map_err(|_| format!("状态码无法解析: {status_line}"))?;
    let length = content_length(head).ok_or("响应缺少 Content-Length")?;
    let body_at = header_end + 4;
    let body_end = body_at.checked_add(length).ok_or("Content-Length 溢出")?;
    if raw.len() < body_end {
        return Err(format!(
            "响应体短于 Content-Length: 有 {} 需要 {length}",
            raw.len().saturating_sub(body_at)
        ));
    }
    Ok(HttpResponse {
        status,
        body: raw[body_at..body_end].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_appends_the_entries_path() {
        let url = parse_http_url("http://127.0.0.1:3003").unwrap();
        assert_eq!(url.host, "127.0.0.1");
        assert_eq!(url.port, 3003);
        assert_eq!(url.path, "/api/v2/log/entries");
        assert_eq!(url.host_header(), "127.0.0.1:3003");

        let full = parse_http_url("http://127.0.0.1:3003/api/v2/log/entries").unwrap();
        assert_eq!(full.path, "/api/v2/log/entries");
        assert!(parse_http_url("https://127.0.0.1:3003").is_err());
        assert!(parse_http_url("http://").is_err());
    }

    #[test]
    fn parse_response_reads_content_length() {
        let raw = b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        let parsed = parse_response(raw).unwrap();
        assert_eq!(parsed.status, 201);
        assert_eq!(parsed.body, b"{}");
        assert!(parse_response(b"nope").is_err());
    }
}
