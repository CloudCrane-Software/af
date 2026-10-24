//! 任务包 P2-F02 验收测试：af-order 下单门面 CLI 骨架。
//!
//! 全部经 std::process 调构建出的真二进制（`CARGO_BIN_EXE_af-order`），
//! 不新增测试框架。覆盖任务书四类验收：
//! ① submit 一命令走通全生命周期并落账（ledger 八 op / MES 四跃迁 / 工件 /
//!    ed25519 凭证可独立验签 / 种子 0600）；
//! ② list / show 正确读回；
//! ③ 重复 submit 同 order_id 幂等（不产生第二份终态、凭证不重签）；
//! ④ 坏 spec 被拒且错误信息含原因（OR 稳定码 + 点名字段）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use af_execline::{LedgerRecord, MesFileWriter, OrderLedger};
use af_workorder::{Ed25519PublicKey, Signature, StableId};

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
        "af-order-it-{}-{}-{}",
        std::process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

fn write_spec(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, text).expect("写 spec 文件");
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("af-order 二进制可执行")
}

fn submit(spec: &Path, state: &Path) -> Output {
    let root = workspace_root();
    run(&[
        "submit",
        "--spec",
        spec.to_str().expect("spec 路径可转 str"),
        "--state",
        state.to_str().expect("state 路径可转 str"),
        "--workspace",
        root.to_str().expect("workspace 路径可转 str"),
    ])
}

fn spec_text(order_id: &str, budget: u64) -> String {
    format!(
        r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":{budget},"note":"骨架验收"}}"#
    )
}

fn stdout_json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).expect("stdout 应是一个 JSON 对象")
}

fn hex_decode(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("合法十六进制"))
        .collect()
}

fn order_dir(state: &Path, order_id: &str) -> PathBuf {
    state.join("orders").join(order_id)
}

fn assert_contiguous(records: &[LedgerRecord], what: &str) {
    let seqs: Vec<u64> = records.iter().map(|record| record.seq).collect();
    let expected: Vec<u64> = (1..=records.len() as u64).collect();
    assert_eq!(seqs, expected, "{what} 的 ledger 序号必须从 1 连续");
}

// ---------- ① submit 全生命周期落账 ----------

#[test]
fn submit_full_lifecycle_lands_ledger_mes_voucher_and_artifact() {
    let state = scratch("submit-happy");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-1", 42));

    let out = submit(&spec, &state);
    assert!(
        out.status.success(),
        "submit 应成功\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let summary = stdout_json(&out);
    assert_eq!(summary["order_id"], "wo-cli-1");
    assert_eq!(summary["state"], "Closed");
    assert_eq!(summary["replayed"], false);
    assert_eq!(
        summary["stable_id"],
        StableId::generate("wo-cli-1").as_str(),
        "stable_id 与 af_workorder::StableId::generate 同规则"
    );
    assert!(
        summary["engine_bin"]
            .as_str()
            .expect("engine_bin 字段")
            .ends_with("af-engine"),
        "判定走真引擎子进程: {summary}"
    );

    // ledger：可重载、序号连续、八个 op 全部成功、close 唯一
    let dir = order_dir(&state, "wo-cli-1");
    let records = OrderLedger::load(&dir.join("ledger.jsonl")).expect("ledger 可重载");
    assert_contiguous(&records, "首驱");
    for op in [
        "create_order",
        "bind_line",
        "attach_voucher",
        "release",
        "start",
        "attach_artifact",
        "complete",
        "close",
    ] {
        assert!(
            records.iter().any(|record| record.op == op && record.ok),
            "ledger 缺成功记录 {op}"
        );
    }
    assert_eq!(
        records.iter().filter(|record| record.op == "close" && record.ok).count(),
        1,
        "close 成功记录唯一"
    );

    // MES：四类跃迁各一，work_order_id 收敛到 stable_id（ADR-004）
    let events = MesFileWriter::load(&dir.join("mes-events.jsonl")).expect("MES 事件可重载");
    let mut kinds: Vec<&str> = events.iter().map(|event| event.kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["close", "complete", "release", "start"]);
    assert!(events
        .iter()
        .all(|event| event.work_order_id == StableId::generate("wo-cli-1").as_str()));

    // 工件文件落盘
    assert!(
        dir.join("artifacts").join("fa-wo-cli-1.json").is_file(),
        "执行桩工件文件应在 orders/<id>/artifacts/ 下"
    );

    // 凭证：真 ed25519，测试内独立验签
    let voucher: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("voucher.json")).expect("凭证材料文件存在"),
    )
    .expect("凭证材料是 JSON");
    assert_eq!(voucher["voucher_id"], "bv-wo-cli-1");
    assert_eq!(voucher["amount"], 42);
    assert_eq!(voucher["algorithm"], "ed25519");
    let key_id = voucher["key_id"].as_str().expect("key_id 字段");
    let signature_hex = voucher["signature_hex"].as_str().expect("signature_hex 字段");
    assert_eq!(key_id.len(), 64, "公钥 32 字节小写十六进制");
    assert_eq!(signature_hex.len(), 128, "签名 64 字节小写十六进制");
    let public = Ed25519PublicKey::from_bytes(&hex_decode(key_id).try_into().expect("公钥 32 字节"))
        .expect("公钥在曲线上");
    let signature = Signature {
        algorithm: "ed25519".into(),
        key_id: key_id.to_string(),
        bytes: hex_decode(signature_hex),
    };
    public
        .verify(
            voucher["payload"].as_str().expect("payload 字段").as_bytes(),
            &signature,
        )
        .expect("凭证 ed25519 签名必须可独立验签");

    // 种子文件权限 0600
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(dir.join("voucher-key.seed"))
        .expect("种子文件存在")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "ed25519 种子必须 0600");
}

// ---------- ② list / show 读回 ----------

#[test]
fn list_and_show_read_back_orders() {
    let state = scratch("list-show");
    let state_str = state.to_str().expect("state 路径可转 str");

    // 空状态目录：list 不报错、输出为空
    let empty = run(&["list", "--state", state_str]);
    assert!(
        empty.status.success(),
        "{}",
        String::from_utf8_lossy(&empty.stderr)
    );
    assert!(
        String::from_utf8_lossy(&empty.stdout).trim().is_empty(),
        "空状态目录应列出零行"
    );

    for (order_id, budget) in [("wo-cli-a", 10), ("wo-cli-b", 20)] {
        let spec = write_spec(&state, &format!("{order_id}.json"), &spec_text(order_id, budget));
        let out = submit(&spec, &state);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // list：两行，各含 order_id / stable_id / state / ledger_ops
    let out = run(&["list", "--state", state_str]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("list 每行是 JSON"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["order_id"], "wo-cli-a", "按 order_id 排序稳定");
    assert_eq!(lines[1]["order_id"], "wo-cli-b");
    for line in &lines {
        assert_eq!(line["state"], "Closed");
        assert!(
            line["ledger_ops"].as_u64().expect("ledger_ops 字段") >= 8,
            "{line}"
        );
    }

    // show：ledger 摘要 + MES 事件
    let out = run(&["show", "wo-cli-a", "--state", state_str]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shown = stdout_json(&out);
    assert_eq!(shown["order_id"], "wo-cli-a");
    assert_eq!(
        shown["stable_id"],
        StableId::generate("wo-cli-a").as_str()
    );
    assert_eq!(shown["state"], "Closed");
    let ops = shown["ledger"]["ops"].as_array().expect("ops 数组");
    assert_eq!(
        ops.first().expect("非空")["op"],
        "create_order",
        "首个 op 是建单"
    );
    assert_eq!(ops.last().expect("非空")["op"], "close", "末个 op 是关单");
    for record in ops {
        assert!(
            record["ok"].as_bool().expect("ok 字段"),
            "快乐路径所有 op 应 ok: {record}"
        );
    }
    let mut kinds: Vec<&str> = shown["mes"]["events"]
        .as_array()
        .expect("mes events 数组")
        .iter()
        .map(|event| event["kind"].as_str().expect("kind 字段"))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["close", "complete", "release", "start"]);

    // 不存在的订单：非零退出 + stderr 点名订单号与稳定码
    let out = run(&["show", "wo-nope", "--state", state_str]);
    assert!(!out.status.success(), "查无此单必须失败");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("wo-nope"), "{stderr}");
    assert!(stderr.contains("OR2004"), "{stderr}");
}

// ---------- ③ 重复 submit 幂等 ----------

#[test]
fn resubmit_same_order_id_is_idempotent() {
    let state = scratch("idem");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-idem", 7));

    let first = submit(&spec, &state);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(stdout_json(&first)["replayed"], false, "首驱不是重放");

    let voucher_after_first =
        fs::read_to_string(order_dir(&state, "wo-cli-idem").join("voucher.json"))
            .expect("首驱后凭证存在");

    let second = submit(&spec, &state);
    assert!(
        second.status.success(),
        "重复 submit 同 order_id 应幂等成功: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let summary = stdout_json(&second);
    assert_eq!(summary["replayed"], true, "二次 submit 标记为重放");
    assert_eq!(summary["state"], "Closed", "终态不变");

    let dir = order_dir(&state, "wo-cli-idem");
    let records = OrderLedger::load(&dir.join("ledger.jsonl")).expect("ledger 可重载");
    assert_contiguous(&records, "重放后");
    assert_eq!(
        records.iter().filter(|record| record.op == "close" && record.ok).count(),
        1,
        "不产生第二份终态"
    );
    assert!(
        records.iter().any(|record| record.op == "replay_skip"),
        "重放应有 replay_skip 审计行"
    );
    assert_eq!(
        MesFileWriter::load(&dir.join("mes-events.jsonl"))
            .expect("MES 可重载")
            .len(),
        4,
        "重放不得重复发 MES 事件"
    );
    assert_eq!(
        fs::read_to_string(dir.join("voucher.json")).expect("二次 submit 后凭证仍存在"),
        voucher_after_first,
        "重复 submit 不重签凭证（读回原凭证）"
    );
}

// ---------- ④ 坏 spec 拒绝 + 用法错误 ----------

#[test]
fn bad_spec_is_rejected_with_reason() {
    let state = scratch("bad-spec");
    let state_str = state.to_str().expect("state 路径可转 str");

    let cases: &[(&str, &str)] = &[
        // 缺必填字段：点名 budget
        (
            r#"{"order_id":"wo-bad","line":{"line_id":"l","line_version":"v"}}"#,
            "budget",
        ),
        // 未知字段：点名 priority
        (
            r#"{"order_id":"wo-bad","line":{"line_id":"l","line_version":"v"},"budget":5,"priority":"High"}"#,
            "priority",
        ),
        // 路径不安全 order_id：点名 order_id
        (
            r#"{"order_id":"../escape","line":{"line_id":"l","line_version":"v"},"budget":5}"#,
            "order_id",
        ),
        // 预算必须 > 0：点名 budget
        (
            r#"{"order_id":"wo-bad","line":{"line_id":"l","line_version":"v"},"budget":0}"#,
            "budget",
        ),
        // line 子字段非空：点名 line
        (
            r#"{"order_id":"wo-bad","line":{"line_id":"","line_version":"v"},"budget":5}"#,
            "line",
        ),
        // 坏 JSON：原因含 JSON
        ("{ not json", "JSON"),
    ];
    for (index, (text, needle)) in cases.iter().enumerate() {
        let spec = write_spec(&state, &format!("bad-{index}.json"), text);
        let out = submit(&spec, &state);
        assert!(
            !out.status.success(),
            "坏 spec #{index} 必须非零退出"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(needle),
            "错误信息应含原因 {needle:?}: {stderr}"
        );
        assert!(stderr.contains("OR2002"), "spec 拒绝稳定码: {stderr}");
    }

    // 坏 spec 不得产生任何订单目录（校验先于建目录）
    assert!(
        !state.join("orders").exists(),
        "spec 被拒不得创建订单目录"
    );

    // spec 文件不存在：OR2001 + 路径
    let out = submit(&state.join("nope.json"), &state);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("OR2001"), "{stderr}");
    assert!(stderr.contains("nope.json"), "{stderr}");

    // 用法错误：无子命令 / submit 缺 --state → 退出码 2
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2), "用法错误退出码 2");
    let out = run(&["submit", "--spec", "x.json"]);
    assert_eq!(out.status.code(), Some(2), "缺 --state 是用法错误");
}
