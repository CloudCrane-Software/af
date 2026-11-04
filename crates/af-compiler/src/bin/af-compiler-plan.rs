//! `af-compiler-plan [--registry <包根>] [--diagnostics-out <产物路径>] <yaml>`：
//! DSL 文档 → 可执行计划 JSON（stage 1-4 全链：装载 → 校验 → lower →
//! bind → plan → stdout）。
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
//!
//! `--diagnostics-out <产物路径>`（G2 机读产物面）：把本次运行已编码的
//! 诊断（af-dsl E0xxx/E1xxx/E2xxx + E900x）按 `af.diagnostics.v1` schema
//! 落盘——码/信息/产生条件逐字来自既有判定面（见 `diagnostics` 模块），
//! 本旗标**只多一个出口**。**不给该旗标时行为与扩展前逐字节不变**（承
//! `--registry` P2-L10 同款纪律）。产物在源文档读取成功后的**每条**退出
//! 路径（成功与失败）都会写：空数组 ≠ 成功——无码的操作性失败（注册表
//! 装载失败等）同样落空数组，成败仍以退出码为准。产物写失败：stderr
//! 点名；失败路径管线退出码优先，成功路径按退出码 2（调用面错误）。

use std::io::Write;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!(
        "用法：af-compiler-plan [--registry <包根>] [--diagnostics-out <产物路径>] <yaml 文档路径>"
    );
    std::process::exit(2);
}

/// 落机读诊断产物（`--diagnostics-out` 在场时；不在场 = no-op）。
/// 写失败 stderr 点名返回 `false`——失败路径管线退出码优先（只记不夺），
/// 成功路径由调用方按退出码 2 处理。
fn emit_diagnostics(
    out: &Option<std::ffi::OsString>,
    source: &std::ffi::OsStr,
    diagnostics: Vec<af_compiler::Diagnostic>,
) -> bool {
    let Some(out) = out else {
        return true;
    };
    let mut file = af_compiler::DiagnosticsFile {
        schema: af_compiler::DIAGNOSTICS_SCHEMA.to_string(),
        diagnostics,
    };
    // 位置盖章：file = 源文档路径（非 UTF-8 走 lossy——CLI 面的已知定位，
    // 行/列无 span 源保持 null，不编造）。
    af_compiler::stamp_source_file(&mut file.diagnostics, &source.to_string_lossy());
    match file.write_to(std::path::Path::new(out)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("诊断产物写入失败（{out:?}）：{e}");
            false
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _prog = args.next();
    let mut registry_root: Option<std::ffi::OsString> = None;
    let mut diagnostics_out: Option<std::ffi::OsString> = None;
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
        } else if arg == "--diagnostics-out" {
            match (args.next(), diagnostics_out.is_none()) {
                (Some(out), true) => diagnostics_out = Some(out),
                _ => {
                    eprintln!("--diagnostics-out 需要且仅需要一个产物路径参数");
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
            // 机读产物：装载拒绝（E0xxx/E1xxx）逐条落盘；退出语义不变。
            let _ = emit_diagnostics(&diagnostics_out, &path, af_compiler::from_load_error(&e));
            eprintln!("装载拒绝：{e}");
            return ExitCode::FAILURE;
        }
    };
    let components = match &registry_root {
        Some(root) => {
            let report = match af_dsl::registry::Registry::load(root) {
                Ok(reg) => af_dsl::validate_with(&doc, &reg.options()),
                Err(e) => {
                    // 操作性失败（无码）——产物落空数组（成败以退出码为准）。
                    let _ = emit_diagnostics(&diagnostics_out, &path, Vec::new());
                    eprintln!("图清单装载失败（{root:?}）：{e}");
                    return ExitCode::FAILURE;
                }
            };
            if !report.is_ok() {
                let _ = emit_diagnostics(
                    &diagnostics_out,
                    &path,
                    af_compiler::from_validate_report(&report),
                );
                eprintln!("语义校验未过：{report}");
                return ExitCode::FAILURE;
            }
            match af_compiler::ComponentRegistry::load(root) {
                Ok(c) => c,
                Err(e) => {
                    // 操作性失败（无码）——同上：空数组 + stderr + 非零退出。
                    let _ = emit_diagnostics(&diagnostics_out, &path, Vec::new());
                    eprintln!("组件清单装载失败（{root:?}）：{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => {
            let report = af_dsl::validate(&doc);
            if !report.is_ok() {
                let _ = emit_diagnostics(
                    &diagnostics_out,
                    &path,
                    af_compiler::from_validate_report(&report),
                );
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
            // E900x 违例（Unbound）逐条落盘；Lower/Serialize 无码——空数组。
            let diags = match &e {
                af_compiler::PlanError::Unbound { violations } => {
                    af_compiler::from_bind_violations(violations)
                }
                _ => Vec::new(),
            };
            let _ = emit_diagnostics(&diagnostics_out, &path, diags);
            eprintln!("计划生成失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    // 成功路径：产物落空数组（无诊断 ≠ 空文件——schema 恒在场）；写失败
    // = 调用面错误（退出码 2），stdout 不输出残缺计划。
    if !emit_diagnostics(&diagnostics_out, &path, Vec::new()) {
        return ExitCode::from(2);
    }
    let mut out = std::io::stdout().lock();
    if let Err(e) = writeln!(out, "{json}") {
        eprintln!("stdout 写入失败：{e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
