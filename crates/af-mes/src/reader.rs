//! 读 `mes_event`。SQL 由本模块生成。
//!
//! 过滤值走客户端绑定，不拼进 SQL。不生成 `OPTIMIZE`，也不生成 `rand64`。
//!
//! 不带 `FINAL`：`af_mes.mes_event` 是 MergeTree（poc-mes 裁决原文），没有
//! 版本列，FINAL 对它无意义且真实例直接拒绝——mingda CH 26.7.34.3 实测
//! `Code: 181 ILLEGAL_FINAL: Storage MergeTree doesn't support FINAL`
//! （2026-10-25 消债 B③ 真实例验证暴露；mock 无 SQL 引擎测不出）。
//! `select_final`（ReplacingMergeTree 读包装器）保留原语义不回退 OPTIMIZE。

use crate::ch::{self, database_or_default};
use crate::error::MesError;
use crate::event::MesEvent;
use crate::final_query::guard_read_sql;
use clickhouse::Client;
use tokio::runtime::Runtime;

/// 可选的等值过滤。空字段表示不加这个谓词。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    pub work_order_id: Option<String>,
    pub line_id: Option<String>,
}

pub struct MesReader {
    runtime: Runtime,
    client: Client,
}

impl MesReader {
    /// 匿名 reader（历史行为原样）：等价于 [`MesReader::open_with_auth`]
    /// 的匿名形态（不带任何 ClickHouse 认证头）。
    pub fn open(
        endpoint: impl Into<String>,
        database: impl Into<String>,
    ) -> Result<Self, MesError> {
        let endpoint = endpoint.into();
        ch::validate_endpoint(&endpoint)?;
        let database = database_or_default(&database.into()).to_string();
        ch::validate_ident(&database, "database")?;
        let client = ch::build_client_auth(&endpoint, &database, None, None);
        let runtime = ch::runtime()?;
        Ok(Self { runtime, client })
    }

    /// 带口令认证的 reader（正式 API；CH 读侧对账需要与 writer 同款的
    /// 认证面——mingda 实例 2026-10-22 起对 default 用户设口令）。
    ///
    /// 凭据以 `X-ClickHouse-User` / `X-ClickHouse-Key` 头进入官方客户端，
    /// 明文只在内存与请求头，不落盘不打印。匿名读用 [`MesReader::open`]。
    pub fn open_with_auth(
        endpoint: impl Into<String>,
        database: impl Into<String>,
        user: &str,
        password: &str,
    ) -> Result<Self, MesError> {
        let endpoint = endpoint.into();
        ch::validate_endpoint(&endpoint)?;
        let database = database_or_default(&database.into()).to_string();
        ch::validate_ident(&database, "database")?;
        let client = ch::build_client_auth(&endpoint, &database, Some(user), Some(password));
        let runtime = ch::runtime()?;
        Ok(Self { runtime, client })
    }

    pub fn apply_schema(&self) -> Result<(), MesError> {
        ch::apply_schema(&self.runtime, &self.client)
    }

    /// `SELECT ?fields FROM mes_event`，可选工单 / 产线等值条件（MergeTree
    /// 不带 FINAL——见模块文档）。
    pub fn fetch_events(&self, filter: &EventFilter) -> Result<Vec<MesEvent>, MesError> {
        let (sql, binds) = event_select_sql(filter);
        guard_read_sql(&sql)?;
        let client = self.client.clone();
        ch::block_on(&self.runtime, async move {
            let mut query = client.query(&sql);
            for value in &binds {
                query = query.bind(value);
            }
            query
                .fetch_all::<MesEvent>()
                .await
                .map_err(|err| MesError::ClickHouse(err.to_string()))
        })
    }
}

fn event_select_sql(filter: &EventFilter) -> (String, Vec<String>) {
    let mut sql = String::from("SELECT ?fields FROM mes_event");
    let mut binds = Vec::new();
    let mut predicates = Vec::new();
    if let Some(work_order_id) = &filter.work_order_id {
        predicates.push("work_order_id = ?");
        binds.push(work_order_id.clone());
    }
    if let Some(line_id) = &filter.line_id {
        predicates.push("line_id = ?");
        binds.push(line_id.clone());
    }
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    (sql, binds)
}
