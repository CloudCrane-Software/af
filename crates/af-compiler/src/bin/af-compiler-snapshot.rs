//! `af-compiler-snapshot <yaml>`：DSL 文档 → IR JSON（stdout）。
//!
//! G3 golden 入库入口。装载纪律（与库输入契约一致）：load → validate（全量
//! 零违例才继续）→ lower → pretty 序列化。失败路径全部 stderr 点名 + 非零
//! 退出码，不静默降级。

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _prog = args.next();
    let path = match (args.next(), args.next()) {
        (Some(p), None) => p,
        _ => {
            eprintln!("用法：af-compiler-snapshot <yaml 文档路径>");
            return ExitCode::from(2);
        }
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读取 {path:?} 失败：{e}");
            return ExitCode::from(2);
        }
    };
    let doc = match af_dsl::load_str(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("装载拒绝：{e}");
            return ExitCode::FAILURE;
        }
    };
    let report = af_dsl::validate(&doc);
    if !report.is_ok() {
        eprintln!("语义校验未过（lowering 输入契约 = 全量零违例）：{report}");
        return ExitCode::FAILURE;
    }
    let json = match af_compiler::snapshot_json_pretty(&doc) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("lowering 失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::io::stdout().lock();
    if let Err(e) = writeln!(out, "{json}") {
        eprintln!("stdout 写入失败：{e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
