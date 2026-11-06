//! strategies/ 策略库装载测试（libA-20261029；ci/run.sh step14 段尾 14d 的编译断言面）
//!
//! 契约（ strategies/README.md §一）：
//!   · 每策略一个 YAML（st-<簇id>.yaml），`dsl_state=compilable` 的条目其
//!     `dsl_snippet` 必须能被 af-dsl 全链接受：`load_str`（信封+七段形状）零错，
//!     `validate`（引用闭包+INV-1..4）零违例。
//!   · 三态自洽：compilable ⇒ snippet 非空；needs_syntax ⇒ note 非空；
//!     not_applicable ⇒ na_reason 非空。
//!   · `id` 与文件名 stem 一致（st-C008.yaml ↔ id: st-C008）。
//!
//! 环境注记（中立性门沙箱兼容）：step8 沙箱 cp 集只含 crates/ 与 af/（裁决19），
//! **不含 strategies/**——本测试在 strategies/ 缺席时如实跳过（打印 SKIP 行），
//! 不失败：隔离臂/对照臂的失败集契约（neutrality-expected-fail.txt 23 项）不含
//! 本测试键，skip 即零失败键；真实工作树（step3 cargo test 与 step14 14d）中
//! strategies/ 在场，断言全量执行。
//!
//! schema 面（字段闭合/枚举/计数对账）由 ci/strategies_check.py 负责；本测试只做
//! 装载+编译断言，两面合起来即 14d 的全量校验。

use af_dsl::{load_str, validate};
use std::fs;
use std::path::{Path, PathBuf};

fn strategies_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/crates/af-dsl → 仓根在上两级。
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CARGO_MANIFEST_DIR 应有上两级（crates/af-dsl）")
        .join("strategies")
}

fn as_nonempty_str(v: &serde_yaml::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).and_then(|s| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_string())
    })
}

#[test]
fn strategy_library_entries_compile() {
    let dir = strategies_dir();
    if !dir.is_dir() {
        // 沙箱/外部拷贝树无 strategies/（如中立性门沙箱 cp 集）——如实跳过。
        eprintln!(
            "[strategy-library] SKIP：strategies/ 缺席（{}）——非本仓完整工作树，编译断言不执行",
            dir.display()
        );
        return;
    }

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("读取 strategies/ 失败（{}）：{e}", dir.display()))
        .map(|e| e.expect("dirent").path())
        .filter(|p| {
            p.extension().map(|x| x == "yaml").unwrap_or(false)
                && p.file_name().and_then(|n| n.to_str()) != Some("index.yaml")
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "strategies/ 无条目 YAML（index.yaml 除外）——库不应为空（空库=覆盖面缩水）"
    );

    let mut compiled = 0usize;
    let mut needs_syntax = 0usize;
    let mut not_applicable = 0usize;

    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("file stem")
            .to_string();
        let src = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
        let doc: serde_yaml::Value = serde_yaml::from_str(&src)
            .unwrap_or_else(|e| panic!("{} 不是合法 YAML：{e}", path.display()));

        // id ↔ 文件名 stem 一致
        let id = as_nonempty_str(&doc, "id")
            .unwrap_or_else(|| panic!("{}：缺 id 或为空", path.display()));
        assert_eq!(
            id,
            stem,
            "{}：id 与文件名 stem 不一致（格式契约）",
            path.display()
        );

        let state = as_nonempty_str(&doc, "dsl_state")
            .unwrap_or_else(|| panic!("{id}：缺 dsl_state 或为空"));

        match state.as_str() {
            "compilable" => {
                let snippet = as_nonempty_str(&doc, "dsl_snippet").unwrap_or_else(|| {
                    panic!("{id}：dsl_state=compilable 但 dsl_snippet 缺席/为空")
                });
                let parsed = load_str(&snippet)
                    .unwrap_or_else(|e| panic!("{id}：dsl_snippet 不被 af-dsl 接受：{e}"));
                let report = validate(&parsed);
                assert!(
                    report.is_ok(),
                    "{id}：dsl_snippet 通过解析但语义校验有违例：\n{report}"
                );
                compiled += 1;
                println!("[strategy-library] COMPILE-OK {id}");
            }
            "needs_syntax" => {
                assert!(
                    as_nonempty_str(&doc, "needs_syntax_note").is_some(),
                    "{id}：dsl_state=needs_syntax 但 needs_syntax_note 缺席/为空"
                );
                needs_syntax += 1;
                println!("[strategy-library] NEEDS-SYNTAX {id}");
            }
            "not_applicable" => {
                assert!(
                    as_nonempty_str(&doc, "na_reason").is_some(),
                    "{id}：dsl_state=not_applicable 但 na_reason 缺席/为空"
                );
                not_applicable += 1;
                println!("[strategy-library] NOT-APPLICABLE {id}");
            }
            other => panic!(
                "{id}：dsl_state={other} 不在三态闭集（compilable/needs_syntax/not_applicable）"
            ),
        }
    }

    println!(
        "[strategy-library] 条目 {}：compilable={compiled} needs_syntax={needs_syntax} not_applicable={not_applicable}",
        files.len()
    );
}
