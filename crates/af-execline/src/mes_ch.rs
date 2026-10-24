//! ClickHouse 下游 sink（消债 B②③：MES 写入上移驱动缝 + 读侧对账幂等）。
//!
//! 补丁台账-第二阶段 #B 的 `af-order` 文件 stub 直写 CH 逻辑（marker 差量）
//! 在此升格为缝的 CH 实现：
//!
//! - **目标声明**（订单面只留声明，不做写侧）：[`MesChTarget`]——`auto`
//!   （env `MES_CH_URL` > 0600 `~/.config/mes-ch/url`，与 tools/bench-mes.py
//!   同约定）或显式 URL（可含 `user:pass@host`，拆出转官方客户端认证头，
//!   凭据零落日志/回执）。
//! - **读侧对账**（替换 marker）：ship 前按 `work_order_id` 查
//!   `af_mes.mes_event`（SELECT FINAL），已在 CH 的 event_id 跳过，只补
//!   差量。MergeTree 无去重——幂等的真值源是 CH 本身，不是本地 marker；
//!   跨驱补送（重放驱新跃迁、上次 park 的批）因此结构性闭合。
//! - **fail-loud**：送批 Parked（CH 未接住、批已进 outbox）= EX2003——
//!   真实落账纪律，不接受"可能已写"；at-least-once 由 af-mes outbox +
//!   下轮 `sync` 开头的 retry_outbox 收敛（重试入账后再对账，不双写）。
//!
//! 真实 ClickHouse 不进 `cargo test`（af-mes 纪律）；mock 集成见
//! tests/mes_sink.rs，mingda 实测见 RESULT-消债ABC.md。

use std::path::Path;

use af_mes::{Accept, MesEvent, MesReader, MesWriter, WriterConfig};

use crate::error::ExeclineError;
use crate::mes::{MesFileWriter, MesSink, MesSyncKind, MesSyncReport};

/// MES CH 目标声明（订单面 CLI 旗标 `--mes-ch` 的解析产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MesChTarget {
    /// `auto`：env `MES_CH_URL` > 0600 `~/.config/mes-ch/url`，缺一不可。
    Auto,
    /// 显式端点 URL（可含 `user:pass@host` 凭据段）。
    Url(String),
}

impl MesChTarget {
    /// 解析目标字符串：`auto`（大小写不敏感）或 URL 原样（凭据段在
    /// [`resolve_mes_ch_endpoint_with`] 拆）。解析不碰 IO。
    pub fn parse(raw: &str) -> Result<Self, ExeclineError> {
        if raw.eq_ignore_ascii_case("auto") {
            Ok(Self::Auto)
        } else if raw.trim().is_empty() {
            Err(ExeclineError::MesCh {
                detail: "--mes-ch 目标为空（可用 auto 或显式 URL）".into(),
            })
        } else {
            Ok(Self::Url(raw.trim().to_string()))
        }
    }
}

/// 解析后的端点：URL 本体（无凭据段）+ 可选口令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MesChEndpoint {
    pub endpoint: String,
    pub auth: Option<(String, String)>,
    /// 回执与日志用的 host:port（无 scheme、无路径、无凭据）。
    pub host_only: String,
}

/// 端点/凭据解析（纯函数；env 与 0600 文件内容以参数注入，可单测）。
///
/// 拆分规则：`scheme://user:pass@host...` → 认证对 + 去凭据 URL；
/// 无 `@` = 匿名；非 `http://` = EX2003（官方 crate HTTP 接口口径，与
/// af-mes `validate_endpoint` 同判）。`Auto` 的取值序：env > 文件 > EX2003。
pub fn resolve_mes_ch_endpoint_with(
    target: &MesChTarget,
    env_url: Option<&str>,
    url_file: Option<&str>,
) -> Result<MesChEndpoint, ExeclineError> {
    let raw = match target {
        MesChTarget::Auto => env_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| url_file.map(str::trim).filter(|value| !value.is_empty()))
            .ok_or_else(|| ExeclineError::MesCh {
                detail:
                    "MES_CH_URL 未设且 0600 凭据文件 ~/.config/mes-ch/url 不可读（auto 无从解析）"
                        .into(),
            })?,
        MesChTarget::Url(url) => url.trim(),
    };
    split_userinfo(raw)
}

/// 环境态解析（真实取 env 与 0600 文件；凭据只在内存，不落日志）。
pub fn resolve_mes_ch_endpoint(target: &MesChTarget) -> Result<MesChEndpoint, ExeclineError> {
    let url_file = std::fs::read_to_string(mes_ch_url_file()).ok();
    resolve_mes_ch_endpoint_with(
        target,
        std::env::var("MES_CH_URL").ok().as_deref(),
        url_file.as_deref(),
    )
}

/// 0600 凭据文件路径（工作区既有约定：`~/.config/mes-ch/url`）。
fn mes_ch_url_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    std::path::PathBuf::from(home).join(".config/mes-ch/url")
}

/// `scheme://user:pass@rest` → 端点 + 认证对；无 `@` 原样匿名。
fn split_userinfo(raw: &str) -> Result<MesChEndpoint, ExeclineError> {
    let (scheme, rest) = raw.split_once("://").ok_or_else(|| ExeclineError::MesCh {
        detail: format!("MES CH 端点必须是 http:// URL（官方 crate HTTP 接口）: {raw}"),
    })?;
    if scheme != "http" || rest.is_empty() {
        return Err(ExeclineError::MesCh {
            detail: format!("MES CH 端点必须是 http:// URL: {raw}"),
        });
    }
    let auth = match rest.split_once('@') {
        Some((userinfo, _)) => {
            let (user, password) =
                userinfo
                    .split_once(':')
                    .ok_or_else(|| ExeclineError::MesCh {
                        detail: "MES CH URL 凭据段应为 user:pass@host".into(),
                    })?;
            if user.is_empty() || password.is_empty() {
                return Err(ExeclineError::MesCh {
                    detail: "MES CH URL 凭据段的 user/pass 不得为空".into(),
                });
            }
            Some((user.to_string(), password.to_string()))
        }
        None => None,
    };
    let endpoint = match rest.split_once('@') {
        Some((_, hostpart)) => format!("{scheme}://{hostpart}"),
        None => raw.to_string(),
    };
    let host_only = endpoint
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(&endpoint)
        .to_string();
    Ok(MesChEndpoint {
        endpoint,
        auth,
        host_only,
    })
}

/// CH 下游 sink：文件正本（必写）+ 读侧 event_id 对账差量送批。
///
/// 目录布局（订单目录内）：`mes-events.jsonl` 正本、`mes-outbox/` CH 批
/// outbox。同目录重开即续（outbox 载入、正本重载）——重跑 submit 只补差量。
pub struct MesChSink {
    file: MesFileWriter,
    writer: MesWriter,
    reader: MesReader,
    host_only: String,
    auth: bool,
}

impl MesChSink {
    /// 打开 CH sink。`order_dir` = 驱动 out_dir（正本与 outbox 落其内）。
    pub fn open(target: &MesChTarget, order_dir: &Path) -> Result<Self, ExeclineError> {
        let resolved = resolve_mes_ch_endpoint(target)?;
        Self::open_endpoint(&resolved, order_dir)
    }

    /// 以已解析端点打开（af-order 声明层与测试直接注入，跳过字符串解析）。
    pub fn open_endpoint(
        resolved: &MesChEndpoint,
        order_dir: &Path,
    ) -> Result<Self, ExeclineError> {
        let file = MesFileWriter::new(order_dir.join(crate::driver::MES_FILE));
        let mut config = WriterConfig::new(resolved.endpoint.clone(), order_dir.join("mes-outbox"));
        // 每行独立批：送批即 Sent/Parked，失败即时显形（事件量级 = 每单个位数）。
        config.max_rows = 1;
        config.max_buffered = 1;
        if let Some((user, password)) = &resolved.auth {
            config = config.with_auth(user.clone(), password.clone());
        }
        let writer =
            MesWriter::open(config, std::sync::Arc::new(af_mes::SystemClock)).map_err(|err| {
                ExeclineError::MesCh {
                    detail: format!("MesWriter open 失败: {err}"),
                }
            })?;
        let database = writer.config().database.clone();
        let reader = match &resolved.auth {
            Some((user, password)) => {
                MesReader::open_with_auth(&resolved.endpoint, &database, user, password)
            }
            None => MesReader::open(&resolved.endpoint, &database),
        }
        .map_err(|err| ExeclineError::MesCh {
            detail: format!("MesReader open 失败: {err}"),
        })?;
        Ok(Self {
            file,
            writer,
            reader,
            host_only: resolved.host_only.clone(),
            auth: resolved.auth.is_some(),
        })
    }
}

impl MesSink for MesChSink {
    fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
        self.file.append(event)
    }

    fn path(&self) -> &Path {
        self.file.path()
    }

    /// 对账面：retry_outbox（drain 停运期批）→ 读侧按 work_order_id 对账
    /// event_id → 补差量送批 → 回执（凭据零落）。
    fn sync(&mut self) -> Result<MesSyncReport, ExeclineError> {
        // ① 先 drain outbox：park 的批可能已在/将进 CH，读侧对账须在其后
        //    才是终态真值（避免"重试入账又按旧对账补送"的双写）。
        if self.writer.outbox_batches() > 0 {
            self.writer
                .retry_outbox()
                .map_err(|err| ExeclineError::MesCh {
                    detail: format!("outbox 重试仍失败（文件保留，at-least-once）: {err}"),
                })?;
        }
        // ② 读侧对账：正本全集 − CH 已有 = 差量
        let events = MesFileWriter::load(self.file.path())?;
        let mut in_ch: Vec<String> = Vec::new();
        for work_order in events
            .iter()
            .map(|event| event.work_order_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let rows = self
                .reader
                .fetch_events(&af_mes::EventFilter {
                    work_order_id: Some(work_order.to_string()),
                    ..af_mes::EventFilter::default()
                })
                .map_err(|err| ExeclineError::MesCh {
                    detail: format!("CH 读侧对账失败（{work_order}）: {err}"),
                })?;
            in_ch.extend(rows.into_iter().map(|row| row.event_id));
        }
        let pending: Vec<&MesEvent> = events
            .iter()
            .filter(|event| !in_ch.contains(&event.event_id))
            .collect();
        // ③ 差量送批：Parked = EX2003 fail-loud
        let mut delivered = 0usize;
        for event in &pending {
            match self
                .writer
                .push((*event).clone())
                .map_err(|err| ExeclineError::MesCh {
                    detail: format!("MES push 失败: {err}"),
                })? {
                Accept::Sent { rows } => delivered += rows,
                Accept::Parked { .. } => {
                    return Err(ExeclineError::MesCh {
                        detail: "MES 批未入 ClickHouse（park 进 outbox），真实落账纪律不接受\"可能已写\"；重跑只补差量".into(),
                    });
                }
                Accept::Buffered { .. } => {}
            }
        }
        match self.writer.flush().map_err(|err| ExeclineError::MesCh {
            detail: format!("MES flush 失败: {err}"),
        })? {
            Accept::Sent { rows } => delivered += rows,
            Accept::Parked { rows } => {
                return Err(ExeclineError::MesCh {
                    detail: format!(
                        "MES 批未入 ClickHouse（{rows} 行 park 进 outbox），真实落账纪律；重跑只补差量"
                    ),
                });
            }
            Accept::Buffered { .. } => {}
        }
        Ok(MesSyncReport {
            kind: MesSyncKind::ClickHouse,
            events_total: events.len(),
            already_in_ch: events.len() - pending.len(),
            shipped: pending.len(),
            rows_delivered: delivered,
            outbox_batches: self.writer.outbox_batches(),
            shipped_event_ids: pending.iter().map(|event| event.event_id.clone()).collect(),
            endpoint_host: self.host_only.clone(),
            auth: self.auth,
        })
    }
}
