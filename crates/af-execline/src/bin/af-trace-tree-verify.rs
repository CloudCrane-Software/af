//! `af-trace-tree-verify <订单目录>`：订单 trace 的 GenAI span 树核对 CLI。
//!
//! G5 前置工具（落地契约-v0 §4.2 语义投影口径）：读 af-state/orders/<id>/
//! 的执行产物，重建 invoke_workflow→invoke_agent→chat→execute_tool 四层树，
//! 断言逐层在场与父子关系，树形报告打印 stdout。exit 0 = 四层完整；
//! exit 1 = 有 findings（每条点名层级与证据）；exit 2 = 用法错。

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "用法：af-trace-tree-verify [--publish <状态面.json>] <订单目录（af-state/orders/<order-id>/）>"
        );
        return ExitCode::from(2);
    }
    let publish = if args[0] == "--publish" {
        args.remove(0);
        if args.len() < 2 {
            eprintln!("用法：af-trace-tree-verify --publish <状态面.json> <订单目录>");
            return ExitCode::from(2);
        }
        Some(PathBuf::from(args.remove(0)))
    } else {
        None
    };
    let dir = PathBuf::from(args.remove(0));
    if !dir.is_dir() {
        eprintln!("订单目录不存在或不是目录：{}", dir.display());
        return ExitCode::from(2);
    }
    let report = af_execline::tracetree::verify_order_dir(&dir);
    print!("{}", report.render());
    if let Some(state) = publish {
        match af_execline::span_produce::publish_order(&dir, &state) {
            Ok(n) => {
                eprintln!("published {n} order-keyed span rows → {}", state.display());
                if n == 0 {
                    return ExitCode::from(1);
                }
            }
            Err(err) => {
                eprintln!("publish 失败：{err}");
                return ExitCode::from(1);
            }
        }
        return ExitCode::SUCCESS;
    }
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
