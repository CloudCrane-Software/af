//! `af-trace-tree-verify <订单目录>`：订单 trace 的 GenAI span 树核对 CLI。
//!
//! G5 前置工具（落地契约-v0 §4.2 语义投影口径）：读 af-state/orders/<id>/
//! 的执行产物，重建 invoke_workflow→invoke_agent→chat→execute_tool 四层树，
//! 断言逐层在场与父子关系，树形报告打印 stdout。exit 0 = 四层完整；
//! exit 1 = 有 findings（每条点名层级与证据）；exit 2 = 用法错。

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(arg) = std::env::args_os().nth(1) else {
        eprintln!("用法：af-trace-tree-verify <订单目录（af-state/orders/<order-id>/）>");
        return ExitCode::from(2);
    };
    let dir = PathBuf::from(arg);
    if !dir.is_dir() {
        eprintln!("订单目录不存在或不是目录：{}", dir.display());
        return ExitCode::from(2);
    }
    let report = af_execline::tracetree::verify_order_dir(&dir);
    print!("{}", report.render());
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
