//! CLI 入口。RED 桩：`run` 一律拒绝（转绿时落地参数解析与三个子命令）。

/// 执行一次 CLI。`args` 不含程序名；返回应打到 stdout 的文本，
/// 错误由 main 统一打 stderr 并定退出码。
pub fn run(_args: &[String]) -> Result<String, crate::error::OrderError> {
    Err(crate::error::OrderError::Unimplemented {
        detail: "af-order CLI 未实现（RED 桩）".into(),
    })
}
