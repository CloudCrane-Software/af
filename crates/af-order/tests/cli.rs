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

/// 断言子命令成功；失败时把 stdout/stderr 全文带进断言消息。
fn expect_ok(out: &Output) {
    assert!(
        out.status.success(),
        "应成功退出\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------- ① submit 全生命周期落账 ----------

#[test]
fn submit_full_lifecycle_lands_ledger_mes_voucher_and_artifact() {
    let state = scratch("submit-happy");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-1", 42));

    let out = submit(&spec, &state);
    expect_ok(&out);
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
        records
            .iter()
            .filter(|record| record.op == "close" && record.ok)
            .count(),
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
    let signature_hex = voucher["signature_hex"]
        .as_str()
        .expect("signature_hex 字段");
    assert_eq!(key_id.len(), 64, "公钥 32 字节小写十六进制");
    assert_eq!(signature_hex.len(), 128, "签名 64 字节小写十六进制");
    let public =
        Ed25519PublicKey::from_bytes(&hex_decode(key_id).try_into().expect("公钥 32 字节"))
            .expect("公钥在曲线上");
    let signature = Signature {
        algorithm: "ed25519".into(),
        key_id: key_id.to_string(),
        bytes: hex_decode(signature_hex),
    };
    public
        .verify(
            voucher["payload"]
                .as_str()
                .expect("payload 字段")
                .as_bytes(),
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
    expect_ok(&empty);
    assert!(
        String::from_utf8_lossy(&empty.stdout).trim().is_empty(),
        "空状态目录应列出零行"
    );

    for (order_id, budget) in [("wo-cli-a", 10), ("wo-cli-b", 20)] {
        let spec = write_spec(
            &state,
            &format!("{order_id}.json"),
            &spec_text(order_id, budget),
        );
        let out = submit(&spec, &state);
        expect_ok(&out);
    }

    // list：两行，各含 order_id / stable_id / state / ledger_ops
    let out = run(&["list", "--state", state_str]);
    expect_ok(&out);
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
    expect_ok(&out);
    let shown = stdout_json(&out);
    assert_eq!(shown["order_id"], "wo-cli-a");
    assert_eq!(shown["stable_id"], StableId::generate("wo-cli-a").as_str());
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
    let stderr = stderr_of(&out);
    assert!(stderr.contains("wo-nope"), "{stderr}");
    assert!(stderr.contains("OR2004"), "{stderr}");
}

// ---------- ③ 重复 submit 幂等 ----------

#[test]
fn resubmit_same_order_id_is_idempotent() {
    let state = scratch("idem");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-idem", 7));

    let first = submit(&spec, &state);
    expect_ok(&first);
    assert_eq!(stdout_json(&first)["replayed"], false, "首驱不是重放");

    let voucher_after_first =
        fs::read_to_string(order_dir(&state, "wo-cli-idem").join("voucher.json"))
            .expect("首驱后凭证存在");

    let second = submit(&spec, &state);
    expect_ok(&second);
    let summary = stdout_json(&second);
    assert_eq!(summary["replayed"], true, "二次 submit 标记为重放");
    assert_eq!(summary["state"], "Closed", "终态不变");

    let dir = order_dir(&state, "wo-cli-idem");
    let records = OrderLedger::load(&dir.join("ledger.jsonl")).expect("ledger 可重载");
    assert_contiguous(&records, "重放后");
    assert_eq!(
        records
            .iter()
            .filter(|record| record.op == "close" && record.ok)
            .count(),
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
        assert!(!out.status.success(), "坏 spec #{index} 必须非零退出");
        let stderr = stderr_of(&out);
        assert!(
            stderr.contains(needle),
            "错误信息应含原因 {needle:?}: {stderr}"
        );
        assert!(stderr.contains("OR2002"), "spec 拒绝稳定码: {stderr}");
    }

    // 坏 spec 不得产生任何订单目录（校验先于建目录）
    assert!(!state.join("orders").exists(), "spec 被拒不得创建订单目录");

    // spec 文件不存在：OR2001 + 路径
    let out = submit(&state.join("nope.json"), &state);
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains("OR2001"), "{stderr}");
    assert!(stderr.contains("nope.json"), "{stderr}");

    // 用法错误：无子命令 / submit 缺 --state → 退出码 2
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2), "用法错误退出码 2");
    let out = run(&["submit", "--spec", "x.json"]);
    assert_eq!(out.status.code(), Some(2), "缺 --state 是用法错误");
}

// ---------- 红队修复批（W1）：C-41 / C-64 ----------

/// C-41：换 line 重放被拒——身份面字段与预算同纪律（OR2003）。
#[test]
fn resubmit_with_different_line_is_rejected() {
    let state = scratch("w1c41");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-w1c41", 7));
    let first = submit(&spec, &state);
    expect_ok(&first);

    // 同 order_id、换 line（预算不变）：凭证面 line 钉死 → OR2003 拒绝。
    let swapped = write_spec(
        &state,
        "order2.json",
        r#"{"order_id":"wo-cli-w1c41","line":{"line_id":"line-other","line_version":"lv-1"},"budget":7}"#,
    );
    let second = submit(&swapped, &state);
    assert!(
        !second.status.success(),
        "换 line 重放应拒绝：{}",
        String::from_utf8_lossy(&second.stdout)
    );
    let err_text = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );
    assert!(err_text.contains("OR2003"), "稳定码 OR2003：{err_text}");
    assert!(err_text.contains("line_id"), "点名字段：{err_text}");
}

/// C-64：stub 冒烟档 MES line_id 加 stub: 前缀——冒烟单与真单在 MES 账面
/// 可判别（此前逐字段同形，mes_line_hour_mv 会把冒烟产量计入产线）。
#[test]
fn stub_submit_marks_mes_line_with_stub_prefix() {
    let state = scratch("w1c64");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-w1c64", 7));
    let out = submit(&spec, &state);
    expect_ok(&out);
    let events = MesFileWriter::load(&order_dir(&state, "wo-cli-w1c64").join("mes-events.jsonl"))
        .expect("MES 可重载");
    assert_eq!(events.len(), 4);
    assert!(
        events.iter().all(|e| e.line_id == "stub:line-poc"),
        "冒烟档 MES line_id 应带 stub: 前缀：{:?}",
        events.iter().map(|e| e.line_id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn same_order_id_across_state_roots_gets_distinct_event_ids() {
    // 红队 W2-C53：同 order_id + 同 line 落在两个 --state 根时，event_id 必须
    // 带上运行实例判别量（订单目录规范路径哈希）——此前 event_id = stable_id
    // （order_id 纯函数）+ kind + seq，两单逐字节同 id，CH 侧按 event_id 去重
    // 的差量送批把第二单 4 条记账事件全部吞掉（shipped=0/already_in_ch=4）
    // 且零告警（C-63 的 line 冲突判据只覆盖异 line，不覆盖跨 state 根）。
    let root_a = scratch("xroot-a");
    let root_b = scratch("xroot-b");
    let spec = write_spec(&root_a, "order.json", &spec_text("wo-xroot", 5));

    let first = submit(&spec, &root_a);
    expect_ok(&first);
    let second = submit(&spec, &root_b);
    expect_ok(&second);

    let events = |root: &Path| -> Vec<String> {
        MesFileWriter::load(&order_dir(root, "wo-xroot").join("mes-events.jsonl"))
            .expect("MES 可重载")
            .into_iter()
            .map(|event| event.event_id)
            .collect()
    };
    let (a, b) = (events(&root_a), events(&root_b));
    assert_eq!(a.len(), 4, "A 根四类跃迁齐全");
    assert_eq!(b.len(), 4, "B 根四类跃迁齐全");
    assert!(
        a.iter().all(|id| !b.contains(id)),
        "跨 state 根同 order_id 的 event_id 不得相撞（相撞即差量互吞）：A={a:?} B={b:?}"
    );
    // 同根内四事件仍互异（kind/seq 判别不破）。
    assert_eq!(a.iter().collect::<std::collections::BTreeSet<_>>().len(), 4);
}

#[test]
fn resubmit_with_different_line_version_is_rejected() {
    // 红队 W2-C-35：W1-C41 的凭证 line 重放核对只钉 line_id 不钉 line_version
    // ——LineBinding 两字段同属身份面（spec check_line 两键都强制非空），
    // 重放时 bind_line 幂等跳过、订单保持首驱 line_version，本次只换
    // line_version 的声明被静默吞掉且零诊断。与换 line_id 同性质 fail-loud
    // （OR2003 点名 line_version）。旧行无此键不核对（历史盘兼容面不动）。
    let state = scratch("w2c35");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-w2c35", 7));
    expect_ok(&submit(&spec, &state));

    // 同 order_id、同 line_id、只换 line_version：凭证面两键同纪律 → OR2003。
    let swapped = write_spec(
        &state,
        "order2.json",
        r#"{"order_id":"wo-cli-w2c35","line":{"line_id":"line-poc","line_version":"lv-2"},"budget":7}"#,
    );
    let second = submit(&swapped, &state);
    assert!(
        !second.status.success(),
        "只换 line_version 的重放应拒绝：{}",
        String::from_utf8_lossy(&second.stdout)
    );
    let err_text = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );
    assert!(err_text.contains("OR2003"), "稳定码 OR2003：{err_text}");
    assert!(err_text.contains("line_version"), "点名字段：{err_text}");

    // 订单绑定保持首驱版本不被漂移（账面 line_version 仍 lv-1）。
    let voucher: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(order_dir(&state, "wo-cli-w2c35").join("voucher.json"))
            .expect("凭证可读"),
    )
    .expect("凭证是 JSON");
    assert_eq!(voucher["line_version"], "lv-1", "凭证钉首驱 line_version");
}

#[test]
fn show_rejects_path_unsafe_order_ids() {
    // 红队 W2-C39：show 的位置参数与 submit 共用 order_id 路径安全门——绝对
    // 路径（Path::join 替换 state 根）、`..` 穿越、分隔符在此拒尽，不得读出
    // state/orders/ 之外的账本（只读影响，但跨目录可读任意 ledger.jsonl）。
    let state = scratch("show-unsafe");
    for bad in ["../rogue", "/etc", "a/b", ".."] {
        let out = run(&["show", bad, "--state", state.to_str().unwrap()]);
        assert!(!out.status.success(), "show {bad:?} 应被拒");
        // 错误面走 stderr（main.rs 薄壳接线）。
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains("OR1001") && text.contains("目录名"),
            "show {bad:?} 应拒为用法错误（点名目录名约束）：{text}"
        );
        assert_eq!(out.status.code(), Some(2), "用法错误退出码 2");
    }
    // 对照：安全 id 照旧走 OrderNotFound（不误伤正常查询路径）。
    let ok = run(&["show", "rogue", "--state", state.to_str().unwrap()]);
    assert!(!ok.status.success(), "不存在的订单仍失败");
    let text = String::from_utf8_lossy(&ok.stderr);
    assert!(text.contains("OR2004"), "安全 id 走订单不存在：{text}");
}

/// 红队 W2-C36：重复旗标必须在参数解析期 OR1001 拒绝（退出码 2、stderr
/// 点名重复旗标），不得再静默取首个值——此前 `--state A --state B` exit 0
/// 且账本落 A、B 无声丢弃（--state 决定账本落点，静默选错目录尤其危险）。
#[test]
fn submit_rejects_duplicate_flags_as_usage_errors() {
    let state_a = scratch("dup-a");
    let state_b = scratch("dup-b");
    let spec = write_spec(&state_a, "dup.json", &spec_text("dup-flag-it", 1));
    let out = run(&[
        "submit",
        "--spec",
        spec.to_str().unwrap(),
        "--state",
        state_a.to_str().unwrap(),
        "--state",
        state_b.to_str().unwrap(),
        "--workspace",
        workspace_root().to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "重复 --state 应被拒");
    assert_eq!(out.status.code(), Some(2), "用法错误退出码 2");
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("OR1001") && text.contains("--state"),
        "应 OR1001 点名重复旗标 --state：{text}"
    );
    // 拒绝发生在参数解析期（凭证签发/目录创建之前）：两侧状态目录都不得
    // 留下订单目录。
    assert!(
        !order_dir(&state_a, "dup-flag-it").exists(),
        "拒绝不得留下订单目录（state A）"
    );
    assert!(
        !order_dir(&state_b, "dup-flag-it").exists(),
        "拒绝不得留下订单目录（state B）"
    );
}

// ---------- 红队修复批（W3）：W3-C-26 ----------

/// W3-C-26：`voucher.json` 损坏 = OR2003（凭证面），不是 OR2005（状态/账本面）。
/// RESULT.md 曾把该情形误写为 OR2005——实证：load_record 的读回/解析失败映射
/// `OrderError::Voucher`（voucher.rs），error.rs 格式化为 OR2003；OR2005 只属于
/// state::load_ledger 的账本路径。本例锁死码位，防止文书与代码再漂移。
#[test]
fn corrupted_voucher_reports_or2003_not_or2005() {
    let state = scratch("w3c26");
    let spec = write_spec(&state, "order.json", &spec_text("wo-cli-w3c26", 9));
    let first = submit(&spec, &state);
    expect_ok(&first);

    // 篡改盘上凭证为非法 JSON 后重放 submit。
    let voucher_path = order_dir(&state, "wo-cli-w3c26").join("voucher.json");
    fs::write(&voucher_path, "{ broken json").expect("篡改 voucher.json");

    let second = submit(&spec, &state);
    assert!(
        !second.status.success(),
        "凭证损坏必须拒绝，不得静默重建：{}",
        String::from_utf8_lossy(&second.stdout)
    );
    let err_text = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );
    assert!(err_text.contains("OR2003"), "稳定码 OR2003：{err_text}");
    assert!(err_text.contains("凭证损坏"), "点名凭证损坏：{err_text}");
    assert!(
        err_text.contains("voucher.json"),
        "点名损坏文件路径：{err_text}"
    );
    assert!(
        !err_text.contains("OR2005"),
        "凭证损坏不得误报 OR2005（那是账本面码位）：{err_text}"
    );
}
