//! ClickHouse HTTP 批写入。
//!
//! 内存批在行数达到 `max_rows`（默认 5 万）或注入时钟走过 `max_delay` 时发送，
//! 哪个先到用哪个。比较发生在 [`MesWriter::push`] 和 [`MesWriter::poll`]，
//! 没有后台睡眠。
//!
//! 发送走官方客户端的 `insert`：HTTP、`FORMAT RowBinary`、`decompress=1`（lz4）。
//! 失败的整批写入 outbox 文件，内存副本随后清掉。outbox 里还有更早的批时，
//! 新批排在后面，不插到前面。outbox 达到上限后，新的 `push` 返回背压。

use crate::ch::{self, database_or_default};
use crate::clock::Clock;
use crate::error::{Accept, MesError};
use crate::event::MesEvent;
use crate::outbox::Outbox;
use crate::schema::{
    DEFAULT_MAX_DELAY, DEFAULT_MAX_OUTBOX_BATCHES, DEFAULT_MAX_ROWS, TABLE_MES_EVENT,
};
use clickhouse::Client;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// 写入器配置。端点、时钟、阈值都由调用方注入。
///
/// 认证面（正式 API）：`user` / `password` 任一为 `None` 即匿名——与
/// 未引入认证前的请求线逐字节一致；两者经 [`WriterConfig::with_auth`]
/// 或直接字段赋值注入后，凭据以 `X-ClickHouse-User` / `X-ClickHouse-Key`
/// 头进入官方客户端，明文只在内存与请求头，不落盘不打印（Debug 已脱敏）。
#[derive(Clone)]
pub struct WriterConfig {
    pub endpoint: String,
    pub database: String,
    pub table: String,
    pub max_rows: u64,
    pub max_delay: Duration,
    /// 内存里最多留多少行。必须 ≥ `max_rows`，否则批还没到阈值就被截断。
    pub max_buffered: usize,
    pub max_outbox_batches: usize,
    pub outbox_dir: PathBuf,
    /// 口令认证用户名（None = 匿名，旧行为）。
    pub user: Option<String>,
    /// 口令认证密码（None = 匿名，旧行为）。Debug 输出脱敏为 "***"。
    pub password: Option<String>,
}

impl fmt::Debug for WriterConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WriterConfig")
            .field("endpoint", &self.endpoint)
            .field("database", &self.database)
            .field("table", &self.table)
            .field("max_rows", &self.max_rows)
            .field("max_delay", &self.max_delay)
            .field("max_buffered", &self.max_buffered)
            .field("max_outbox_batches", &self.max_outbox_batches)
            .field("outbox_dir", &self.outbox_dir)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .finish()
    }
}

impl WriterConfig {
    pub fn new(endpoint: impl Into<String>, outbox_dir: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            database: crate::schema::DEFAULT_DATABASE.into(),
            table: TABLE_MES_EVENT.into(),
            max_rows: DEFAULT_MAX_ROWS,
            max_delay: DEFAULT_MAX_DELAY,
            max_buffered: usize::try_from(DEFAULT_MAX_ROWS).unwrap_or(usize::MAX),
            max_outbox_batches: DEFAULT_MAX_OUTBOX_BATCHES,
            outbox_dir: outbox_dir.into(),
            user: None,
            password: None,
        }
    }

    /// 注入口令认证（builder）。凭据只进官方客户端与请求头，
    /// 不落盘不打印；`None/None`（缺省）= 匿名，行为与旧版一致。
    pub fn with_auth(mut self, user: impl Into<String>, password: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self.password = Some(password.into());
        self
    }
}

pub struct MesWriter {
    runtime: Runtime,
    client: Client,
    config: WriterConfig,
    clock: Arc<dyn Clock>,
    buffer: Vec<MesEvent>,
    /// 当前内存批第一行被接收时的注入时刻。空批为 `None`。
    batch_started_ms: Option<i64>,
    outbox: Outbox,
    last_delivery_error: Option<String>,
}

impl MesWriter {
    pub fn open(config: WriterConfig, clock: Arc<dyn Clock>) -> Result<Self, MesError> {
        ch::validate_endpoint(&config.endpoint)?;
        if config.max_rows == 0 {
            return Err(MesError::Config("max_rows 必须 ≥ 1".into()));
        }
        let max_rows_usize = usize::try_from(config.max_rows).unwrap_or(usize::MAX);
        if config.max_buffered < max_rows_usize {
            return Err(MesError::Config(
                "max_buffered 不能小于 max_rows，否则批会在阈值前被截断".into(),
            ));
        }
        if config.max_outbox_batches == 0 {
            return Err(MesError::Config("max_outbox_batches 必须 ≥ 1".into()));
        }
        let database = database_or_default(&config.database).to_string();
        ch::validate_ident(&database, "database")?;
        ch::validate_ident(&config.table, "table")?;
        let outbox = Outbox::open(&config.outbox_dir)?;
        let client = ch::build_client_auth(
            &config.endpoint,
            &database,
            config.user.as_deref(),
            config.password.as_deref(),
        );
        let runtime = ch::runtime()?;
        let mut config = config;
        config.database = database;
        Ok(Self {
            runtime,
            client,
            config,
            clock,
            buffer: Vec::new(),
            batch_started_ms: None,
            outbox,
            last_delivery_error: None,
        })
    }

    pub fn config(&self) -> &WriterConfig {
        &self.config
    }

    pub fn pending_rows(&self) -> usize {
        self.buffer.len()
    }

    pub fn outbox_batches(&self) -> usize {
        self.outbox.len()
    }

    /// 最近一次没被 ClickHouse 接住的原因。成功发送后清空。
    pub fn last_delivery_error(&self) -> Option<&str> {
        self.last_delivery_error.as_deref()
    }

    pub fn apply_schema(&self) -> Result<(), MesError> {
        ch::apply_schema(&self.runtime, &self.client)
    }

    /// 收下这一行。到了行数或延迟就发送当前批。
    pub fn push(&mut self, event: MesEvent) -> Result<Accept, MesError> {
        self.ensure_can_accept()?;
        if !self.buffer.is_empty() && self.batch_is_due() {
            self.deliver_buffer()?;
            self.ensure_can_accept()?;
        }
        if self.buffer.is_empty() {
            self.batch_started_ms = Some(self.clock.now_ms());
        }
        self.buffer.push(event);
        if self.batch_is_due() {
            self.deliver_buffer()
        } else {
            Ok(Accept::Buffered {
                pending: self.buffer.len(),
            })
        }
    }

    /// 延迟到了就发送。没到就不动。调用方用自己的调度来调，本方法不睡眠。
    pub fn poll(&mut self) -> Result<Accept, MesError> {
        if self.buffer.is_empty() {
            return Ok(Accept::Sent { rows: 0 });
        }
        if self.batch_is_due() {
            self.deliver_buffer()
        } else {
            Ok(Accept::Buffered {
                pending: self.buffer.len(),
            })
        }
    }

    /// 不管阈值，把内存批送出去（失败则进 outbox）。
    pub fn flush(&mut self) -> Result<Accept, MesError> {
        self.deliver_buffer()
    }

    /// 按文件序重放 outbox。失败的那份留下，已成功的删除。
    /// 返回开始时的文件数。
    pub fn retry_outbox(&mut self) -> Result<usize, MesError> {
        let before = self.outbox.len();
        self.retry_outbox_files()?;
        Ok(before)
    }

    fn ensure_can_accept(&self) -> Result<(), MesError> {
        let outbox_batches = self.outbox.len();
        if outbox_batches >= self.config.max_outbox_batches {
            return Err(MesError::Backpressure {
                pending_rows: self.buffer.len(),
                outbox_batches,
                outbox_limit: self.config.max_outbox_batches,
                reason: "outbox 已满",
            });
        }
        if self.buffer.len() >= self.config.max_buffered {
            return Err(MesError::Backpressure {
                pending_rows: self.buffer.len(),
                outbox_batches,
                outbox_limit: self.config.max_outbox_batches,
                reason: "内存批已满且未能送出",
            });
        }
        Ok(())
    }

    fn batch_is_due(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        if self.buffer.len() as u64 >= self.config.max_rows {
            return true;
        }
        let Some(started) = self.batch_started_ms else {
            return false;
        };
        let elapsed = self.clock.now_ms().saturating_sub(started);
        elapsed >= self.max_delay_ms()
    }

    fn max_delay_ms(&self) -> i64 {
        i64::try_from(self.config.max_delay.as_millis()).unwrap_or(i64::MAX)
    }

    fn deliver_buffer(&mut self) -> Result<Accept, MesError> {
        if self.buffer.is_empty() {
            return Ok(Accept::Sent { rows: 0 });
        }
        let rows = self.buffer.len();
        if !self.outbox.is_empty() {
            match self.retry_outbox_files() {
                Ok(()) => {}
                Err(MesError::RetryFailed { detail, .. }) => {
                    self.last_delivery_error = Some(detail);
                    self.park_buffer(rows)?;
                    return Ok(Accept::Parked { rows });
                }
                Err(other) => return Err(other),
            }
        }
        match send_events(
            &self.runtime,
            &self.client,
            &self.config.table,
            &self.buffer,
        ) {
            Ok(()) => {
                self.buffer.clear();
                self.batch_started_ms = None;
                self.last_delivery_error = None;
                Ok(Accept::Sent { rows })
            }
            Err(detail) => {
                self.last_delivery_error = Some(detail);
                self.park_buffer(rows)?;
                Ok(Accept::Parked { rows })
            }
        }
    }

    fn park_buffer(&mut self, rows: usize) -> Result<(), MesError> {
        match self.outbox.park(&self.buffer) {
            Ok(_) => {
                self.buffer.clear();
                self.batch_started_ms = None;
                Ok(())
            }
            Err(detail) => Err(MesError::OutboxPersist { rows, detail }),
        }
    }

    fn retry_outbox_files(&mut self) -> Result<(), MesError> {
        let ids: Vec<String> = self.outbox.ids().to_vec();
        for id in ids {
            let events = self.outbox.load(&id)?;
            match send_events(&self.runtime, &self.client, &self.config.table, &events) {
                Ok(()) => self.outbox.remove(&id)?,
                Err(detail) => {
                    return Err(MesError::RetryFailed {
                        path: self.config.outbox_dir.join(format!("{id}.batch")),
                        detail,
                    });
                }
            }
        }
        Ok(())
    }
}

fn send_events(
    runtime: &Runtime,
    client: &Client,
    table: &str,
    rows: &[MesEvent],
) -> Result<(), String> {
    ch::block_on(runtime, async {
        let mut insert = client
            .insert::<MesEvent>(table)
            .await
            .map_err(|err| err.to_string())?;
        for row in rows {
            insert.write(row).await.map_err(|err| err.to_string())?;
        }
        insert.end().await.map_err(|err| err.to_string())?;
        Ok(())
    })
}
