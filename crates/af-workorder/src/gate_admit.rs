//! gate_admit 层的前级门工件两查（PoC-F 纯函数，不在 release 状态机硬塞载体）。
//!
//! **主代理串行合入交接（PoC-A）**：待 PoC-A 的 admit 骨架收卷后，在创建
//! downstream WorkOrder 之前调用 [`validate_required_gate_inputs`]。调用方负责
//! 从配方读 `RequiredGateInput`，并按 `input_id` 从 CAS 解出
//! `af.gate_result.v1`，把其 `payload.verdict` 原字符串填入
//! [`ProvidedGateInput`]。本函数不读文件、不推断默认 verdict、不把红/黄门映射成绿。
//! 本任务按隔离约束只交函数与单测，不修改 PoC-A 正在建设的 admit 调用点。
//!
//! **合流改名消歧（2026-11-04）**：本模块原同名 `admit`，与 af-order 的订单受理层
//! `af_order::admit`（AdmissionRequest→AdmissionReceipt）同名不同层，改名
//! `gate_admit` 以消歧；af_order::admit 受理层保留原名。

use serde::{Deserialize, Serialize};

/// 配方声明的一条必需前级输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredGateInput {
    pub input_id: String,
}

/// 调用方已从门工件读出的前级判定。未知字符串也不是 pass， fail-closed。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvidedGateInput {
    pub input_id: String,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum GateAdmitRejection {
    #[error("RequiredInputMissing: 必需前级门工件 {input_id} 缺失")]
    RequiredInputMissing { input_id: String },
    #[error("InputGateNotGreen: 前级门工件 {input_id} verdict={verdict}，不是 pass")]
    InputGateNotGreen { input_id: String, verdict: String },
}

/// 两查且仅两查：先查存在，再查 verdict 是否严格等于 `pass`。
///
/// 同一 input_id 若提供多份，任何一份非绿都会拒单，避免用一份绿工件洗掉同名的
/// 红工件；重复身份本身的更多准入规则属于 PoC-A 骨架，不在本函数扩张。
pub fn validate_required_gate_inputs(
    required: &[RequiredGateInput],
    provided: &[ProvidedGateInput],
) -> Result<(), GateAdmitRejection> {
    for need in required {
        let mut matches = provided
            .iter()
            .filter(|item| item.input_id == need.input_id);
        let Some(first) = matches.next() else {
            return Err(GateAdmitRejection::RequiredInputMissing {
                input_id: need.input_id.clone(),
            });
        };
        for item in std::iter::once(first).chain(matches) {
            if item.verdict != "pass" {
                return Err(GateAdmitRejection::InputGateNotGreen {
                    input_id: need.input_id.clone(),
                    verdict: item.verdict.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required(id: &str) -> RequiredGateInput {
        RequiredGateInput {
            input_id: id.into(),
        }
    }

    fn provided(id: &str, verdict: &str) -> ProvidedGateInput {
        ProvidedGateInput {
            input_id: id.into(),
            verdict: verdict.into(),
        }
    }

    #[test]
    fn missing_required_gate_artifact_is_rejected() {
        let err = validate_required_gate_inputs(&[required("f1")], &[]).unwrap_err();
        assert_eq!(
            err,
            GateAdmitRejection::RequiredInputMissing {
                input_id: "f1".into()
            }
        );
    }

    #[test]
    fn fail_needs_human_and_unknown_verdicts_are_not_green() {
        for verdict in ["fail", "needs_human", "PASS", "", "unknown"] {
            let err = validate_required_gate_inputs(&[required("f1")], &[provided("f1", verdict)])
                .unwrap_err();
            assert_eq!(
                err,
                GateAdmitRejection::InputGateNotGreen {
                    input_id: "f1".into(),
                    verdict: verdict.into(),
                }
            );
        }
    }

    #[test]
    fn only_all_required_green_inputs_admit() {
        validate_required_gate_inputs(
            &[required("f1"), required("security")],
            &[provided("f1", "pass"), provided("security", "pass")],
        )
        .unwrap();
    }

    #[test]
    fn same_name_green_cannot_wash_a_red_duplicate() {
        let err = validate_required_gate_inputs(
            &[required("f1")],
            &[provided("f1", "pass"), provided("f1", "fail")],
        )
        .unwrap_err();
        assert!(matches!(err, GateAdmitRejection::InputGateNotGreen { .. }));
    }
}
