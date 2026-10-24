//! 执行单元接口（对齐 落地契约-v0.md §1 方向，PoC 不求全）：
//! 可被统一执行面声明、拉起、观测、记账的最小工作体。
//!
//! v0 只落进程内 trait 形态；unit.json 声明文件、host_form 注册表、
//! `af.plan.record.v1` 结果行契约是后续任务包（见 RESULT.md 差距节）。
//! 执行单元自身不持有工单状态——上下文只给只读定位信息与产出目录。

use std::fs;
use std::path::{Path, PathBuf};

use af_workorder::WorkArtifact;

use crate::error::ExeclineError;
use crate::record::RecordSink;

/// 执行上下文。
///
/// P4 扩展（吸收方案 §2.2"ExecContext 增字段"）：每单执行期输入——"同一档跑
/// 不同题"的东西进这里，不进单元声明。判据一句话（方案原文）：换档案/换档位
/// =换 spec；换题/换宿主/换输出根=换 ctx。`task_id` 为表列之外的补充字段：
/// 方案 §2.2 判据明言"换题=换 ctx"，题目标识因此进 ctx（单元从
/// `<data_root>/<task_id>.json` 装载任务包）。attempt 目录的驱动层分配与
/// RecordSink 的文件实现是 P5（方案 §5-P5）；P4 直调单元的调用方自备。
pub struct ExecContext<'a> {
    pub order_id: &'a str,
    pub stable_id: &'a str,
    pub line_id: &'a str,
    /// 工件产出目录（执行面创建并传入）。
    pub out_dir: &'a Path,
    /// 目标题标识（swe-v = instance_id；换题=换 ctx）。
    pub task_id: &'a str,
    /// 执行宿主别名（anolis/local），缺省 anolis（main.rs:102）。
    pub host_alias: &'a str,
    /// 题库根（load task_json / eval_script；main.rs:640, 850）。
    pub data_root: &'a Path,
    /// run 目录（重跑递增不覆写，D-20；驱动层 next_attempt_index 分配=P5 接线）。
    pub attempt_dir: &'a Path,
    /// 结果行写入器（append+flush 语义，main.rs:644-648）。
    pub record_sink: &'a dyn RecordSink,
    /// 试验标记（不进决策，main.rs:11）。
    pub trial: bool,
}

/// 执行产出：工件（进订单）+ 产出文件（进证据目录）+ 备注。
///
/// P4 扩展（吸收方案 §2.2"ExecOutput 增字段"）：判定值（只从 judge stdout
/// JSON 取）、本单元落的 `af.plan.record.v1` 行、判分三件套实际路径。
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub artifacts: Vec<WorkArtifact>,
    pub outputs: Vec<PathBuf>,
    pub note: String,
    /// 判定（resolved）；None = 无判定（供给面失败/未跑完）。
    pub verdict: Option<bool>,
    /// 本单元落的 `af.plan.record.v1` 行（失败路径无 v1 投影 → None）。
    pub record: Option<serde_json::Value>,
    /// 判分三件套实际路径（task_json 引用 / eval.log / agent.diff）。
    pub evidence: Vec<PathBuf>,
}

/// 执行单元。
pub trait ExecuteUnit {
    fn unit_id(&self) -> &str;

    /// 执行。失败 = 类型化错误（EX3001），由驱动层记账后向上传播；
    /// 不 panic、不吞错。
    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError>;
}

/// 执行桩：把一个工件落成 out_dir 下的 JSON 文件并登记。
/// PoC 专用——证明"执行可编程耦合订单语义"，不接真实执行宿主
/// （容器 / LLM / ssh 都不在本 PoC 范围）。
#[derive(Debug, Clone)]
pub struct StubExecuteUnit {
    pub artifact_id: String,
    pub is_final: bool,
    pub qualified: bool,
}

impl StubExecuteUnit {
    /// 合格终态工件（close 门控材料）。
    pub fn qualified_final(artifact_id: impl Into<String>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            is_final: true,
            qualified: true,
        }
    }
}

impl ExecuteUnit for StubExecuteUnit {
    fn unit_id(&self) -> &str {
        "stub-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        fs::create_dir_all(ctx.out_dir)?;
        let artifact_path = ctx.out_dir.join(format!("{}.json", self.artifact_id));
        let payload = serde_json::json!({
            "unit_id": self.unit_id(),
            "order_id": ctx.order_id,
            "stable_id": ctx.stable_id,
            "line_id": ctx.line_id,
            "artifact": {
                "artifact_id": self.artifact_id,
                "is_final": self.is_final,
                "qualified": self.qualified,
            },
        });
        let text =
            serde_json::to_string_pretty(&payload).map_err(|err| ExeclineError::UnitFailed {
                detail: format!("工件载荷序列化失败: {err}"),
            })?;
        fs::write(&artifact_path, format!("{text}\n"))?;
        Ok(ExecOutput {
            artifacts: vec![WorkArtifact {
                artifact_id: self.artifact_id.clone(),
                is_final: self.is_final,
                qualified: self.qualified,
            }],
            outputs: vec![artifact_path],
            note: format!("stub 单元产出工件 {}", self.artifact_id),
            verdict: None,
            record: None,
            evidence: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_writes_artifact_file_and_registers_it() {
        let dir = std::env::temp_dir().join(format!(
            "af-execline-exec-unit-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let unit = StubExecuteUnit::qualified_final("fa-wo-exec");
        let sink = crate::record::PendingRecordSink;
        let ctx = ExecContext {
            order_id: "wo-exec",
            stable_id: "sid-wo-exec",
            line_id: "line-poc",
            out_dir: &dir,
            task_id: "astropy__astropy-12907",
            host_alias: "local",
            data_root: &dir,
            attempt_dir: &dir,
            record_sink: &sink,
            trial: false,
        };
        let output = unit.execute(&ctx).unwrap();
        assert_eq!(output.artifacts.len(), 1);
        assert!(output.artifacts[0].is_final && output.artifacts[0].qualified);
        assert_eq!(output.artifacts[0].artifact_id, "fa-wo-exec");
        assert_eq!(output.outputs.len(), 1);
        assert!(output.outputs[0].is_file(), "工件文件必须落盘");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
