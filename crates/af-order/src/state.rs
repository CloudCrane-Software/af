//! state 目录布局与订单读回（list / show 的数据源）。
//!
//! ```text
//! <state>/orders/<order_id>/ledger.jsonl        ← af-execline OrderLedger
//! <state>/orders/<order_id>/mes-events.jsonl    ← af-execline MES stub
//! <state>/orders/<order_id>/artifacts/…         ← 执行工件文件
//! <state>/orders/<order_id>/voucher.json        ← 凭证材料（ed25519 可验签）
//! <state>/orders/<order_id>/voucher-key.seed    ← ed25519 种子（0600）
//! ```
//!
//! 订单目录名 = order_id（spec 解析已保证是安全路径成分）。读回只信盘上
//! ledger（ADR-003：账本自成恢复源）；账本损坏（EX2xxx）透传为 OR2005，
//! 不静默跳过坏账。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::{LedgerRecord, OrderLedger};
use af_workorder::OrderState;
use serde::Serialize;

use crate::error::OrderError;

/// state 根下的订单根目录名。
pub const ORDERS_DIR: &str = "orders";
/// 订单目录内的账本文件名（与 af-execline 驱动层一致）。
pub const LEDGER_FILE: &str = "ledger.jsonl";
/// 订单目录内的 MES 事件文件名。
pub const MES_FILE: &str = "mes-events.jsonl";

/// 一张订单的读回摘要（list 一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderSummary {
    pub order_id: String,
    pub stable_id: String,
    /// 当前状态（最近一条带快照行的状态；serde 名：Created/…/Closed）。
    pub state: String,
    pub ledger_ops: usize,
}

/// state 目录句柄。
#[derive(Debug, Clone)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn orders_root(&self) -> PathBuf {
        self.root.join(ORDERS_DIR)
    }

    pub fn order_dir(&self, order_id: &str) -> PathBuf {
        self.orders_root().join(order_id)
    }

    /// 全部订单摘要，按订单目录名（= order_id）排序。
    /// `orders/` 不存在 = 空清单（新状态目录不是错误）；
    /// 无 ledger 的目录跳过；账本损坏透传 OR2005。
    pub fn list(&self) -> Result<Vec<OrderSummary>, OrderError> {
        let root = self.orders_root();
        let mut summaries = Vec::new();
        if !root.is_dir() {
            return Ok(summaries);
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(&root)
            .map_err(|err| OrderError::State {
                detail: format!("扫描 {}: {err}", root.display()),
            })?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for order_dir in entries {
            let ledger_path = order_dir.join(LEDGER_FILE);
            if !ledger_path.is_file() {
                continue;
            }
            let records = load_ledger(&ledger_path)?;
            let Some(first) = records.first() else {
                continue; // 空账本无从摘要（submit 首笔必写 create_order，防御而已）
            };
            summaries.push(OrderSummary {
                order_id: first.order_id.clone(),
                stable_id: first.stable_id.clone(),
                state: current_state(&records).unwrap_or_else(|| "Unknown".into()),
                ledger_ops: records.len(),
            });
        }
        Ok(summaries)
    }
}

/// 只读重载订单账本（af-execline EX2xxx 详情透传为 OR2005）。
pub fn load_ledger(path: &Path) -> Result<Vec<LedgerRecord>, OrderError> {
    OrderLedger::load(path).map_err(|err| OrderError::State {
        detail: err.to_string(),
    })
}

/// 当前状态：最近一条带快照行的状态名；全账本无快照 → None（不经 submit 不可能）。
pub fn current_state(records: &[LedgerRecord]) -> Option<String> {
    records
        .iter()
        .rev()
        .find_map(|record| record.order_after.as_ref())
        .map(|order| state_name(order.state()))
}

/// OrderState 的 serde 名（与 af-execline ledger 的 `state_after` 同一线格式）。
pub fn state_name(state: OrderState) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{state:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态名与 af-execline ledger 线格式一致。
    #[test]
    fn state_names_match_serde_wire_format() {
        use af_workorder::OrderState::*;
        assert_eq!(state_name(Created), "Created");
        assert_eq!(state_name(Released), "Released");
        assert_eq!(state_name(InProgress), "InProgress");
        assert_eq!(state_name(Completed), "Completed");
        assert_eq!(state_name(Closed), "Closed");
        assert_eq!(state_name(Cancelled), "Cancelled");
    }

    /// 新状态目录（无 orders/）= 空清单，不是错误。
    #[test]
    fn list_on_fresh_state_dir_is_empty_not_error() {
        let dir = std::env::temp_dir().join(format!("af-order-state-{}-fresh", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let state = StateDir::new(&dir);
        assert_eq!(state.list().unwrap(), Vec::new());
        assert!(state.order_dir("wo-x").ends_with("orders/wo-x"));
        let _ = fs::remove_dir_all(&dir);
    }
}
