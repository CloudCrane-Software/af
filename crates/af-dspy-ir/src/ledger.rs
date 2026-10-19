//! JIT 护栏账本。
//!
//! 一行是一次预筛或一次修复判定。序号从 1 连续递增，只追加。
//! 时刻由调用方注入，读回时原样保留，不按墙钟改写。
//!
//! 写入只在 crate 内部发生。恢复一份 JSON 不会凭空造出放行凭证：
//! 凭证只有预筛和修复判定两个挂接点能发。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::codes::{E_INFRA, E_SUBJECT, REPAIR_DENY_CODES};
use crate::model::blank;

/// 账本文档版本。不是 IR 的 `version`。缺失或不是整数 1 就整份拒绝。
pub const LEDGER_VERSION: u32 = 1;

/// 读账本失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("护栏账本不是 JSON: {0}")]
    Json(String),
    #[error("{0}")]
    Invalid(String),
}

/// 一条审计原因。`code` 是 DI1xxx 或 JG1xxx / JG2xxx。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardReason {
    pub code: String,
    pub message: String,
}

/// 账本里的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerEntry {
    pub seq: u64,
    pub at_ms: i64,
    pub event: GuardEvent,
}

/// 预筛或修复判定的结果。字段名是审计文档的一部分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GuardEvent {
    ScreenAdmit {
        batch_id: String,
        candidate_id: String,
        program_id: String,
    },
    ScreenReject {
        batch_id: String,
        candidate_id: String,
        program_id: Option<String>,
        reasons: Vec<GuardReason>,
    },
    RepairAllow {
        run_id: String,
        code: String,
        scored: bool,
    },
    RepairDeny {
        run_id: String,
        failure_code: Option<String>,
        scored: bool,
        reason_code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerFile {
    ledger_version: u32,
    entries: Vec<LedgerEntry>,
}

/// 进程内的追加账本。耐久化是调用方把 [`Ledger::to_json`] 存下来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger {
    entries: Vec<LedgerEntry>,
    next_seq: u64,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_seq: 1,
        }
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn append(&mut self, at_ms: i64, event: GuardEvent) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.entries.push(LedgerEntry { seq, at_ms, event });
        seq
    }

    /// 写成审计 JSON。信封是 `ledger_version`，不是 IR 的 `version`。
    pub fn to_json(&self) -> Result<String, LedgerError> {
        let doc = LedgerFile {
            ledger_version: LEDGER_VERSION,
            entries: self.entries.clone(),
        };
        serde_json::to_string_pretty(&doc).map_err(|err| LedgerError::Json(err.to_string()))
    }

    /// 读回审计 JSON。序号必须从 1 连续，放行行必须仍是崩溃类且没有分数。
    pub fn from_json(text: &str) -> Result<Self, LedgerError> {
        let value: Value =
            serde_json::from_str(text).map_err(|err| LedgerError::Json(err.to_string()))?;
        read_version(&value)?;
        let doc: LedgerFile = serde_json::from_value(value)
            .map_err(|err| LedgerError::Invalid(format!("护栏账本无法读取: {err}")))?;
        check_entries(&doc.entries)?;
        let next_seq = u64::try_from(doc.entries.len())
            .ok()
            .and_then(|len| len.checked_add(1))
            .ok_or_else(|| LedgerError::Invalid("护栏账本序号溢出".into()))?;
        Ok(Self {
            entries: doc.entries,
            next_seq,
        })
    }
}

fn read_version(value: &Value) -> Result<(), LedgerError> {
    let Some(obj) = value.as_object() else {
        return Err(LedgerError::Invalid("护栏账本必须是 JSON 对象".into()));
    };
    let Some(raw) = obj.get("ledger_version") else {
        return Err(LedgerError::Invalid("护栏账本缺少 ledger_version".into()));
    };
    match raw {
        Value::Number(number) => {
            let Some(wide) = number.as_u64() else {
                return Err(LedgerError::Invalid(format!(
                    "无法识别的 ledger_version: {raw}"
                )));
            };
            let Ok(version) = u32::try_from(wide) else {
                return Err(LedgerError::Invalid(format!(
                    "无法识别的 ledger_version: {raw}"
                )));
            };
            if version != LEDGER_VERSION {
                return Err(LedgerError::Invalid(format!(
                    "未知 ledger_version {version}，只接受 {LEDGER_VERSION}"
                )));
            }
            Ok(())
        }
        other => Err(LedgerError::Invalid(format!(
            "无法识别的 ledger_version: {other}"
        ))),
    }
}

fn check_entries(entries: &[LedgerEntry]) -> Result<(), LedgerError> {
    for (index, entry) in entries.iter().enumerate() {
        let Some(expect) = u64::try_from(index).ok().and_then(|i| i.checked_add(1)) else {
            return Err(LedgerError::Invalid("护栏账本序号溢出".into()));
        };
        if entry.seq != expect {
            return Err(LedgerError::Invalid(format!(
                "账本序号不连续：第 {} 行 seq={}，应为 {expect}",
                index + 1,
                entry.seq
            )));
        }
        check_event(&entry.event)?;
    }
    Ok(())
}

fn check_event(event: &GuardEvent) -> Result<(), LedgerError> {
    match event {
        GuardEvent::ScreenAdmit {
            batch_id,
            candidate_id,
            program_id,
        } => {
            require_text("screen_admit.batch_id", batch_id)?;
            require_text("screen_admit.candidate_id", candidate_id)?;
            require_text("screen_admit.program_id", program_id)?;
        }
        GuardEvent::ScreenReject { reasons, .. } => {
            if reasons.is_empty() {
                return Err(LedgerError::Invalid(
                    "screen_reject 至少要有一条原因".into(),
                ));
            }
            for reason in reasons {
                require_text("reason.code", &reason.code)?;
                require_text("reason.message", &reason.message)?;
            }
        }
        GuardEvent::RepairAllow {
            run_id,
            code,
            scored,
        } => {
            require_text("repair_allow.run_id", run_id)?;
            if code != E_INFRA && code != E_SUBJECT {
                return Err(LedgerError::Invalid(format!(
                    "repair_allow 的码只能是 {E_INFRA} 或 {E_SUBJECT}，实际 {code}"
                )));
            }
            if *scored {
                return Err(LedgerError::Invalid("repair_allow 不能带评测分数".into()));
            }
        }
        GuardEvent::RepairDeny {
            failure_code,
            reason_code,
            message,
            ..
        } => {
            if let Some(code) = failure_code {
                require_text("repair_deny.failure_code", code)?;
            }
            if !REPAIR_DENY_CODES.contains(&reason_code.as_str()) {
                return Err(LedgerError::Invalid(format!(
                    "未知的修复拒绝码 {reason_code}"
                )));
            }
            require_text("repair_deny.message", message)?;
        }
    }
    Ok(())
}

fn require_text(what: &str, text: &str) -> Result<(), LedgerError> {
    if blank(text) {
        Err(LedgerError::Invalid(format!("{what} 为空")))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{JG_LOW_SCORE, JG_NOT_A_CRASH};
    use serde_json::json;

    fn admit(batch: &str, candidate: &str, program: &str) -> GuardEvent {
        GuardEvent::ScreenAdmit {
            batch_id: batch.to_string(),
            candidate_id: candidate.to_string(),
            program_id: program.to_string(),
        }
    }

    fn reject(candidate: &str) -> GuardEvent {
        GuardEvent::ScreenReject {
            batch_id: "bo-1".to_string(),
            candidate_id: candidate.to_string(),
            program_id: None,
            reasons: vec![GuardReason {
                code: "DI1002".to_string(),
                message: "签名不存在".to_string(),
            }],
        }
    }

    #[test]
    fn injected_time_and_seq_roundtrip_through_json() {
        let mut ledger = Ledger::new();
        assert_eq!(ledger.append(-5, admit("bo-1", "c1", "qa")), 1);
        assert_eq!(ledger.append(0, reject("c2")), 2);
        assert_eq!(
            ledger.append(
                40,
                GuardEvent::RepairAllow {
                    run_id: "run-1".to_string(),
                    code: E_INFRA.to_string(),
                    scored: false,
                }
            ),
            3
        );
        assert_eq!(
            ledger.append(
                41,
                GuardEvent::RepairDeny {
                    run_id: "run-2".to_string(),
                    failure_code: Some(E_SUBJECT.to_string()),
                    scored: true,
                    reason_code: "JG2003".to_string(),
                    message: "崩溃码附带了分数".to_string(),
                }
            ),
            4
        );
        assert_eq!(ledger.entries()[0].at_ms, -5);
        assert_eq!(ledger.entries()[1].at_ms, 0);

        let restored = Ledger::from_json(&ledger.to_json().expect("json")).expect("reload");
        assert_eq!(restored, ledger);

        let value: Value = serde_json::from_str(&ledger.to_json().expect("json")).expect("value");
        assert_eq!(value["ledger_version"], json!(1));
        assert_eq!(value["entries"][0]["event"]["kind"], json!("screen_admit"));
        assert_eq!(value["entries"][1]["event"]["kind"], json!("screen_reject"));
        assert_eq!(value["entries"][2]["event"]["kind"], json!("repair_allow"));
        assert_eq!(value["entries"][3]["event"]["kind"], json!("repair_deny"));
        assert_eq!(value["entries"][2]["event"]["code"], json!(E_INFRA));
        assert_eq!(value["entries"][2]["event"]["scored"], json!(false));
    }

    #[test]
    fn envelope_rejects_bad_version_and_unknown_keys() {
        let missing = Ledger::from_json(r#"{"entries":[]}"#).expect_err("missing");
        assert!(missing.to_string().contains("ledger_version"), "{missing}");

        for text in [
            r#"{"ledger_version":2,"entries":[]}"#,
            r#"{"ledger_version":0,"entries":[]}"#,
            r#"{"ledger_version":"1","entries":[]}"#,
            r#"{"ledger_version":1.5,"entries":[]}"#,
            r#"{"ledger_version":-1,"entries":[]}"#,
            r#"{"ledger_version":null,"entries":[]}"#,
        ] {
            let err = Ledger::from_json(text).expect_err(text);
            assert!(
                err.to_string().contains("ledger_version"),
                "{text} -> {err}"
            );
        }

        let extra =
            Ledger::from_json(r#"{"ledger_version":1,"entries":[],"extra":1}"#).expect_err("extra");
        assert!(extra.to_string().contains("无法读取"), "{extra}");

        let empty = Ledger::from_json(r#"{"ledger_version":1,"entries":[]}"#).expect("empty");
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn broken_shape_is_rejected() {
        let gap = r#"{"ledger_version":1,"entries":[
            {"seq":1,"at_ms":1,"event":{"kind":"screen_admit","batch_id":"b","candidate_id":"c","program_id":"p"}},
            {"seq":3,"at_ms":1,"event":{"kind":"screen_admit","batch_id":"b","candidate_id":"d","program_id":"p"}}
        ]}"#;
        assert!(Ledger::from_json(gap)
            .expect_err("gap")
            .to_string()
            .contains("不连续"));

        let zero = r#"{"ledger_version":1,"entries":[
            {"seq":0,"at_ms":1,"event":{"kind":"screen_admit","batch_id":"b","candidate_id":"c","program_id":"p"}}
        ]}"#;
        assert!(Ledger::from_json(zero).is_err());

        let empty_reasons = r#"{"ledger_version":1,"entries":[
            {"seq":1,"at_ms":1,"event":{"kind":"screen_reject","batch_id":"b","candidate_id":"c","program_id":null,"reasons":[]}}
        ]}"#;
        assert!(Ledger::from_json(empty_reasons)
            .expect_err("reasons")
            .to_string()
            .contains("原因"));

        let blank_admit = r#"{"ledger_version":1,"entries":[
            {"seq":1,"at_ms":1,"event":{"kind":"screen_admit","batch_id":"b","candidate_id":" ","program_id":"p"}}
        ]}"#;
        assert!(Ledger::from_json(blank_admit).is_err());

        let unknown_kind = r#"{"ledger_version":1,"entries":[
            {"seq":1,"at_ms":1,"event":{"kind":"screen_maybe","batch_id":"b"}}
        ]}"#;
        assert!(Ledger::from_json(unknown_kind).is_err());
    }

    #[test]
    fn forged_repair_allow_is_rejected() {
        let score_code = format!(
            r#"{{"ledger_version":1,"entries":[{{"seq":1,"at_ms":1,"event":{{"kind":"repair_allow","run_id":"r","code":"{JG_LOW_SCORE}","scored":false}}}}]}}"#
        );
        let err = Ledger::from_json(&score_code).expect_err("score code");
        assert!(err.to_string().contains(E_INFRA), "{err}");

        let scored = format!(
            r#"{{"ledger_version":1,"entries":[{{"seq":1,"at_ms":1,"event":{{"kind":"repair_allow","run_id":"r","code":"{E_SUBJECT}","scored":true}}}}]}}"#
        );
        let err = Ledger::from_json(&scored).expect_err("scored");
        assert!(err.to_string().contains("分数"), "{err}");

        let bad_deny = format!(
            r#"{{"ledger_version":1,"entries":[{{"seq":1,"at_ms":1,"event":{{"kind":"repair_deny","run_id":"r","failure_code":"E_TIMEOUT","scored":false,"reason_code":"{JG_NOT_A_CRASH}","message":" "}}}}]}}"#
        );
        assert!(Ledger::from_json(&bad_deny).is_err());

        let unknown_deny = r#"{"ledger_version":1,"entries":[
            {"seq":1,"at_ms":1,"event":{"kind":"repair_deny","run_id":"r","failure_code":"E_SCORE","scored":true,"reason_code":"JG9999","message":"no"}}
        ]}"#;
        assert!(Ledger::from_json(unknown_deny)
            .expect_err("deny code")
            .to_string()
            .contains("JG9999"));
    }

    #[test]
    fn restore_then_append_continues_seq() {
        let mut ledger = Ledger::new();
        ledger.append(7, admit("bo-1", "c1", "qa"));
        let mut restored = Ledger::from_json(&ledger.to_json().expect("json")).expect("reload");
        let seq = restored.append(8, reject("c2"));
        assert_eq!(seq, 2);
        assert_eq!(restored.entries()[1].at_ms, 8);
        assert_eq!(restored.len(), 2);
    }
}
