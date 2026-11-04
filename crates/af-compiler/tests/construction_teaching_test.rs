//! E1 缺口④：协议教学内置——编译器生成的任务书自动内嵌 declare/envelope
//! 协议教学段（锚 af-execline graph_unit.rs:24-32 的 typed envelope 交接
//! 语义）。通用任务不靠专用模板也会 declare：教学段 + 本任务的确切输出
//! 契约（端口 + 物化 schema 自编译产物提取）由编译链写进 task_book。

use af_compiler::{
    assemble_construction_package, compile_construction_plan, decode_plan, output_contract_block,
    task_book, verify_package_manifest, ConstructionSpec, ManifestVerification, PROTOCOL_TEACHING,
};

fn repo_packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn spec(task_id: &str) -> ConstructionSpec {
    ConstructionSpec {
        task_id: task_id.to_string(),
        statement: "建设 tools/factory-stat.py：读 quota.json 与 kb.db 汇总一行 JSON。".into(),
        acceptance: "unittest 全过（夹具数据，不碰真库）+ README 段落地。".into(),
        image: None,
        test_command: None,
        registry_root: repo_packages_root(),
    }
}

#[test]
fn task_book_embeds_protocol_teaching_and_exact_output_contract() {
    let spec = spec("E2G-TEACH-1");
    let plan = compile_construction_plan(&spec.registry_root).unwrap();
    let book = task_book(&spec, &plan).unwrap();

    // 教学段在场（缺口④落点）且先于任务陈述（模型先读协议再做活）。
    let teaching_at = book.find(PROTOCOL_TEACHING).expect("任务书应内嵌教学段");
    let task_at = book.find("[task]").expect("任务书应有任务段");
    let accept_at = book.find("[acceptance]").expect("任务书应有验收段");
    assert!(
        teaching_at < task_at && task_at < accept_at,
        "教学→任务→验收序"
    );

    // 教学段覆盖 declare/envelope 锚语义（graph_unit.rs:24-32 + declare 契约面）：
    for needle in [
        "declare",                // 交付工具 = declare（非 prose 总结）
        "artifact port",          // 端口名键控对象（envelope 键集 = 端口名）
        "ends the run",           // declare = 完成信号（环终止语义）
        "typed envelope",         // 类型化交接（schema 校验）
        "rejected with feedback", // 违例拒收给反馈（闭环重试）
    ] {
        assert!(book.contains(needle), "教学段应含 {needle:?}：{book}");
    }

    // 本任务的确切输出契约：deliverable 端口 + 物化 schema 自编译产物提取。
    assert!(book.contains("[OUTPUT CONTRACT"), "应带输出契约段：{book}");
    assert!(
        book.contains("\"deliverable\""),
        "契约应点名 deliverable 端口"
    );
    assert!(
        book.contains("\"tests_passed\""),
        "契约应含物化 schema 字段"
    );

    // 任务陈述与验收判据原样进书。
    assert!(book.contains(&spec.statement));
    assert!(book.contains(&spec.acceptance));
}

#[test]
fn output_contract_block_is_plan_derived_not_hardcoded() {
    let plan = compile_construction_plan(&repo_packages_root()).unwrap();
    let block = output_contract_block(&plan).unwrap();
    // 端口名/类型表达式来自编译产物的 node_exec/types（提取式，非硬编码串）。
    assert!(block.contains("declare with key \"deliverable\" (type deliverable)"));
    assert!(block.contains("\"files\"") && block.contains("\"tests_passed\""));
    // 计划身份与契约同源（同一编译产物）。
    assert!(plan
        .body
        .plan_id
        .starts_with("plan.factory_construction_build@1.0+"));
}

#[test]
fn assemble_package_end_to_end_teaching_package_verifies() {
    let dir = std::env::temp_dir().join(format!(
        "af-compiler-teach-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let spec = spec("E2G-TEACH-2");
    let manifest = assemble_construction_package(&dir, &spec).unwrap();

    // 三件齐备：plan（提交形态）/ task（教学任务书）/ manifest（内容留档）。
    let plan_src = std::fs::read_to_string(dir.join("plan.json")).unwrap();
    let plan = decode_plan(&plan_src).expect("组包 plan.json 应是可提交形态");
    assert_eq!(plan.body.plan_id, manifest.plan_id);

    let task: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("task.json")).unwrap()).unwrap();
    assert_eq!(task["task_id"], serde_json::json!("E2G-TEACH-2"));
    let book = task["task_book"].as_str().unwrap();
    assert!(book.contains(PROTOCOL_TEACHING), "组包任务书应内嵌教学段");
    // 通用字段面：零 SWE 字段（缺口③编译面同口径）。
    for swe in ["instance_id", "repo", "problem_statement"] {
        assert!(task.get(swe).is_none(), "task.json 不应要求 {swe}");
    }

    // 缺口②闭环：组包即留档，现场核对过（G3：核对过 = Verified 显式
    // 状态，与缺席 Absent 分形态）；改一字即拒（清态换号重提）。
    assert!(matches!(
        verify_package_manifest(&dir).unwrap(),
        ManifestVerification::Verified(_)
    ));
    let plan_bad = plan_src.replace("factory_construction_build", "factory_construction_build ");
    std::fs::write(dir.join("plan.json"), plan_bad).unwrap();
    let err = verify_package_manifest(&dir).unwrap_err();
    assert!(err.to_string().contains("清态换号重提"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
