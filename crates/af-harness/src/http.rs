//! 同步 HTTP。明文密钥只出现在请求头里，错误文本会先抹掉。
//!
//! 客户端不读 `HTTP_PROXY` / `ALL_PROXY`。测试打 `127.0.0.1` 的 stub 时不能被环境代理拐走。

use crate::error::{redact, Error, Result};
use std::time::Duration;

/// 一次 HTTP 响应。非 2xx 也作为 `Ok` 交回调用方，由协议层决定拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// 同步 POST JSON。实现必须可以跨线程共享。
pub trait HttpClient: Send + Sync {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
        secret: &str,
    ) -> Result<HttpResponse>;
}

/// ureq 2 客户端。`tls` 用 rustls，默认特性里的 gzip 关掉。
#[derive(Debug, Clone)]
pub struct UreqClient {
    agent: ureq::Agent,
}

impl UreqClient {
    pub fn new() -> Self {
        // 慢速本地模型（8b 档全思考预算可达分钟级）与慢网供给面都要覆盖；
        // AF_HARNESS_HTTP_TIMEOUT_SEC 可覆盖，默认 600s。
        let timeout_sec = std::env::var("AF_HARNESS_HTTP_TIMEOUT_SEC")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(600);
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(timeout_sec))
            .redirects(0)
            .build();
        Self { agent }
    }
}

impl Default for UreqClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for UreqClient {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
        secret: &str,
    ) -> Result<HttpResponse> {
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        match request.send_string(body) {
            Ok(response) => {
                let status = response.status();
                let text = response
                    .into_string()
                    .map_err(|err| Error::Transport(redact(&err.to_string(), secret)))?;
                Ok(HttpResponse { status, body: text })
            }
            Err(ureq::Error::Status(status, response)) => {
                let text = response.into_string().unwrap_or_else(|err| err.to_string());
                Ok(HttpResponse {
                    status,
                    body: redact(&text, secret),
                })
            }
            Err(ureq::Error::Transport(err)) => {
                Err(Error::Transport(redact(&err.to_string(), secret)))
            }
        }
    }
}
