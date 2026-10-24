//! 官方 clickhouse 0.15.3 客户端。
//!
//! 显式 `Compression::Lz4`，并关掉校验。校验打开时插入格式变成
//! `RowBinaryWithNamesAndTypes`；裁决要的是 HTTP + RowBinary + lz4。
//! `test-util` 会把默认压缩改成 None，所以这里不依赖默认值。

use crate::error::MesError;
use crate::schema::{DEFAULT_DATABASE, SCHEMA_STATEMENTS};
use clickhouse::Client;
use std::future::Future;
use tokio::runtime::Runtime;

/// 客户端构造（正式 API；原补丁台账-第二阶段 #A 转正，历史 `build_client`
/// 并入本函数的 `None/None` 形态——匿名请求线逐字节不变，由 tests/http.rs
/// `anonymous_writer_wire_is_unchanged_no_credentials` 钉死）。
///
/// 口令认证面向 mingda MES 实例（2026-10-22 起 default 用户设口令）等
/// 需要凭据的 ClickHouse 部署。凭据经官方客户端以 `X-ClickHouse-User` /
/// `X-ClickHouse-Key` 请求头携带（明文只在头里，不进 URL、不进日志）；
/// 由调用方解析注入，本 crate 不读凭据文件。
///
/// `user`/`password` 任一为 `None` 即该头不发送——两者都 `None` 时与
/// 认证面引入前的请求线完全一致。
pub fn build_client_auth(
    endpoint: &str,
    database: &str,
    user: Option<&str>,
    password: Option<&str>,
) -> Client {
    let mut client = Client::default()
        .with_url(endpoint)
        .with_database(database)
        .with_compression(clickhouse::Compression::Lz4)
        .with_validation(false)
        .with_product_info("af-mes", env!("CARGO_PKG_VERSION"));
    if let Some(user) = user {
        client = client.with_user(user);
    }
    if let Some(password) = password {
        client = client.with_password(password);
    }
    client
}

pub fn runtime() -> Result<Runtime, MesError> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("af-mes")
        .enable_io()
        .enable_time()
        .build()
        .map_err(|err| MesError::Config(format!("tokio runtime: {err}")))
}

pub fn block_on<F>(runtime: &Runtime, future: F) -> F::Output
where
    F: Future,
{
    runtime.block_on(future)
}

pub fn validate_endpoint(endpoint: &str) -> Result<(), MesError> {
    if endpoint.starts_with("http://") && !endpoint[7..].is_empty() {
        Ok(())
    } else {
        Err(MesError::Config(
            "端点必须是注入的 http:// URL（官方 crate 的 HTTP 接口）".into(),
        ))
    }
}

pub fn validate_ident(name: &str, what: &str) -> Result<(), MesError> {
    let mut chars = name.chars();
    let ok = match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {
            chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(MesError::Config(format!("{what} 不是合法标识符: {name}")))
    }
}

pub fn database_or_default(database: &str) -> &str {
    if database.is_empty() {
        DEFAULT_DATABASE
    } else {
        database
    }
}

pub fn apply_schema(runtime: &Runtime, client: &Client) -> Result<(), MesError> {
    for statement in SCHEMA_STATEMENTS {
        block_on(runtime, async {
            client
                .query(statement)
                .execute()
                .await
                .map_err(|err| MesError::ClickHouse(err.to_string()))
        })?;
    }
    Ok(())
}
