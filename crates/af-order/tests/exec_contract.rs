//! 消债 B① 验收测试：submit 执行参数面 = 正式 CLI 契约。
//!
//! 设计（对齐补丁台账-第二阶段 #B 消除条件）：
//! - OrderSpec（JSON 文件）保持**身份极简**（order_id/line/budget/note）——
//!   单身与执行解耦，F-首单/二单的 7 行 spec 原样可复用；
//! - 执行面 = CLI 旗标声明（`--task/--profile/--data-root` 三件同给走真实
//!   SwevExecuteUnit；`--host/--max-turns/--spend-cap-cents` 预算硬顶；
//!   `--mes-ch` CH 目标声明，写侧在 af-execline 驱动缝，订单面零写侧逻辑）；
//! - **向后兼容钉死**：F-首单/F-第二单两条已归档命令逐字可解析（证据链
//!   可复现）；缺省值 = fix2 代际（anolis/250/$3）；
//! - 不给三件套 = stub 冒烟档（既有测试与冒烟面不变，明示非退役即标注）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};

use af_order::cli::{parse_args, Command};
use af_order::exec::{parse_exec_flags, swev_unit_spec_for, RealExec, DEFAULT_HOST};
use af_workorder::StableId;

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

/// F-首单已归档命令（F-首单-交付-20261024.md §3.1）逐字形态。
const F1_FLAGS: &[&str] = &[
    "--task",
    "django__django-11099",
    "--profile",
    "glm-cp",
    "--data-root",
    "/tmp/swev-s1",
    "--host",
    "anolis",
    "--max-turns",
    "60",
    "--spend-cap-cents",
    "100",
    "--mes-ch",
    "auto",
];

/// F-第二单已归档命令（F-第二单-交付-20261024.md §3.1）——仅 task/profile 与
/// 首单不同（psf/requests-1142 × step-5，OpenAI 协议档）。
const F2_FLAGS: &[&str] = &[
    "--task",
    "psf__requests-1142",
    "--profile",
    "step-5",
    "--data-root",
    "/tmp/swev-s1",
    "--host",
    "anolis",
    "--max-turns",
    "60",
    "--spend-cap-cents",
    "100",
    "--mes-ch",
    "auto",
];

fn submit_parsed(exec_flags: &[&str]) -> Command {
    let mut parts = vec![
        "submit".to_string(),
        "--spec".into(),
        "o.json".into(),
        "--state".into(),
        "st".into(),
    ];
    parts.extend(exec_flags.iter().map(|f| (*f).to_string()));
    parse_args(&parts).expect("应解析为 Submit")
}

fn submit_exec(exec_flags: &[&str]) -> Option<RealExec> {
    match submit_parsed(exec_flags) {
        Command::Submit { exec, .. } => exec,
        other => panic!("应解析为 Submit，实际 {other:?}"),
    }
}

fn submit_mes_ch(exec_flags: &[&str]) -> Option<String> {
    match submit_parsed(exec_flags) {
        Command::Submit { mes_ch, .. } => mes_ch,
        other => panic!("应解析为 Submit，实际 {other:?}"),
    }
}

/// F-首单命令逐字兼容：旗标名、取值、缺省全部与归档证据链一致。
/// （mes_ch 在正式契约中上移为 Submit 独立记账声明——语义与补丁期等价，
/// GREEN 实现时随契约归位调整断言位置。）
#[test]
fn f1_archived_command_parses_verbatim() {
    let exec = submit_exec(F1_FLAGS).expect("F1 命令应有真实执行面");
    assert_eq!(
        exec,
        RealExec {
            task: "django__django-11099".into(),
            profile: "glm-cp".into(),
            data_root: PathBuf::from("/tmp/swev-s1"),
            host: "anolis".into(),
            max_turns: 60,
            spend_cap_cents: 100,
        }
    );
    assert_eq!(submit_mes_ch(F1_FLAGS).as_deref(), Some("auto"));
}

/// F-第二单命令逐字兼容（换仓库换协议档全通——执行面与端点档案解耦）。
#[test]
fn f2_archived_command_parses_verbatim() {
    let exec = submit_exec(F2_FLAGS).expect("F2 命令应有真实执行面");
    assert_eq!(
        exec,
        RealExec {
            task: "psf__requests-1142".into(),
            profile: "step-5".into(),
            data_root: PathBuf::from("/tmp/swev-s1"),
            host: "anolis".into(),
            max_turns: 60,
            spend_cap_cents: 100,
        }
    );
    assert_eq!(submit_mes_ch(F2_FLAGS).as_deref(), Some("auto"));
}

/// 缺省 = fix2 代际（host anolis / 250 轮 / $3 / 无 CH 下游）。
#[test]
fn exec_defaults_are_fix2_generation() {
    let exec = submit_exec(&[
        "--task",
        "t-1",
        "--profile",
        "glm-cp",
        "--data-root",
        "/tmp/d",
    ])
    .expect("三件齐给即真实执行面");
    assert_eq!(exec.host, DEFAULT_HOST);
    assert_eq!(exec.host, "anolis");
    assert_eq!(exec.max_turns, 250);
    assert_eq!(exec.spend_cap_cents, 300);
    // --mes-ch 与执行面正交：不声明 = None
    assert_eq!(
        submit_mes_ch(&["--task", "t-1", "--profile", "p", "--data-root", "/d"]),
        None
    );
}

/// --mes-ch 单独出现合法（记账声明与执行面正交——冒烟档也要记账）。
#[test]
fn mes_ch_is_orthogonal_to_exec_face() {
    assert_eq!(
        submit_mes_ch(&["--mes-ch", "auto"]).as_deref(),
        Some("auto"),
        "冒烟档 + CH 记账应被接受"
    );
    assert!(submit_exec(&["--mes-ch", "auto"]).is_none());
}

/// 三件套全给或全不给（半套 = 用法错误 OR1001 / 退出码 2）。
#[test]
fn exec_trio_is_all_or_nothing() {
    for partial in [
        vec!["--task", "t-1"],
        vec!["--task", "t-1", "--profile", "glm-cp"],
        vec!["--profile", "glm-cp", "--data-root", "/tmp/d"],
        vec!["--data-root", "/tmp/d"],
    ] {
        let err = parse_args(&args(
            &[
                vec!["submit", "--spec", "o.json", "--state", "st"],
                partial.clone(),
            ]
            .concat(),
        ))
        .unwrap_err();
        assert!(
            err.to_string().contains("OR1001"),
            "半套 {:?} 应 OR1001: {err}",
            partial
        );
        assert_eq!(err.exit_code(), 2);
    }
    // 全不给 = None（冒烟档），且 --host 等执行附属旗标不得单独出现
    assert!(submit_exec(&[]).is_none());
    let err = parse_args(&args(&[
        "submit", "--spec", "o.json", "--state", "st", "--host", "anolis",
    ]))
    .unwrap_err();
    assert!(err.to_string().contains("OR1001"), "{err}");
}

/// 数值旗标非整数 = OR1001 点名。
#[test]
fn exec_numeric_flags_reject_garbage() {
    for bad in [
        vec![
            "--task",
            "t",
            "--profile",
            "p",
            "--data-root",
            "/d",
            "--max-turns",
            "six",
        ],
        vec![
            "--task",
            "t",
            "--profile",
            "p",
            "--data-root",
            "/d",
            "--spend-cap-cents",
            "1.5",
        ],
        vec![
            "--task",
            "t",
            "--profile",
            "p",
            "--data-root",
            "/d",
            "--max-turns",
            "-1",
        ],
    ] {
        let err = parse_args(&args(
            &[vec!["submit", "--spec", "o.json", "--state", "st"], bad].concat(),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("OR1001"), "{err}");
        assert_eq!(err.exit_code(), 2);
    }
}

/// 预算硬顶注入映射：SwevUnitSpec 以 exec 覆写 max_turns/spend_cap，
/// 判分锚 = 现产冻结链（a292547f…，指纹门 fail-loud）。
#[test]
fn swev_spec_mapping_overrides_budget_hats() {
    let exec = RealExec {
        task: "django__django-11099".into(),
        profile: "glm-cp".into(),
        data_root: PathBuf::from("/tmp/swev-s1"),
        host: "anolis".into(),
        max_turns: 60,
        spend_cap_cents: 100,
    };
    let spec = swev_unit_spec_for(&exec);
    assert_eq!(spec.profile, "glm-cp");
    assert_eq!(spec.max_turns, 60, "F-首单任务包口径覆写 fix2 缺省 250");
    assert_eq!(spec.spend_cap_cents, 100, "$1.00 硬顶覆写 fix2 缺省 $3");
    assert_eq!(
        spec.judge.script_sha256,
        "a292547f3bfce825b0b44997fd5cc145a5aa7ee89d8533729716c0275950617c"
    );
    // 缺省不覆写：fix2 代际原值
    let mut defaults = exec.clone();
    defaults.max_turns = 250;
    defaults.spend_cap_cents = 300;
    let spec = swev_unit_spec_for(&defaults);
    assert_eq!(spec.max_turns, 250);
    assert_eq!(spec.spend_cap_cents, 300);
}

/// parse_exec_flags 纯函数面：flag 表 → 执行面（供 run_submit 复用）。
#[test]
fn parse_exec_flags_matches_cli_path() {
    let flags = vec![
        ("--task", "t-9".to_string()),
        ("--profile", "kimi-k3".to_string()),
        ("--data-root", "/data".to_string()),
    ];
    let exec = parse_exec_flags(&flags).unwrap().expect("三件齐给");
    assert_eq!(exec.task, "t-9");
    assert_eq!(exec.profile, "kimi-k3");
    assert_eq!(exec.data_root, PathBuf::from("/data"));
    assert_eq!(exec.host, "anolis");
    assert!(parse_exec_flags(&[]).unwrap().is_none());
}

// ---------- CLI → 驱动缝端到端（订单面只声明，写侧在 af-execline） ----------

const BIN: &str = env!("CARGO_BIN_EXE_af-order");

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根可解析")
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "af-order-exec-it-{}-{}-{}",
        std::process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建 scratch");
    dir
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_submit(spec: &Path, state: &Path, extra: &[&str]) -> Output {
    let mut cmd = StdCommand::new(BIN);
    cmd.args([
        "submit",
        "--spec",
        spec.to_str().expect("spec 路径"),
        "--state",
        state.to_str().expect("state 路径"),
        "--workspace",
        workspace_root().to_str().expect("workspace 路径"),
    ]);
    for flag in extra {
        cmd.arg(flag);
    }
    cmd.output().expect("af-order 可执行")
}

/// `--mes-ch <mock>`：声明直达驱动缝——真引擎全生命周期 + mock CH 恰收
/// 4 行四类跃迁；重跑同单（重放）读侧对账零补送。订单面零 CH 写侧代码
/// （本测试只给目标字符串）。
#[test]
fn submit_mes_ch_declaration_flows_to_driver_seam() {
    let mock = af_mes::mock::MockClickHouse::start();
    let state = Scratch(scratch("mes-ch-it"));
    let spec = state.0.join("order.json");
    fs::write(
        &spec,
        r#"{"order_id":"wo-exec-mes-ch","line":{"line_id":"line-A","line_version":"1"},"budget":42}"#,
    )
    .unwrap();

    let out = run_submit(&spec, &state.0, &["--mes-ch", mock.url()]);
    assert!(
        out.status.success(),
        "首驱应全绿\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["state"], "Closed");
    assert_eq!(summary["mes_ch"]["kind"], "click_house");
    assert_eq!(summary["mes_ch"]["shipped"], 4, "{summary}");
    assert_eq!(summary["mes_ch"]["already_in_ch"], 0);
    assert_eq!(summary["mes_ch"]["rows_delivered"], 4);
    // 凭据零落回执：endpoint 只含 host:port
    assert_eq!(summary["mes_ch"]["endpoint_host"], mock_addr(&mock));

    let inserts = mock.inserts();
    let mut kinds: Vec<&str> = inserts
        .iter()
        .flat_map(|c| c.rows.iter().map(|r| r.kind.as_str()))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["close", "complete", "release", "start"]);

    // 重跑（重放）：SELECT 回放 4 行 → 读侧对账认行 → 零补送零双写
    let raw_all: Vec<u8> = mock
        .inserts()
        .iter()
        .flat_map(|c| c.raw_rowbinary.iter().copied())
        .collect();
    mock.enqueue_query_body(raw_all);
    let second = run_submit(&spec, &state.0, &["--mes-ch", mock.url()]);
    assert!(
        second.status.success(),
        "重放驱应全绿\nstderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let summary2: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(summary2["replayed"], true);
    assert_eq!(summary2["mes_ch"]["shipped"], 0, "{summary2}");
    assert_eq!(summary2["mes_ch"]["already_in_ch"], 4);
    let total: usize = mock
        .inserts()
        .iter()
        .filter(|c| c.accepted)
        .map(|c| c.rows.len())
        .sum();
    assert_eq!(total, 4, "CH 终态恰 4 行");
    // stable_id 口径：CH 行的 work_order_id 与本地派生一致
    assert!(mock.inserts().iter().all(|c| c
        .rows
        .iter()
        .all(|r| r.work_order_id == StableId::generate("wo-exec-mes-ch").as_str())));
}

fn mock_addr(mock: &af_mes::mock::MockClickHouse) -> String {
    mock.url().trim_start_matches("http://").to_string()
}

/// 坏目标 fail-loud：`--mes-ch` 指向不可解析目标（无 scheme）→ 非零退出 +
/// EX2003 稳定码透传（订单本体已 Closed，CH 记账未达显性上报）。
#[test]
fn submit_mes_ch_bad_target_fails_loud_ex2003() {
    let state = Scratch(scratch("mes-ch-bad"));
    let spec = state.0.join("order.json");
    fs::write(
        &spec,
        r#"{"order_id":"wo-exec-mes-bad","line":{"line_id":"line-A","line_version":"1"},"budget":7}"#,
    )
    .unwrap();
    let out = run_submit(&spec, &state.0, &["--mes-ch", "mingda-no-scheme:8123"]);
    assert!(!out.status.success(), "坏目标必须非零退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("EX2003"), "{stderr}");
}
