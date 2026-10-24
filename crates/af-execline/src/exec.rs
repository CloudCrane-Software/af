//! 执行单元接口（对齐 落地契约-v0.md §1 方向，PoC 不求全）：
//! 可被统一执行面声明、拉起、观测、记账的最小工作体。
//!
//! v0 只落进程内 trait 形态；unit.json 声明文件、host_form 注册表、
//! `af.plan.record.v1` 结果行契约是后续任务包（见 RESULT.md 差距节）。
//! 执行单元自身不持有工单状态——上下文只给只读定位信息与产出目录。

use std::path::{Path, PathBuf};

use af_workorder::WorkArtifact;

use crate::error::ExeclineError;

/// 执行上下文。
pub struct ExecContext<'a> {
    pub order_id: &'a str,
    pub stable_id: &'a str,
    pub line_id: &'a str,
    /// 工件产出目录（执行面创建并传入）。
    pub out_dir: &'a Path,
}

/// 执行产出：工件（进订单）+ 产出文件（进证据目录）+ 备注。
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub artifacts: Vec<WorkArtifact>,
    pub outputs: Vec<PathBuf>,
    pub note: String,
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

    fn execute(&self, _ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        unimplemented!("RED：StubExecuteUnit::execute 待最小实现")
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
        let ctx = ExecContext {
            order_id: "wo-exec",
            stable_id: "sid-wo-exec",
            line_id: "line-poc",
            out_dir: &dir,
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
