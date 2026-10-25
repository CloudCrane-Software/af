//! G4 软件工程包 v1 真实现验收测试（设计《G4-软件工程包设计-v1》§2.2/§2.3
//! 逐件；TDD 先红后绿——本文件先于 `src/pkg/` 实现提交，模块缺席时编译红）。
//!
//! 分级口径（裁决17）：15 件中 E2E 路径组件（T1 env/bash + U1 swebench_eval）
//! 真实现；其余 13 件单元级实测 = FakeBridge 命令断言（承 tests/p2_bridge.rs
//! 先例）+ 真容器抽样 ≥1 例/件（`#[ignore]` 段，anolis sweb.eval 镜像，
//! 起/清容器前 `docker ps` 避让——F-首单 §1 预检口径）。
//!
//! 模板机制（裁决10：两包唯一、解析器一套）测试住本文件：tmp 沙箱正例 +
//! 漂移/未登记/语法负例（EX4001-4003）。仓库正本包资产（sweng/prompts 与
//! MANIFEST.sha256 字节一致性）由 `pkg_sweng_package.rs` 验收（与包文件
//! 同 commit 落盘）。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use af_execline::error::ErrorCode;
use af_execline::hostbridge::{HostBridge, SshBridge};
use af_execline::pkg::prompts;
use af_execline::pkg::sweng;
use af_execline::suite::swev::{self, SwevTask};
use af_execline::suite::swev_unit::SwevUnitSpec;
use af_execline::suite::tb::TbEnv;
use serde_json::{json, Value};

type BridgeResult = Result<(bool, String), String>;

// ---- FakeBridge（p2_bridge.rs 同款；Mutex 化以满足 EnvBash 的 Send+Sync 持有）----

#[derive(Default)]
struct FakeBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
    scripted: Mutex<VecDeque<BridgeResult>>,
}

impl FakeBridge {
    fn new(scripted: Vec<BridgeResult>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            scripted: Mutex::new(scripted.into()),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> BridgeResult {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            host.to_string(),
            script.to_string(),
            timeout_sec,
        ));
        self.scripted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| Ok((true, String::new())))
    }
}

fn fake_env() -> TbEnv {
    TbEnv {
        container: "afbench-swev-django-django-11099-glm-cp".into(),
        task: "django__django-11099".into(),
        host: "anolis".into(),
        workdir: "/testbed".into(),
    }
}

fn swev_task() -> SwevTask {
    SwevTask {
        instance_id: "django__django-11099".into(),
        repo: "django/django".into(),
        problem_statement: "Fix the thing".into(),
        image: "swebench/sweb.eval.x86_64.django_1776.django-11099:latest".into(),
        eval_script: "echo eval".into(),
        hints_text: String::new(),
    }
}

// ============================================================
// 锚约定（裁决13：impl_ref 双形态入契约）
// ============================================================

#[test]
fn builtin_impl_ref_dual_form() {
    assert_eq!(
        sweng::builtin_impl_ref("pkg.sweagent.env"),
        "executor:builtin/pkg.sweagent.env@af-execline",
        "裁决13 正例形态（设计 §2.1 锚约定）"
    );
    assert_eq!(
        sweng::builtin_impl_ref("pkg.sweng.edit"),
        "executor:builtin/pkg.sweng.edit@af-execline"
    );
    assert!(sweng::is_builtin_impl(
        "executor:builtin/pkg.sweng.edit@af-execline"
    ));
    assert!(
        !sweng::is_builtin_impl("packages/sweng@1.0.0/tools/edit.py"),
        "包路径形态不经 builtin 判别"
    );
    assert_eq!(
        sweng::builtin_target("executor:builtin/pkg.sweng.edit@af-execline"),
        Some(("pkg.sweng.edit".to_string(), "af-execline".to_string()))
    );
    assert_eq!(
        sweng::builtin_target("packages/sweng@1.0.0/tools/edit.py"),
        None
    );
}

#[test]
fn sweng_v1_component_table_lists_exactly_fifteen() {
    assert_eq!(
        sweng::SWENG_V1_COMPONENTS.len(),
        15,
        "15 件真实现件（设计 §2）"
    );
    // 11 tool + 4 unit（设计 §2.2/§2.3 分级）；逐名与注册面声明串一致。
    let tools = [
        sweng::REF_ENV,
        sweng::REF_ACI_SEARCH,
        sweng::REF_ACI_OPEN,
        sweng::REF_ACI_EDIT,
        sweng::REF_ACI_CREATE,
        sweng::REF_ACI_RUN_TEST,
        sweng::REF_ACI_SUBMIT,
        sweng::REF_SWENG_LOCATE,
        sweng::REF_SWENG_VIEW,
        sweng::REF_SWENG_EDIT,
        sweng::REF_SWENG_PUBLISH,
    ];
    let units = [
        sweng::REF_SWEBENCH_EVAL,
        sweng::REF_ACI_SCHEMA,
        sweng::REF_SWENG_TEST_UNIT,
        sweng::REF_HUMAN_GATE,
    ];
    for r in tools.iter().chain(units.iter()) {
        assert!(sweng::SWENG_V1_COMPONENTS.contains(r), "组件表缺 {r}");
        assert_eq!(
            sweng::builtin_impl_ref(sweng::name_of(r)),
            format!("executor:builtin/{}@af-execline", sweng::name_of(r))
        );
    }
}

// ============================================================
// ToolDecl 表（模型面声明；声明面/模型面双轨——设计 §2.1）
// ============================================================

#[test]
fn tool_decl_table_covers_eleven_tools_with_registry_effects() {
    let env = fake_env();
    // T1（bash 先例直接复用）：name=bash、effect=external_once、描述含 workdir。
    let bash = sweng::tool_decl(sweng::REF_ENV, &env.workdir).expect("T1 应有 ToolDecl");
    assert_eq!(bash.name, "bash");
    assert_eq!(bash.effect, af_harness::EffectClass::ExternalOnce);
    assert!(bash.description.contains("/testbed"));

    // pure 件（注册面 effect_class=pure ↔ ToolDecl.effect=Pure）。
    for r in [
        sweng::REF_ACI_SEARCH,
        sweng::REF_ACI_OPEN,
        sweng::REF_SWENG_LOCATE,
        sweng::REF_SWENG_VIEW,
    ] {
        let d = sweng::tool_decl(r, &env.workdir).unwrap_or_else(|| panic!("{r} 缺 ToolDecl"));
        assert_eq!(d.effect, af_harness::EffectClass::Pure, "{r}");
        assert!(!d.name.is_empty());
    }
    // persist 件。
    for r in [
        sweng::REF_ACI_EDIT,
        sweng::REF_ACI_CREATE,
        sweng::REF_ACI_SUBMIT,
        sweng::REF_SWENG_EDIT,
        sweng::REF_SWENG_PUBLISH,
    ] {
        let d = sweng::tool_decl(r, &env.workdir).unwrap_or_else(|| panic!("{r} 缺 ToolDecl"));
        assert_eq!(d.effect, af_harness::EffectClass::Persist, "{r}");
    }
    // external_once 件。
    for r in [sweng::REF_ENV, sweng::REF_ACI_RUN_TEST] {
        let d = sweng::tool_decl(r, &env.workdir).unwrap();
        assert_eq!(d.effect, af_harness::EffectClass::ExternalOnce, "{r}");
    }
    // ToolDecl 名字唯一（防同表撞名）。
    let mut names: Vec<String> = Vec::new();
    for r in sweng::SWENG_V1_COMPONENTS {
        if let Some(d) = sweng::tool_decl(r, &env.workdir) {
            names.push(d.name.clone());
        }
    }
    let uniq: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(names.len(), 11);
    assert_eq!(uniq.len(), 11, "11 件 tool 名字互异");
    // unit 类与未知引用 → None（unit 无模型面声明）。
    assert!(sweng::tool_decl(sweng::REF_SWEBENCH_EVAL, &env.workdir).is_none());
    assert!(sweng::tool_decl("pkg.nope.tool@1", &env.workdir).is_none());
}

// ============================================================
// T1 pkg.sweagent.env（E2E 主链真实现：bash_tool 先例直接复用）
// ============================================================

#[test]
fn t1_env_bash_executes_via_bridge_with_default_60s_timeout() {
    let bridge = Arc::new(FakeBridge::new(vec![Ok((true, "hi\n__AF_RC=0".into()))]));
    let env = Arc::new(fake_env());
    let tool = sweng::EnvBash::new(bridge.clone(), env);
    let out = tool.call("echo hi", None);
    assert_eq!(
        out, "hi\n\nexit=0",
        "split_rc 正文保留尾换行（p1_suite 黄金契约），exit 行另起"
    );
    let calls = bridge.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "anolis");
    assert_eq!(
        calls[0].1,
        "docker exec afbench-swev-django-django-11099-glm-cp bash -lc 'cd /testbed && echo hi'; echo __AF_RC=$?"
    );
    assert_eq!(calls[0].2, 60, "缺省 timeout_sec=60（设计 §2.2-T1）");
}

#[test]
fn t1_env_bash_honors_custom_timeout() {
    let bridge = Arc::new(FakeBridge::new(vec![Ok((true, "__AF_RC=0".into()))]));
    let tool = sweng::EnvBash::new(bridge.clone(), Arc::new(fake_env()));
    let _ = tool.call("make test", Some(45));
    assert_eq!(bridge.calls()[0].2, 45);
}

#[test]
fn t1_env_bash_guard_semantics_archaeology_and_repeat() {
    let scripted = vec![
        Ok((true, "o\n__AF_RC=0".into())), // echo hi（首调用）
        Ok((true, "o\n__AF_RC=0".into())), // git fsck（反考古注入）
        Ok((true, "b\n__AF_RC=0".into())), // dup ×3（第三次拦截）
        Ok((true, "b\n__AF_RC=0".into())),
        Ok((true, "b\n__AF_RC=0".into())),
    ];
    let bridge = Arc::new(FakeBridge::new(scripted));
    let tool = sweng::EnvBash::new(bridge.clone(), Arc::new(fake_env()));
    assert_eq!(tool.call("echo hi", None), "o\n\nexit=0");
    let arch = tool.call("git fsck --lost-found", None);
    assert!(
        arch.starts_with("[harness] Searching for an upstream fix"),
        "反考古注入前缀（swev_unit 护栏原样）：{arch}"
    );
    let _ = tool.call("dup", None);
    let _ = tool.call("dup", None);
    let third = tool.call("dup", None);
    assert!(
        third.starts_with("[harness] You have issued this exact command 3 times."),
        "第 3 次同命令拦截教学：{third}"
    );
    assert!(third.ends_with("b\n\nexit=0"), "原输出保留：{third}");
    assert_eq!(bridge.calls().len(), 5);
}

// ============================================================
// T2/T8 只读检索（pure：grep -rnE；命令自构即只读白名单）
// ============================================================

#[test]
fn t2_aci_search_builds_readonly_grep_with_defaults() {
    let bridge = FakeBridge::new(vec![Ok((
        true,
        "django/__init__.py:1:hit\n__AF_RC=0".into(),
    ))]);
    let env = fake_env();
    let out = sweng::aci_search(&bridge, &env, "VERSION", None, None);
    assert_eq!(out, "django/__init__.py:1:hit\n\nexit=0");
    let calls = bridge.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].2, 60);
    assert_eq!(
        calls[0].1,
        "docker exec afbench-swev-django-django-11099-glm-cp bash -lc 'cd /testbed && grep -rnE -I -- '\\''VERSION'\\'' '\\''/testbed'\\'' | head -n 50'; echo __AF_RC=$?",
        "缺省 path=workdir、max_hits=50；模式与路径经 shell_quote"
    );
}

#[test]
fn t2_aci_search_custom_path_and_max_hits() {
    let bridge = FakeBridge::new(vec![Ok((true, "__AF_RC=0".into()))]);
    let env = fake_env();
    let _ = sweng::aci_search(
        &bridge,
        &env,
        "class Model",
        Some("django/db/models"),
        Some(5),
    );
    assert_eq!(
        bridge.calls()[0].1,
        "docker exec afbench-swev-django-django-11099-glm-cp bash -lc 'cd /testbed && grep -rnE -I -- '\\''class Model'\\'' '\\''django/db/models'\\'' | head -n 5'; echo __AF_RC=$?"
    );
}

#[test]
fn t8_sweng_locate_is_search_with_query_param() {
    let bridge = FakeBridge::new(vec![Ok((true, "x.py:2:y\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::sweng_locate(&bridge, &env, "settings", None, None);
    assert_eq!(out, "x.py:2:y\n\nexit=0");
    assert!(
        bridge.calls()[0]
            .1
            .contains("grep -rnE -I -- '\\''settings'\\'' '\\''/testbed'\\'' | head -n 50"),
        "{}",
        bridge.calls()[0].1
    );
}

// ============================================================
// T3/T9 只读取片段（pure：awk 行号投影；越界报错观察）
// ============================================================

#[test]
fn t3_aci_open_builds_numbered_range_read() {
    let bridge = FakeBridge::new(vec![Ok((true, "1\thello\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::aci_open(&bridge, &env, "x.py", Some(1), Some(10));
    assert_eq!(out, "1\thello\n\nexit=0");
    assert_eq!(
        bridge.calls()[0].1,
        "docker exec afbench-swev-django-django-11099-glm-cp bash -lc 'cd /testbed && awk '\\''NR>=1 && NR<=10 { print NR \"\\t\" $0 }'\\'' '\\''x.py'\\'''; echo __AF_RC=$?"
    );
}

#[test]
fn t3_aci_open_out_of_range_is_error_observation() {
    let bridge = FakeBridge::new(vec![Ok((true, "__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::aci_open(&bridge, &env, "x.py", Some(900), Some(999));
    assert!(
        out.starts_with("ERROR: no lines in range 900..999 for x.py"),
        "越界/空读报错观察（设计 §2.2-T3）：{out}"
    );
}

#[test]
fn t3_aci_open_rejects_invalid_range_without_bridge_call() {
    let bridge = FakeBridge::new(vec![]);
    let env = fake_env();
    let out = sweng::aci_open(&bridge, &env, "x.py", Some(0), Some(5));
    assert!(out.starts_with("ERROR: invalid line range"));
    let out = sweng::aci_open(&bridge, &env, "x.py", Some(9), Some(5));
    assert!(out.starts_with("ERROR: invalid line range"));
    assert!(bridge.calls().is_empty(), "非法区间不触容器");
}

#[test]
fn t9_sweng_view_defaults_and_range() {
    let bridge = FakeBridge::new(vec![Ok((true, "7\tline\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::sweng_view(&bridge, &env, "y.py", Some(7), Some(7));
    assert_eq!(out, "7\tline\n\nexit=0");
    assert!(
        bridge.calls()[0]
            .1
            .contains("awk '\\''NR>=7 && NR<=7 { print NR \"\\t\" $0 }'\\'' '\\''y.py'\\''"),
        "{}",
        bridge.calls()[0].1
    );
    // 缺省 1..start+99（v1 口径，登记于设计 §2.2-T3 参数语义）。
    let bridge2 = FakeBridge::new(vec![Ok((true, "__AF_RC=0".into()))]);
    let _ = sweng::sweng_view(&bridge2, &env, "y.py", None, None);
    assert!(
        bridge2.calls()[0]
            .1
            .contains("awk '\\''NR>=1 && NR<=100 { print NR \"\\t\" $0 }'\\'' '\\''y.py'\\''"),
        "{}",
        bridge2.calls()[0].1
    );
}

// ============================================================
// T4/T10 软对齐编辑（persist：old_text 恰一次才替换）
// ============================================================

#[test]
fn t4_aci_edit_carries_json_payload_via_python() {
    let bridge = FakeBridge::new(vec![Ok((true, "OK /tmp/a.py\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::aci_edit(&bridge, &env, "/tmp/a.py", "OLD", "NEW");
    assert_eq!(out, "OK /tmp/a.py\n\nexit=0");
    let script = &bridge.calls()[0].1;
    assert!(script.contains("python3 -c "), "python3 载体：{script}");
    assert!(
        script.contains(r#""path":"/tmp/a.py""#)
            && script.contains(r#""old":"OLD""#)
            && script.contains(r#""new":"NEW""#),
        "载荷 JSON 逐字段可见：{script}"
    );
    assert!(bridge.calls()[0].2 == 60);
}

#[test]
fn t4_aci_edit_conflict_observations_pass_through() {
    let env = fake_env();
    // 0 匹配。
    let bridge = FakeBridge::new(vec![Ok((
        false,
        "ERROR 0 matches for old_text in /tmp/a.py (need exactly 1)\n__AF_RC=1".into(),
    ))]);
    let out = sweng::aci_edit(&bridge, &env, "/tmp/a.py", "OLD", "NEW");
    assert!(out.contains("ERROR 0 matches"), "{out}");
    assert!(out.ends_with("exit=1"));
    // 多匹配（>1）。
    let bridge = FakeBridge::new(vec![Ok((
        false,
        "ERROR 2 matches for old_text in /tmp/a.py (need exactly 1)\n__AF_RC=1".into(),
    ))]);
    let out = sweng::aci_edit(&bridge, &env, "/tmp/a.py", "OLD", "NEW");
    assert!(out.contains("ERROR 2 matches"), "{out}");
}

#[test]
fn t10_sweng_edit_same_contract_as_t4() {
    let bridge = FakeBridge::new(vec![Ok((true, "OK b.py\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::sweng_edit(&bridge, &env, "b.py", "x = 1", "x = 2");
    assert_eq!(out, "OK b.py\n\nexit=0");
    let script = &bridge.calls()[0].1;
    assert!(
        script.contains("python3 -c ") && script.contains(r#""old":"x = 1""#),
        "{script}"
    );
}

// ============================================================
// T5 新建文件（persist：父目录须在、已存在报错）
// ============================================================

#[test]
fn t5_aci_create_payload_and_error_paths() {
    let env = fake_env();
    let bridge = FakeBridge::new(vec![Ok((true, "OK /tmp/new.py\n__AF_RC=0".into()))]);
    let out = sweng::aci_create(&bridge, &env, "/tmp/new.py", "print(1)\n");
    assert_eq!(out, "OK /tmp/new.py\n\nexit=0");
    let script = &bridge.calls()[0].1;
    assert!(script.contains("python3 -c "), "{script}");
    assert!(
        script.contains(r#""path":"/tmp/new.py""#) && script.contains(r#""content":"print(1)\n""#),
        "content 原文入载荷：{script}"
    );

    let bridge = FakeBridge::new(vec![Ok((
        false,
        "ERROR file exists: /tmp/new.py\n__AF_RC=1".into(),
    ))]);
    let out = sweng::aci_create(&bridge, &env, "/tmp/new.py", "x");
    assert!(out.contains("ERROR file exists"), "{out}");

    let bridge = FakeBridge::new(vec![Ok((
        false,
        "ERROR parent directory missing: /nope\n__AF_RC=1".into(),
    ))]);
    let out = sweng::aci_create(&bridge, &env, "/nope/f.py", "x");
    assert!(out.contains("ERROR parent directory missing"), "{out}");
}

// ============================================================
// T6 测试执行（external_once：缺省取环境 test_command、300s）
// ============================================================

#[test]
fn t6_aci_run_test_defaults_to_env_handle_command() {
    let bridge = FakeBridge::new(vec![Ok((true, "Ran 3 tests\nOK\n__AF_RC=0".into()))]);
    let env = fake_env();
    let out = sweng::aci_run_test(&bridge, &env, "./runtests.py", None, None);
    assert_eq!(out, "Ran 3 tests\nOK\n\nexit=0");
    let calls = bridge.calls();
    assert_eq!(calls[0].2, 300, "缺省 timeout 300（设计 §2.2-T6）");
    assert!(
        calls[0]
            .1
            .ends_with("bash -lc 'cd /testbed && ./runtests.py'; echo __AF_RC=$?"),
        "{}",
        calls[0].1
    );
}

#[test]
fn t6_aci_run_test_override_cmd_and_timeout() {
    let bridge = FakeBridge::new(vec![Ok((true, "__AF_RC=0".into()))]);
    let env = fake_env();
    let _ = sweng::aci_run_test(
        &bridge,
        &env,
        "./runtests.py",
        Some("python -m pytest x"),
        Some(30),
    );
    let calls = bridge.calls();
    assert_eq!(calls[0].2, 30);
    assert!(calls[0].1.contains("python -m pytest x"), "{}", calls[0].1);
}

// ============================================================
// T7 提交（persist：TASK_COMPLETE 首行 + 取证 diff 定稿）
// ============================================================

#[test]
fn t7_aci_submit_returns_sentinel_and_captures_diff() {
    let diff_text = "diff --git a/x.py b/x.py\n--- a/x.py\n";
    let bridge = FakeBridge::new(vec![
        // capture_diff 的 `git diff` 调用不附 __AF_RC 标记（swev.rs:192-195
        // 命令构造原样）——脚本化响应即纯 diff 文本。
        Ok((true, diff_text.to_string())),
    ]);
    let env = fake_env();
    let work = std::env::temp_dir().join(format!("pkg-sweng-t7-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let out = sweng::aci_submit(&bridge, &env, &work, Some("fixed the query"));
    assert!(
        out.lines().next() == Some("TASK_COMPLETE"),
        "首行哨兵（StopCondition 命中面）：{out}"
    );
    assert!(out.contains("fixed the query"));
    assert!(out.contains("agent.diff"), "diff 摘要行：{out}");
    let on_disk = std::fs::read_to_string(work.join("agent.diff")).unwrap();
    assert_eq!(
        on_disk, diff_text,
        "取证 diff 定稿入 workspace 工件（设计 §2.2-T7）"
    );
    assert!(
        bridge.calls()[0]
            .1
            .contains("git -c core.fileMode=false diff"),
        "{}",
        bridge.calls()[0].1
    );
    std::fs::remove_dir_all(&work).ok();
}

#[test]
fn t7_aci_submit_survives_diff_failure_with_sentinel_intact() {
    let bridge = FakeBridge::new(vec![Err("bridge down".into())]);
    let env = fake_env();
    let work = std::env::temp_dir().join(format!("pkg-sweng-t7b-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let out = sweng::aci_submit(&bridge, &env, &work, None);
    assert_eq!(
        out.lines().next(),
        Some("TASK_COMPLETE"),
        "完成信号语义不因取证失败回撤：{out}"
    );
    assert!(out.contains("capture failed"), "{out}");
    std::fs::remove_dir_all(&work).ok();
}

// ============================================================
// T11 发布完成（persist：node_event(done) 行 + 工件登记幂等）
// ============================================================

#[test]
fn t11_publish_writes_node_event_and_is_idempotent() {
    let work = std::env::temp_dir().join(format!("pkg-sweng-t11-{}", std::process::id()));
    std::fs::remove_dir_all(&work).ok();
    std::fs::create_dir_all(&work).unwrap();
    let first = sweng::publish_task_finished(
        &work,
        "build.engineer",
        "done: query fixed",
        Some("agent.diff"),
    )
    .expect("发布应成功");
    assert_eq!(first, "PUBLISHED build.engineer-finish");
    let path = work.join("node-events.jsonl");
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content.lines().count(), 1, "首次发布恰 1 行：{content}");
    let row: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(row["event"], "node_event");
    assert_eq!(row["kind"], "done");
    assert_eq!(row["node"], "build.engineer");
    assert_eq!(row["event_id"], "build.engineer/done");
    assert_eq!(row["summary"], "done: query fixed");
    assert_eq!(row["patch_ref"], "agent.diff");
    assert_eq!(row["artifact"]["artifact_id"], "build.engineer-finish");

    // 同节点二次发布：event_id 去重，不重复落行（幂等，承 mes-ch-shipped marker 语义）。
    let second = sweng::publish_task_finished(
        &work,
        "build.engineer",
        "done: query fixed",
        Some("agent.diff"),
    )
    .expect("重放发布应成功");
    assert_eq!(second, "PUBLISHED build.engineer-finish");
    let content2 = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content2.lines().count(), 1, "重放不追加：{content2}");

    // 另一节点：新行。
    let _ = sweng::publish_task_finished(&work, "qa.tester", "done: tests green", None).unwrap();
    let content3 = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content3.lines().count(), 2);
    std::fs::remove_dir_all(&work).ok();
}

// ============================================================
// U1 pkg.sweagent.swebench_eval（E2E 主链真实现：swev 判分链复用）
// ============================================================

#[test]
fn u1_swebench_eval_fingerprint_gate_fails_loud_before_any_container_work() {
    // 篡改判分锚 → EX3002 拒跑；判分链不启动（零容器调用）——指纹门牙齿
    // （judge.rs 双门之一；裁决 §六-17 判据 5 的单元级面）。
    let bridge = FakeBridge::new(vec![]);
    let env = fake_env();
    let judge = af_execline::JudgeLocator {
        python: PathBuf::from("/nonexistent/python"),
        script: PathBuf::from("/nonexistent/swev-judge.py"),
        script_sha256: "0".repeat(64),
    };
    let work = std::env::temp_dir().join(format!("pkg-sweng-u1-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let task_json = work.join("task.json");
    std::fs::write(&task_json, "{}").unwrap();
    let out = sweng::swebench_eval(&bridge, &env, &judge, &swev_task(), &work, &task_json);
    assert_eq!(
        out.resolved, None,
        "指纹门失败=供给面失败（null，非 false）"
    );
    assert!(out.verdict["resolved"].is_null(), "{}", out.verdict);
    assert!(
        out.verdict["error"]
            .as_str()
            .unwrap_or("")
            .contains("EX3002"),
        "判定错误透传判分链文案：{}",
        out.verdict
    );
    assert!(out.eval_sha256.is_none());
    assert!(
        bridge.calls().is_empty(),
        "指纹门先于取证/eval/judge——零容器调用"
    );
    std::fs::remove_dir_all(&work).ok();
}

#[test]
fn u1_swebench_eval_assembles_verdict_from_judge_json_passthrough() {
    // 判定装配（纯函数面）：judge stdout JSON 原样透传 + eval_sha256 注入，
    // Rust 侧零再解释（judge.rs 三原则 §3.2-3）；成功臂的全链行为由真容器
    // 抽样 real_swebench_eval_full_chain 承载。
    let note = "judge: {\"resolved\": true, \"log_parsed\": true, \"f2p_success\": 4, \"f2p_failure\": 0, \"p2p_success\": 7, \"p2p_failure\": 0}";
    let sha = "a".repeat(64);
    let v = sweng::verdict_from_verify(Some(true), note, Some(sha.clone()));
    assert_eq!(v["resolved"], true);
    assert_eq!(v["log_parsed"], true);
    assert_eq!(v["f2p_success"], 4);
    assert_eq!(v["f2p_failure"], 0);
    assert_eq!(v["p2p_success"], 7);
    assert_eq!(v["eval_sha256"], sha.as_str());
    // 供给面失败（无 judge JSON）→ resolved null + error 透传。
    let v = sweng::verdict_from_verify(None, "EX3002: 指纹不匹配", None);
    assert!(v["resolved"].is_null(), "{}", v);
    assert_eq!(v["error"], "EX3002: 指纹不匹配");
    assert!(v["eval_sha256"].is_null());
}

// ============================================================
// U2 pkg.sweagent.aci_schema（纯结构校验，零 LLM）
// ============================================================

#[test]
fn u2_aci_schema_accepts_wellformed_action() {
    let ok = sweng::aci_schema(
        &json!({"thought": "find the bug", "command": "grep -n x y.py", "submit": false}),
    );
    assert_eq!(ok["error"], "");
    assert_eq!(ok["submit"], false);
    let ok2 = sweng::aci_schema(
        &json!({"thought": "done", "command": "echo TASK_COMPLETE", "submit": true}),
    );
    assert_eq!(ok2["error"], "");
    assert_eq!(ok2["submit"], true);
}

#[test]
fn u2_aci_schema_rejects_missing_and_mistyped_fields() {
    let miss = sweng::aci_schema(&json!({"command": "ls", "submit": false}));
    assert!(
        miss["error"].as_str().unwrap().contains("thought"),
        "{}",
        miss
    );
    assert_eq!(miss["submit"], false);
    let typo = sweng::aci_schema(&json!({"thought": "x", "command": 42, "submit": false}));
    assert!(
        typo["error"].as_str().unwrap().contains("command"),
        "{}",
        typo
    );
    let bool_typo = sweng::aci_schema(&json!({"thought": "x", "command": "y", "submit": "yes"}));
    assert!(
        bool_typo["error"].as_str().unwrap().contains("submit"),
        "{}",
        bool_typo
    );
    let not_obj = sweng::aci_schema(&json!("just a string"));
    assert!(
        not_obj["error"].as_str().unwrap().contains("object"),
        "{}",
        not_obj
    );
}

// ============================================================
// U3 pkg.sweng.test_unit（判定只取进程退出码与输出文本，零模型）
// ============================================================

#[test]
fn u3_test_unit_verdict_from_exit_code_and_tail_lines() {
    let env = fake_env();
    // 通过臂：exit=0 → pass=true，无 failures。
    let bridge = FakeBridge::new(vec![Ok((true, "Ran 2 tests\nOK\n__AF_RC=0".into()))]);
    let v = sweng::test_unit(&bridge, &env, "./runtests.py", None);
    assert_eq!(v["pass"], true);
    assert_eq!(v["failures"], json!([]));

    // 失败臂：exit=1 → pass=false，failures=输出末行。
    let bridge = FakeBridge::new(vec![Ok((
        false,
        "FAIL test_a\nFAIL test_b\n__AF_RC=1".into(),
    ))]);
    let v = sweng::test_unit(&bridge, &env, "./runtests.py", None);
    assert_eq!(v["pass"], false);
    assert_eq!(v["failures"], json!(["FAIL test_a", "FAIL test_b"]));

    // 覆写命令优先（test_cmd 指定面）。
    let bridge = FakeBridge::new(vec![Ok((true, "ok\n__AF_RC=0".into()))]);
    let v = sweng::test_unit(
        &bridge,
        &env,
        "./runtests.py",
        Some("python -m pytest tests/x"),
    );
    assert_eq!(v["pass"], true);
    assert!(
        bridge.calls()[0].1.contains("python -m pytest tests/x"),
        "{}",
        bridge.calls()[0].1
    );
}

#[test]
fn u3_test_unit_caps_failure_lines_at_ten_and_handles_no_command() {
    let env = fake_env();
    let body: String = (0..15).map(|i| format!("FAIL case_{i}\n")).collect();
    let bridge = FakeBridge::new(vec![Ok((false, format!("{body}__AF_RC=1")))]);
    let v = sweng::test_unit(&bridge, &env, "cmd", None);
    let failures = v["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 10, "末 N=10 行：{failures:?}");
    assert_eq!(failures[0], "FAIL case_5");
    assert_eq!(failures[9], "FAIL case_14");

    // 命令源双缺 → fail-closed 观察而非崩溃。
    let bridge = FakeBridge::new(vec![]);
    let v = sweng::test_unit(&bridge, &env, "", None);
    assert_eq!(v["pass"], false);
    assert!(
        v["failures"][0]
            .as_str()
            .unwrap()
            .contains("no test command"),
        "{}",
        v
    );
    assert!(bridge.calls().is_empty());
}

// ============================================================
// U4 pkg.sweagent.human_gate（v1 fail-closed 最小实现）
// ============================================================

#[test]
fn u4_human_gate_fail_closed_both_configs() {
    // 裁决17/开放问题 4 已批：fail-closed 恒 {ask:false} 为 v1 口径；
    // enabled=true 无真工位接线（af-station 推 v2）时同样走默认分支。
    let disabled = sweng::HumanGateConfig::default();
    assert!(!disabled.enabled);
    let v = sweng::human_gate(&disabled, &json!({"progress": 0.5}));
    assert_eq!(v, json!({"ask": false}));

    let enabled = sweng::HumanGateConfig { enabled: true };
    let v = sweng::human_gate(&enabled, &json!({}));
    assert_eq!(v, json!({"ask": false}), "v1 恒不打扰人工（fail-closed）");
}

// ============================================================
// SwevUnitSpec 提示词装载化（设计 §3.2-3：&'static str → 装载包资产）
// ============================================================

#[test]
fn swev_unit_spec_prompts_are_owned_strings_equal_to_frozen_constants() {
    let spec = SwevUnitSpec::fix2("glm-cp", af_execline::JudgeLocator::production());
    // 装载化（设计 §3.2-3）：字段从 &'static str 改为 String 持有——包资产可
    // 整体替换装载；fix2 缺省仍字节等于冻结常量（测量语义载荷不动）。
    assert_eq!(spec.system_prompt, swev::SWEV_SYSTEM_PROMPT);
    assert_eq!(spec.instance_protocol, swev::SWEV_INSTANCE_PROTOCOL);
    let mut from_pkg = spec.clone();
    from_pkg.system_prompt = "pkg-loaded system".into();
    from_pkg.instance_protocol = "pkg-loaded protocol".into();
    assert_eq!(from_pkg.system_prompt, "pkg-loaded system");
    assert_eq!(from_pkg.instance_protocol, "pkg-loaded protocol");
}

// ============================================================
// template_ref 机制（裁决10：唯一机制，解析器一套）
// ============================================================

fn write_manifest(dir: &Path, version: &str, entries: &[(&Path, &str)]) {
    use std::fmt::Write as _;
    let mut text = String::new();
    let _ = writeln!(text, "# manifest_version: {version}");
    for (path, content) in entries {
        std::fs::write(path, content).unwrap();
        let sha = sha256_of(content);
        // 清单相对路径以包目录为基（prompts/<名>.md，与解析约定一致）。
        let _ = writeln!(
            text,
            "{sha}  prompts/{}",
            path.file_name().unwrap().to_string_lossy()
        );
    }
    std::fs::write(dir.join("MANIFEST.sha256"), text).unwrap();
}

fn sha256_of(data: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut out = String::new();
    for b in Sha256::digest(data.as_bytes()) {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn tmp_pkg_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("pkg-sweng-prompts-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("demo").join("prompts")).unwrap();
    root
}

#[test]
fn template_ref_parse_accepts_optional_version() {
    use prompts::parse_template_ref;
    assert_eq!(
        parse_template_ref("pkg.sweng.swev_system@1").unwrap(),
        ("sweng".into(), "swev_system".into(), Some("1".into()))
    );
    assert_eq!(
        parse_template_ref("pkg.reportkit.reader_system").unwrap(),
        ("reportkit".into(), "reader_system".into(), None)
    );
    // 分段名（报告包骨架未用，但与 uses.* 可选版本一致的宽松面）。
    assert_eq!(
        parse_template_ref("pkg.a.b.c@2").unwrap(),
        ("a".into(), "b.c".into(), Some("2".into()))
    );
}

#[test]
fn template_ref_parse_rejects_malformed_refs() {
    use prompts::parse_template_ref;
    for bad in [
        "swev_system@1",          // 无 pkg. 前缀
        "pkg.sweng",              // 缺名段
        "pkg..swev_system",       // 空包段
        "pkg.sweng.swev_system@", // 空版本段
        "pkg.sweng.a@1@2",        // 双 @
        "pkg.sweng./etc/passwd",  // 路径注入
        "pkg../secrets",          // 点点注入
    ] {
        let err = parse_template_ref(bad).unwrap_err();
        assert_eq!(err.code(), ErrorCode::TemplateRefInvalid, "{bad}: {err}");
        assert!(err.to_string().starts_with("EX4001"), "{bad}: {err}");
    }
}

#[test]
fn template_resolve_happy_path_and_version_check() {
    let root = tmp_pkg_root("ok");
    let prompts_dir = root.join("demo").join("prompts");
    write_manifest(
        &prompts_dir,
        "1",
        &[(&prompts_dir.join("sys.md"), "You are an agent.")],
    );
    // 无版本引用。
    let asset = prompts::resolve_template(&root, "pkg.demo.sys").unwrap();
    assert_eq!(asset.content, "You are an agent.");
    assert_eq!(asset.package, "demo");
    assert_eq!(asset.name, "sys");
    assert_eq!(asset.sha256, sha256_of("You are an agent."));
    // 版本一致引用通过；版本不符 → EX4003。
    let asset = prompts::resolve_template(&root, "pkg.demo.sys@1").unwrap();
    assert_eq!(asset.version.as_deref(), Some("1"));
    let err = prompts::resolve_template(&root, "pkg.demo.sys@2").unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateDrift);
    assert!(err.to_string().starts_with("EX4003"), "{err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn template_resolve_rejects_unregistered_tampered_and_missing() {
    let root = tmp_pkg_root("bad");
    let prompts_dir = root.join("demo").join("prompts");
    write_manifest(
        &prompts_dir,
        "1",
        &[(&prompts_dir.join("sys.md"), "payload")],
    );
    // 未登记名 → EX4002。
    let err = prompts::resolve_template(&root, "pkg.demo.nope").unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateUnregistered);
    assert!(err.to_string().starts_with("EX4002"), "{err}");
    // 篡改文件一字 → EX4003（漂移防护，同 E9004 拒绝式纪律）。
    std::fs::write(prompts_dir.join("sys.md"), "tampered").unwrap();
    let err = prompts::resolve_template(&root, "pkg.demo.sys").unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateDrift);
    assert!(err.to_string().starts_with("EX4003"), "{err}");
    // 清单缺失 → EX4002。
    std::fs::remove_file(prompts_dir.join("MANIFEST.sha256")).unwrap();
    let err = prompts::resolve_template(&root, "pkg.demo.sys").unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateUnregistered);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn template_manifest_malformed_line_rejected() {
    let root = tmp_pkg_root("mal");
    let prompts_dir = root.join("demo").join("prompts");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(
        prompts_dir.join("MANIFEST.sha256"),
        "# manifest_version: 1\nnot-a-manifest-line\n",
    )
    .unwrap();
    let err = prompts::resolve_template(&root, "pkg.demo.x").unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateUnregistered);
    assert!(err.to_string().contains("清单行不合法"), "{err}");
    std::fs::remove_dir_all(&root).ok();
}

// ============================================================
// 真容器抽样（#[ignore]；anolis + sweb.eval 镜像；docker ps 避让）
// ============================================================

/// 避让检查：anolis 上存在任何 afbench-* 容器（在役/残留）即跳过——
/// 不与他人抢宿主（F-首单 §1 预检口径；护栏常量校准见 tb.rs:80-100）。
fn anolis_clear_or_panic() -> TbEnv {
    let out = std::process::Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("anolis")
        .args(["docker", "ps", "-a", "--format", "{{.Names}}"])
        .output()
        .expect("ssh anolis 应可达");
    assert!(out.status.success(), "ssh anolis docker ps 失败");
    let names = String::from_utf8_lossy(&out.stdout);
    let busy: Vec<&str> = names
        .lines()
        .filter(|n| n.trim().starts_with("afbench-"))
        .collect();
    assert!(
        busy.is_empty(),
        "避让：anolis 存在 afbench-* 容器（{:?}），跳过真容器抽样",
        busy
    );
    let task = swev_task();
    swev::setup(&SshBridge, "anolis", &task, "glm-cp").expect("起真容器")
}

struct RealGuard(TbEnv);
impl Drop for RealGuard {
    fn drop(&mut self) {
        self.0.cleanup(&SshBridge);
    }
}

/// 真容器抽样 · 13 件单元级件（T2-T11 覆盖容器件 + U3；T1 真实现护栏在单测面，
/// 此处再抽 1 例真执行）。一件 ≥1 例，共一容器生命周期。
#[test]
#[ignore = "真容器：anolis + sweb.eval django-11099 镜像；显式 -- --ignored 运行"]
fn real_container_sampling_tools_and_units() {
    let tb = anolis_clear_or_panic();
    let _guard = RealGuard(tb.clone());
    let bridge = SshBridge;
    let work = std::env::temp_dir().join(format!("pkg-sweng-real-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();

    // T1（E2E 路径件·真实现）：容器内真执行。
    let bash = sweng::EnvBash::new(Arc::new(SshBridge), Arc::new(tb.clone()));
    let out = bash.call("echo af-g4-t1", None);
    assert!(
        out.contains("af-g4-t1") && out.ends_with("exit=0"),
        "T1: {out}"
    );

    // T2 只读检索（pure）：/testbed 下真命中。
    let out = sweng::aci_search(&bridge, &tb, "def get_version", Some("django"), Some(5));
    assert!(out.contains("django/__init__.py:"), "T2 真命中：{out}");

    // T3 只读取片段（pure）：行号投影。
    let out = sweng::aci_open(&bridge, &tb, "django/__init__.py", Some(1), Some(3));
    assert!(out.contains("1\t"), "T3 行号制：{out}");

    // T5→T4→T9 链：新建 → 软对齐编辑 → 只读复核（persist/pure 逐件）。
    let scratch = "/tmp/af-g4-sweng-sampling.py";
    let _ = bash.call(&format!("rm -f {scratch}"), None);
    let out = sweng::aci_create(&bridge, &tb, scratch, "VALUE = 1\n");
    assert!(out.contains("OK"), "T5: {out}");
    let out = sweng::aci_edit(&bridge, &tb, scratch, "VALUE = 1", "VALUE = 2");
    assert!(out.contains("OK"), "T4: {out}");
    let out = sweng::sweng_view(&bridge, &tb, scratch, Some(1), Some(1));
    assert!(out.contains("VALUE = 2"), "T9 复核编辑生效：{out}");
    // T4 冲突臂：0 匹配真报错。
    let out = sweng::aci_edit(&bridge, &tb, scratch, "NOT_PRESENT", "x");
    assert!(out.contains("ERROR 0 matches"), "T4 冲突：{out}");
    // T5 已存在臂。
    let out = sweng::aci_create(&bridge, &tb, scratch, "x");
    assert!(out.contains("ERROR file exists"), "T5 已存在：{out}");
    // T10 sweng.edit 同契（另一文件）。
    let scratch2 = "/tmp/af-g4-sweng-sampling2.py";
    let _ = bash.call(
        &format!("rm -f {scratch2} && echo hello > {scratch2}"),
        None,
    );
    let out = sweng::sweng_edit(&bridge, &tb, scratch2, "hello", "world");
    assert!(out.contains("OK"), "T10: {out}");
    // T8 locate 真命中。
    let out = sweng::sweng_locate(&bridge, &tb, "VALUE = 2", Some("/tmp"), Some(10));
    assert!(out.contains("af-g4-sweng-sampling.py:"), "T8: {out}");

    // T6 测试执行（external_once）。
    let out = sweng::aci_run_test(
        &bridge,
        &tb,
        "python3 -V",
        Some("python3 -c 'import sys; sys.exit(0)'"),
        Some(60),
    );
    assert!(out.ends_with("exit=0"), "T6: {out}");

    // T7 提交：哨兵 + 取证（/testbed 未被上面 persist 件污染——写的是 /tmp）。
    let out = sweng::aci_submit(&bridge, &tb, &work, Some("g4 sampling"));
    assert_eq!(out.lines().next(), Some("TASK_COMPLETE"), "T7: {out}");
    assert!(work.join("agent.diff").is_file(), "T7 agent.diff 在盘");

    // T11 发布（本件无容器面；同一抽样内验真盘）。
    let out = sweng::publish_task_finished(&work, "fix", "sampled", None).unwrap();
    assert_eq!(out, "PUBLISHED fix-finish");

    // U3 判定单元：真容器退出码。
    let v = sweng::test_unit(
        &bridge,
        &tb,
        "python3 -V",
        Some("python3 -c 'raise SystemExit(3)'"),
    );
    assert_eq!(v["pass"], false, "U3 真退出码：{v}");

    // U2/U4 纯函数件（零容器）在同一抽样内各锁 1 例。
    let v = sweng::aci_schema(&json!({"thought": "t", "command": "c", "submit": true}));
    assert_eq!(v["submit"], true);
    let v = sweng::human_gate(&sweng::HumanGateConfig::default(), &json!({}));
    assert_eq!(v["ask"], false);

    std::fs::remove_dir_all(&work).ok();
}

/// 真容器抽样 · U1 swebench_eval 全链（E2E 路径件真实现）：指纹门 →
/// capture_diff → eval.sh 容器执行 1800s → 本机 judge 子进程。
#[test]
#[ignore = "真容器+真判分：django-11099 官方 eval（分钟级）；显式 -- --ignored 运行"]
fn real_swebench_eval_full_chain() {
    let tb = anolis_clear_or_panic();
    let _guard = RealGuard(tb.clone());
    let data_root = Path::new("/tmp/swev-s1");
    let task_json = data_root.join("django__django-11099.json");
    assert!(task_json.is_file(), "任务包在盘：{}", task_json.display());
    let tasks = swev::load_swev_tasks(data_root, &["django__django-11099".to_string()]).unwrap();
    let task = tasks.into_iter().next().unwrap();
    let work = std::env::temp_dir().join(format!("pkg-sweng-real-u1-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let out = sweng::swebench_eval(
        &SshBridge,
        &tb,
        &af_execline::JudgeLocator::production(),
        &task,
        &work,
        &task_json,
    );
    // 判据口径（设计 §4.4-4）：verdict ∈ {true,false} 非 null；eval_sha256 在盘。
    let resolved = out.verdict["resolved"].as_bool();
    assert!(
        resolved == Some(false) || resolved == Some(true),
        "独立判分必须有结论（真假均可，null=不通过）：{}",
        out.verdict
    );
    let sha = out.eval_sha256.expect("eval_sha256 在案");
    assert_eq!(sha.len(), 64);
    assert!(work.join("eval.log").is_file(), "三件套 eval.log 在盘");
    assert!(work.join("agent.diff").is_file(), "三件套 agent.diff 在盘");
    assert!(
        out.verdict.get("log_parsed") == Some(&json!(true))
            || out.verdict["log_parsed"].is_boolean(),
        "judge JSON 透传：{}",
        out.verdict
    );
    std::fs::remove_dir_all(&work).ok();
}
