//! `af-compiler-plan [--registry <包根>] <yaml>`：DSL 文档 → 可执行计划 JSON
//! （stage 1-4 全链：装载 → 校验 → lower → bind → plan → stdout）。
//!
//! G2b 门入口。`--registry <包根>`（默认 `af/packages` 语义由调用方给出）
//! 一根两用：①`af-dsl::registry` 图清单 → `declared_graphs` 注入
//! `validate_with`（topology_selector 候选核对）；②`af-compiler::components`
//! 组件清单 → stage 3 绑定对照面。不给该旗标：校验走默认空集（selector
//! 稿必被 E2001 拒——fail-closed），组件注册表为空（任何 uses.* 引用必被
//! E9001 拒——fail-closed）。
//!
//! 失败路径全部 stderr 点名 + 非零退出码：装载拒绝 / 校验违例 / 组件清单
//! 装载错误 / E900x 绑定违例（逐条列出，一次收齐）/ 计划生成拒绝。绝不
//! 静默降级，绝不输出残缺计划。

use std::io::Write;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!("用法：af-compiler-plan [--registry <包根>] <yaml 文档路径>");
    std::process::exit(2);
}

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
            usage();
        }
    }
    let path = match path {
        Some(p) => p,
        None => usage(),
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读取 {path:?} 失败：{e}");
            return ExitCode::from(2);
        }
    };

    // stage 1：装载 + 校验（--registry 时注入 declared_graphs）。
    let doc = match af_dsl::load_str(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("装载拒绝：{e}");
            return ExitCode::FAILURE;
        }
    };
    let components = match &registry_root {
        Some(root) => {
            let report = match af_dsl::registry::Registry::load(root) {
                Ok(reg) => af_dsl::validate_with(&doc, &reg.options()),
                Err(e) => {
                    eprintln!("图清单装载失败（{root:?}）：{e}");
                    return ExitCode::FAILURE;
                }
            };
            if !report.is_ok() {
                eprintln!("语义校验未过：{report}");
                return ExitCode::FAILURE;
            }
            match af_compiler::ComponentRegistry::load(root) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("组件清单装载失败（{root:?}）：{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => {
            let report = af_dsl::validate(&doc);
            if !report.is_ok() {
                eprintln!("语义校验未过：{report}");
                return ExitCode::FAILURE;
            }
            af_compiler::ComponentRegistry::empty()
        }
    };

    // stage 2-4：lower → bind（fail-loud 逐条点名）→ plan → pretty 序列化。
    let json = match af_compiler::plan_json_pretty(&doc, &components) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("计划生成失败：{e}");
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
