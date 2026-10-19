//! afg — 图模板校验 CLI（PoC 演示用）。
//!
//! 用法：`afg validate <template.json> [more.json ...]`
//! 全部通过退出码 0，任一违例退出码 1。

use af_graph::{format_validate_stdout, validate, GraphTemplate};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("validate") || args.len() < 2 {
        eprintln!("用法: afg validate <template.json> [more.json ...]");
        return ExitCode::from(2);
    }

    let mut all_ok = true;
    for path in &args[1..] {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}: 读取失败: {e}");
                all_ok = false;
                continue;
            }
        };
        let t: GraphTemplate = match serde_json::from_str(&src) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: schema 解析失败（不是合法图模板）: {e}");
                all_ok = false;
                continue;
            }
        };
        let report = validate(&t);
        if !report.is_ok() {
            all_ok = false;
        }
        print!("{}", format_validate_stdout(path, &t, &report));
    }
    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
