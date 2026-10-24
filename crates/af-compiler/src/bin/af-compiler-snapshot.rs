//! `af-compiler-snapshot [--registry <包根>] <yaml>`：DSL 文档 → IR JSON（stdout）。
//!
//! G3 golden 入库入口。装载纪律（与库输入契约一致）：load → validate（全量
//! 零违例才继续）→ lower → pretty 序列化。失败路径全部 stderr 点名 + 非零
//! 退出码，不静默降级。
//!
//! `--registry <包根>`（P2-L10，最小扩展）：装载 `af-dsl::registry` 包图清单
//! （扫描 `<包根>/<包>/manifest.json` 得 `declared_graphs`）并注入
//! `validate_with`——topology_selector 候选图核对走声明集而非默认空集。
//! 不给该旗标时行为与扩展前**逐字节不变**（`validate` 默认空集，E2001 照拒）。

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _prog = args.next();
    let mut registry_root: Option<std::ffi::OsString> = None;
    let mut path: Option<std::ffi::OsString> = None;
    while let Some(arg) = args.next() {
        if arg == "--registry" {
            match (args.next(), registry_root.is_none()) {
                (Some(root), true) => registry_root = Some(root),
                _ => {
                    eprintln!("--registry 需要且仅需要一个包根目录参数");
                    return ExitCode::from(2);
                }
            }
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("用法：af-compiler-snapshot [--registry <包根>] <yaml 文档路径>");
            return ExitCode::from(2);
        }
    }
    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("用法：af-compiler-snapshot [--registry <包根>] <yaml 文档路径>");
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
    let report = match &registry_root {
        Some(root) => {
            // registry 装载 → declared_graphs 注入 → validate_with（P2-L10）。
            let reg = match af_dsl::registry::Registry::load(root) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("registry 装载失败（{root:?}）：{e}");
                    return ExitCode::FAILURE;
                }
            };
            af_dsl::validate_with(&doc, &reg.options())
        }
        None => af_dsl::validate(&doc),
    };
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
