//! 同步 HTTP。明文密钥只出现在请求头里，错误文本会先抹掉。
//!
//! 客户端不读 `HTTP_PROXY` / `ALL_PROXY`。测试打 `127.0.0.1` 的 stub 时不能被环境代理拐走。
//!
//! 供给面瞬断（429/5xx）在本层指数退避重试，重试耗尽才把非 2xx 交回协议层——
//! 这是可靠性修复：让限流风暴下的任务等到位，而不是直接记 run_failed 浪费轮次。

use crate::error::{redact, Error, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 一次 HTTP 响应。非 2xx 也作为 `Ok` 交回调用方，由协议层决定拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    /// 本次请求在 HTTP 层执行的退避重试次数（不含首次发送）。
    pub retries: u32,
}

/// 退避参数。生产默认见 [`RetryConfig::default`]；测试注入短等待。
#[derive(Debug, Clone)]
pub(crate) struct RetryConfig {
    pub(crate) base_delay: Duration,
    /// 单次等待上限（同时封顶 Retry-After），防止病态头部把任务挂死。
    pub(crate) max_delay: Duration,
    pub(crate) max_retries: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        // 初始 ~5s、×2 递增带抖动；单请求最多重试 5 次（5+10+20+40+80 ≈ 155s 封顶）。
        Self {
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(120),
            max_retries: 5,
        }
    }
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
    retry: RetryConfig,
    /// 本客户端累计退避重试次数。`Opened` 每次打开新建客户端，天然按 Run 分桶。
    retries_done: Arc<AtomicU64>,
}

impl UreqClient {
    pub fn new() -> Self {
        // 慢速本地模型（8b 档全思考预算可达分钟级）与慢网供给面都要覆盖；
        // AF_HARNESS_HTTP_TIMEOUT_SEC 可覆盖，默认 600s。
        let timeout_sec = std::env::var("AF_HARNESS_HTTP_TIMEOUT_SEC")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(600);
        Self::build(
            ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(timeout_sec))
                .redirects(0)
                .build(),
            RetryConfig::default(),
        )
    }

    /// 测试注入短等待的构造口（仅测试构建存在，避免 dead_code 基线噪声）。
    #[cfg(test)]
    pub(crate) fn with_retry(retry: RetryConfig) -> Self {
        Self::build(
            ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(10))
                .redirects(0)
                .build(),
            retry,
        )
    }

    fn build(agent: ureq::Agent, retry: RetryConfig) -> Self {
        Self {
            agent,
            retry,
            retries_done: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 本客户端累计的 HTTP 层退避重试次数。
    pub fn retries_done(&self) -> u64 {
        self.retries_done.load(Ordering::Relaxed)
    }
}

impl Default for UreqClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 429/5xx 是供给面瞬断，值得退避；4xx 其余是请求本身的问题，重试只会烧时间。
fn retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// Retry-After 只认秒数形式（供应商限流均为此形式）；HTTP-date 形式回落指数退避。
fn retry_after(value: &str, cap: Duration) -> Option<Duration> {
    let secs: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(secs.min(cap.as_secs())))
}

/// 指数退避 + 抖动：base × 2^attempt，±20% 由时钟纳秒决定（不引入随机 crate）。
fn backoff_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let exp = attempt.min(16);
    let base = config.base_delay.as_millis().saturating_mul(1u128 << exp);
    let capped = base.min(config.max_delay.as_millis());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.subsec_nanos())
        .unwrap_or(0);
    let frac = f64::from(nanos) / 2f64.powi(32);
    let jittered = capped as f64 * (1.0 + (frac * 0.4 - 0.2));
    Duration::from_millis(jittered.max(1.0) as u64)
}

impl HttpClient for UreqClient {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
        secret: &str,
    ) -> Result<HttpResponse> {
        let mut retries: u32 = 0;
        loop {
            let mut request = self.agent.post(url);
            for (name, value) in headers {
                request = request.set(name, value);
            }
            let (status, text, after) = match request.send_string(body) {
                Ok(response) => {
                    let status = response.status();
                    let text = response
                        .into_string()
                        .map_err(|err| Error::Transport(redact(&err.to_string(), secret)))?;
                    (status, text, None)
                }
                Err(ureq::Error::Status(status, response)) => {
                    // 先读头再消费 body。
                    let after = response
                        .header("retry-after")
                        .and_then(|value| retry_after(value, self.retry.max_delay));
                    let text = response.into_string().unwrap_or_else(|err| err.to_string());
                    (status, text, after)
                }
                Err(ureq::Error::Transport(err)) => {
                    return Err(Error::Transport(redact(&err.to_string(), secret)));
                }
            };
            if !retryable(status) || retries >= self.retry.max_retries {
                return Ok(HttpResponse {
                    status,
                    body: redact(&text, secret),
                    retries,
                });
            }
            let delay = after.unwrap_or_else(|| backoff_delay(&self.retry, retries));
            std::thread::sleep(delay);
            retries += 1;
            self.retries_done.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::{StubResponse, StubServer};

    fn fast_client() -> UreqClient {
        UreqClient::with_retry(RetryConfig {
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
            max_retries: 5,
        })
    }

    #[test]
    fn retries_429_then_succeeds_and_counts() {
        let stub = StubServer::spawn().unwrap();
        let client = fast_client();
        stub.push(StubResponse::json(429, r#"{"error":{"code":"1302"}}"#));
        stub.push(StubResponse::json(200, r#"{"ok":true}"#));
        let response = client.post_json(&stub.url(), &[], "{}", "").unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.retries, 1);
        assert_eq!(client.retries_done(), 1);
        assert_eq!(stub.requests().len(), 2);
    }

    #[test]
    fn retries_503_then_succeeds() {
        let stub = StubServer::spawn().unwrap();
        let client = fast_client();
        stub.push(StubResponse::json(503, "unavailable"));
        stub.push(StubResponse::json(200, r#"{"ok":true}"#));
        let response = client.post_json(&stub.url(), &[], "{}", "").unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.retries, 1);
    }

    #[test]
    fn respects_retry_after_header() {
        let stub = StubServer::spawn().unwrap();
        let client = fast_client();
        stub.push(StubResponse::json(429, "limited").with_retry_after(1));
        stub.push(StubResponse::json(200, r#"{"ok":true}"#));
        let response = client.post_json(&stub.url(), &[], "{}", "").unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.retries, 1);
        assert_eq!(stub.requests().len(), 2);
    }

    #[test]
    fn gives_up_after_max_retries_and_surfaces_status() {
        let stub = StubServer::spawn().unwrap();
        let client = fast_client();
        for _ in 0..6 {
            stub.push(StubResponse::json(429, "busy"));
        }
        let response = client.post_json(&stub.url(), &[], "{}", "").unwrap();
        assert_eq!(response.status, 429);
        assert_eq!(response.retries, 5);
        assert_eq!(stub.requests().len(), 6);
    }

    #[test]
    fn does_not_retry_client_errors() {
        let stub = StubServer::spawn().unwrap();
        let client = fast_client();
        stub.push(StubResponse::json(401, "bad key"));
        let response = client.post_json(&stub.url(), &[], "{}", "").unwrap();
        assert_eq!(response.status, 401);
        assert_eq!(response.retries, 0);
        assert_eq!(stub.requests().len(), 1);
    }

    #[test]
    fn backoff_delay_grows_and_stays_capped() {
        let config = RetryConfig::default();
        let first = backoff_delay(&config, 0);
        let second = backoff_delay(&config, 1);
        assert!(first >= Duration::from_secs(4) && first <= Duration::from_secs(6));
        assert!(second >= Duration::from_secs(8) && second <= Duration::from_secs(12));
        let huge = backoff_delay(&config, 30);
        assert!(huge <= config.max_delay * 12 / 10);
    }
}
